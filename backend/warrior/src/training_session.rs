//! トレーニングの終了と、保存したサマリーの取得。
//!
//! 終了の時点でサーバーが D1 の種目、セット、段から今回のセッションのベストを計算し、
//! 過去の終了済みトレーニングのベストと比較して、更新した自己ベストを返す。
//! 今回のセッションのベストは `training_sessions.personal_bests` に保存する (ADR-015)。
//! 保存と読み直しは 1 つの `batch()` にまとめ、終了済みのトレーニングの終了は
//! 保存済みの値を返す (ADR-002)。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::jst::JstDate;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::{
    summary_from_sets, updated_personal_bests, PersonalBestItem, TrainingSessionSummary,
};
use worker::{console_error, Date, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::stored_sets;

#[derive(Debug, Deserialize)]
struct SessionSummaryRow {
    date: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    duration_seconds: Option<i64>,
    total_weight_kg: Option<f64>,
    exercise_count: Option<i64>,
    set_count: Option<i64>,
    personal_bests: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PersonalBestsRow {
    personal_bests: Option<String>,
}

const SESSION_SUMMARY_COLUMNS: &str = "date, started_at, finished_at, duration_seconds, \
     total_weight_kg, exercise_count, set_count, personal_bests";

/// `POST /api/trainings/:id/finish`
pub async fn finish(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(session_row) = load_session(&db, &training_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if session_row.finished_at.is_some() {
        return summary_response(&db, &session_row, &training_id, &authenticated.id).await;
    }
    let Some(started_at) = session_row.started_at.as_deref() else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the training has no start time",
        ));
    };
    let Some(started_seconds) = datetime::parse_utc(started_at) else {
        console_error!("the stored start time is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let Some(achieved_on) = JstDate::parse(&session_row.date) else {
        console_error!("the stored date is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let sets = match stored_sets::load_training(&db, &authenticated.id, &training_id).await {
        Ok(exercises) => stored_sets::session_sets(&exercises),
        Err(error) => {
            console_error!("failed to build the sets: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let now_seconds = current_seconds();
    let finished_at = datetime::format_utc(now_seconds);
    let duration_seconds = now_seconds.saturating_sub(started_seconds);
    let summary = summary_from_sets(achieved_on, duration_seconds, &sets);
    let personal_bests = match serde_json::to_string(&summary.personal_bests) {
        Ok(personal_bests) => personal_bests,
        Err(error) => {
            console_error!("failed to serialize the personal bests: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let statements = vec![
        finish_statement(
            &training_id,
            &authenticated.id,
            &finished_at,
            &summary,
            &personal_bests,
        ),
        select_summary_statement(&training_id, &authenticated.id),
    ];
    let results = match db.batch(&statements).await {
        Ok(results) => results,
        Err(error) => {
            console_error!("failed to save the summary: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let saved = match results.get(1) {
        Some(result) => result.results::<SessionSummaryRow>()?.into_iter().next(),
        None => None,
    };
    let Some(saved) = saved else {
        console_error!("the summary select returned no row");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    summary_response(&db, &saved, &training_id, &authenticated.id).await
}

/// `GET /api/trainings/:id/summary`
pub async fn summary(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(session_row) = load_session(&db, &training_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if session_row.finished_at.is_none() {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    summary_response(&db, &session_row, &training_id, &authenticated.id).await
}

fn current_seconds() -> i64 {
    (Date::now().as_millis() / 1_000) as i64
}

async fn load_session(
    db: &Db,
    training_id: &str,
    user_id: &str,
) -> Result<Option<SessionSummaryRow>> {
    db.first(&select_summary_statement(training_id, user_id))
        .await
}

fn select_summary_statement(training_id: &str, user_id: &str) -> SqlStatement {
    SqlStatement::new(
        format!(
            "SELECT {SESSION_SUMMARY_COLUMNS} FROM training_sessions WHERE id = ? AND user_id = ?"
        ),
        vec![
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

fn finish_statement(
    training_id: &str,
    user_id: &str,
    finished_at: &str,
    summary: &TrainingSessionSummary,
    personal_bests: &str,
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE training_sessions SET finished_at = ?, duration_seconds = ?, \
         total_weight_kg = ?, exercise_count = ?, set_count = ?, personal_bests = ? \
         WHERE id = ? AND user_id = ? AND finished_at IS NULL",
        vec![
            SqlValue::Text(finished_at.to_owned()),
            SqlValue::Integer(summary.duration_seconds),
            SqlValue::Real(summary.total_weight_kg),
            SqlValue::Integer(i64::from(summary.exercise_count)),
            SqlValue::Integer(i64::from(summary.set_count)),
            SqlValue::Text(personal_bests.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

async fn summary_response(
    db: &Db,
    row: &SessionSummaryRow,
    training_id: &str,
    user_id: &str,
) -> Result<Response> {
    let Some(summary) = stored_summary(row) else {
        console_error!("the stored summary is incomplete");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let finished_at = row.finished_at.as_deref().unwrap_or_default();
    let past = past_personal_bests(db, user_id, training_id, finished_at).await?;
    let personal_bests = updated_personal_bests(&summary.personal_bests, &past);
    Response::from_json(&TrainingSessionSummary {
        duration_seconds: summary.duration_seconds,
        total_weight_kg: summary.total_weight_kg,
        exercise_count: summary.exercise_count,
        set_count: summary.set_count,
        personal_bests,
    })
}

fn stored_summary(row: &SessionSummaryRow) -> Option<TrainingSessionSummary> {
    Some(TrainingSessionSummary {
        duration_seconds: row.duration_seconds?,
        total_weight_kg: row.total_weight_kg?,
        exercise_count: u32::try_from(row.exercise_count?).ok()?,
        set_count: u32::try_from(row.set_count?).ok()?,
        personal_bests: serde_json::from_str::<Vec<PersonalBestItem>>(
            row.personal_bests.as_deref()?,
        )
        .ok()?,
    })
}

async fn past_personal_bests(
    db: &Db,
    user_id: &str,
    training_id: &str,
    finished_at: &str,
) -> Result<Vec<PersonalBestItem>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT personal_bests FROM training_sessions \
             WHERE user_id = ? AND id <> ? AND finished_at IS NOT NULL AND finished_at < ? \
             AND personal_bests IS NOT NULL",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(finished_at.to_owned()),
            ],
        ))
        .await?;
    let rows = result.results::<PersonalBestsRow>()?;
    let mut items = Vec::new();
    for row in rows {
        let Some(personal_bests) = row.personal_bests else {
            continue;
        };
        let stored =
            serde_json::from_str::<Vec<PersonalBestItem>>(&personal_bests).map_err(|error| {
                worker::Error::RustError(format!("the stored personal bests are invalid: {error}"))
            })?;
        items.extend(stored);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use warrior_core::training_session::{BestKind, TrainingSessionSummary};

    use super::{finish_statement, stored_summary, SessionSummaryRow};

    #[test]
    fn the_finish_batch_scopes_the_update_to_the_user_and_an_unfinished_session() {
        let statement = finish_statement(
            "3f2504e0-4f89-41d3-9a0c-0305e82c6601",
            "3f2504e0-4f89-41d3-9a0c-0305e82c6602",
            "2026-11-12T04:00:00Z",
            &TrainingSessionSummary {
                duration_seconds: 3_600,
                total_weight_kg: 1_640.0,
                exercise_count: 2,
                set_count: 3,
                personal_bests: Vec::new(),
            },
            "[]",
        );
        assert!(statement
            .sql()
            .contains("WHERE id = ? AND user_id = ? AND finished_at IS NULL"));
        assert_eq!(statement.bindings().len(), 8);
        assert_eq!(
            statement.bindings()[6],
            warrior_core::sql::SqlValue::Text("3f2504e0-4f89-41d3-9a0c-0305e82c6601".to_owned())
        );
    }

    #[test]
    fn a_stored_summary_round_trips_through_the_row() {
        let row = SessionSummaryRow {
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            finished_at: Some("2026-11-12T04:00:00Z".to_owned()),
            duration_seconds: Some(3_600),
            total_weight_kg: Some(1_640.0),
            exercise_count: Some(2),
            set_count: Some(3),
            personal_bests: Some(
                r#"[{"exercise_id":"bench","exercise_name":"ベンチプレス","record_mode":"weight_and_reps","laterality":"bilateral","grip":null,"attachment":null,"kind":"max_weight","value":100.0}]"#
                    .to_owned(),
            ),
        };
        let summary = stored_summary(&row).expect("a stored summary");
        assert_eq!(summary.duration_seconds, 3_600);
        assert_eq!(summary.total_weight_kg, 1_640.0);
        assert_eq!(summary.exercise_count, 2);
        assert_eq!(summary.set_count, 3);
        assert_eq!(summary.personal_bests[0].kind, BestKind::MaxWeight);
        assert_eq!(summary.personal_bests[0].value, 100.0);
    }

    #[test]
    fn an_unfinished_summary_row_has_no_summary() {
        let row = SessionSummaryRow {
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            finished_at: None,
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: None,
            personal_bests: None,
        };
        assert_eq!(stored_summary(&row), None);
    }
}
