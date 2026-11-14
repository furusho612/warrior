//! トレーニングの終了と、保存したサマリーの取得。
//!
//! 終了の時点でサーバーが D1 の種目、セット、段から今回のセッションのベストを計算する。
//! そのベストを `personal_bests` の表へ書き、このトレーニングで更新したものだけを
//! `training_sessions.personal_bests` に保存して画面へ返す。
//! 保存と読み直しは 1 つの `batch()` にまとめ、終了済みのトレーニングの終了は
//! 保存済みの値を返す (ADR-002)。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::jst::JstDate;
use warrior_core::ownership::{decide_write, WriteDecision};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::{summary_from_sets, PersonalBestItem, TrainingSessionSummary};
use worker::{console_error, Date, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::{changed_rows, Db};
use crate::error::{response, ErrorCode};
use crate::ownership;
use crate::stored_sets;

#[derive(Debug, Deserialize)]
struct SessionSummaryRow {
    date: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    owner_terminal_id: Option<String>,
    duration_seconds: Option<i64>,
    total_weight_kg: Option<f64>,
    exercise_count: Option<i64>,
    set_count: Option<i64>,
    personal_bests: Option<String>,
}

const SESSION_SUMMARY_COLUMNS: &str = "date, started_at, finished_at, owner_terminal_id, \
     duration_seconds, total_weight_kg, exercise_count, set_count, personal_bests";

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
        return summary_response(&session_row);
    }
    let Some(terminal_id) = ownership::ensure(&db, &request, &authenticated.id).await? else {
        return Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        ));
    };
    if decide_write(
        false,
        session_row.owner_terminal_id.as_deref(),
        &terminal_id,
    ) == WriteDecision::Forbid
    {
        return Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        ));
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
    let candidates = warrior_core::personal_best::candidates(achieved_on, &sets);
    if let Err(error) = crate::personal_best::apply_candidates(
        &db,
        &authenticated.id,
        &training_id,
        achieved_on,
        &candidates,
    )
    .await
    {
        console_error!("failed to save the personal bests: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    let personal_bests = match crate::personal_best::updated_for_training(
        &db,
        &authenticated.id,
        &training_id,
        &summary.personal_bests,
    )
    .await
    {
        Ok(personal_bests) => personal_bests,
        Err(error) => {
            console_error!("failed to read the updated personal bests: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let personal_bests = match serde_json::to_string(&personal_bests) {
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
            &terminal_id,
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
    let Some(finish_result) = results.first() else {
        console_error!("the finish batch returned no result");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let changed = match changed_rows(finish_result) {
        Ok(changed) => changed,
        Err(error) => {
            console_error!("failed to read the changed rows: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if changed == 0 {
        return reject_or_return_saved_summary(&db, &training_id, &authenticated.id).await;
    }
    let saved = match results.get(1) {
        Some(result) => result.results::<SessionSummaryRow>()?.into_iter().next(),
        None => None,
    };
    let Some(saved) = saved else {
        console_error!("the summary select returned no row");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    summary_response(&saved)
}

/// 終了の更新が 0 行だったときの、再取得した行からの応答。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinishAfterBatch {
    /// ほかの端末が終了済みである。保存済みのサマリーを返す。
    Saved,
    /// 未終了である。この端末は編集権を失った。
    Forbidden,
    /// トレーニングが無い。
    Missing,
}

fn finish_after_batch(row: Option<&SessionSummaryRow>) -> FinishAfterBatch {
    match row {
        None => FinishAfterBatch::Missing,
        Some(row) if row.finished_at.is_some() => FinishAfterBatch::Saved,
        Some(_) => FinishAfterBatch::Forbidden,
    }
}

/// 終了の更新が 0 行だったとき、終了済みなら保存済みのサマリーを返し、未終了なら拒否する。
async fn reject_or_return_saved_summary(
    db: &Db,
    training_id: &str,
    user_id: &str,
) -> Result<Response> {
    let current = load_session(db, training_id, user_id).await?;
    match finish_after_batch(current.as_ref()) {
        FinishAfterBatch::Saved => {
            let Some(row) = current else {
                return Ok(response(ErrorCode::Internal, "internal error"));
            };
            summary_response(&row)
        }
        FinishAfterBatch::Forbidden => {
            // 判定の後、更新の前にほかの端末が編集権を得た。
            Ok(response(
                ErrorCode::Forbidden,
                "the training is being edited on another terminal",
            ))
        }
        FinishAfterBatch::Missing => Ok(response(ErrorCode::NotFound, "resource not found")),
    }
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
    summary_response(&session_row)
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
    terminal_id: &str,
    finished_at: &str,
    summary: &TrainingSessionSummary,
    personal_bests: &str,
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE training_sessions SET finished_at = ?, duration_seconds = ?, \
         total_weight_kg = ?, exercise_count = ?, set_count = ?, personal_bests = ?, \
         owner_terminal_id = NULL \
         WHERE id = ? AND user_id = ? AND finished_at IS NULL \
         AND (owner_terminal_id IS NULL OR owner_terminal_id = ?)",
        vec![
            SqlValue::Text(finished_at.to_owned()),
            SqlValue::Integer(summary.duration_seconds),
            SqlValue::Real(summary.total_weight_kg),
            SqlValue::Integer(i64::from(summary.exercise_count)),
            SqlValue::Integer(i64::from(summary.set_count)),
            SqlValue::Text(personal_bests.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(terminal_id.to_owned()),
        ],
    )
}

fn summary_response(row: &SessionSummaryRow) -> Result<Response> {
    let Some(summary) = stored_summary(row) else {
        console_error!("the stored summary is incomplete");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    Response::from_json(&summary)
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

#[cfg(test)]
mod tests {
    use warrior_core::training_session::{BestKind, TrainingSessionSummary};

    use super::{
        finish_after_batch, finish_statement, stored_summary, FinishAfterBatch, SessionSummaryRow,
    };

    #[test]
    fn a_zero_row_finish_returns_the_saved_summary_or_rejects_an_unfinished_training() {
        let mut row = SessionSummaryRow {
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            finished_at: None,
            owner_terminal_id: Some("another-terminal".to_owned()),
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: None,
            personal_bests: None,
        };
        assert_eq!(finish_after_batch(Some(&row)), FinishAfterBatch::Forbidden);
        row.finished_at = Some("2026-11-12T04:00:00Z".to_owned());
        assert_eq!(finish_after_batch(Some(&row)), FinishAfterBatch::Saved);
        assert_eq!(finish_after_batch(None), FinishAfterBatch::Missing);
    }

    #[test]
    fn the_finish_batch_scopes_the_update_to_the_user_the_owner_and_an_unfinished_session() {
        let statement = finish_statement(
            "3f2504e0-4f89-41d3-9a0c-0305e82c6601",
            "3f2504e0-4f89-41d3-9a0c-0305e82c6602",
            "3f2504e0-4f89-41d3-9a0c-0305e82c6603",
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
        assert!(statement.sql().contains("owner_terminal_id = NULL"));
        assert!(statement
            .sql()
            .contains("WHERE id = ? AND user_id = ? AND finished_at IS NULL"));
        assert!(statement
            .sql()
            .contains("(owner_terminal_id IS NULL OR owner_terminal_id = ?)"));
        assert_eq!(statement.bindings().len(), 9);
        assert_eq!(
            statement.bindings()[6],
            warrior_core::sql::SqlValue::Text("3f2504e0-4f89-41d3-9a0c-0305e82c6601".to_owned())
        );
        assert_eq!(
            statement.bindings()[8],
            warrior_core::sql::SqlValue::Text("3f2504e0-4f89-41d3-9a0c-0305e82c6603".to_owned())
        );
    }

    #[test]
    fn a_stored_summary_round_trips_through_the_row() {
        let row = SessionSummaryRow {
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            finished_at: Some("2026-11-12T04:00:00Z".to_owned()),
            owner_terminal_id: None,
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
            owner_terminal_id: None,
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: None,
            personal_bests: None,
        };
        assert_eq!(stored_summary(&row), None);
    }
}
