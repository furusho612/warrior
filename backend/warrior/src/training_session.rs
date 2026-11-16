//! トレーニングの終了と、保存したサマリーの取得。
//!
//! 終了の時点でサーバーが D1 の種目、セット、段から今回のセッションのベストを計算する。
//! そのベストを `personal_bests` の表へ書き、このトレーニングが保持するものだけを
//! サマリーとして返す。`training_sessions` は自己ベストを二重に保存しない。
//! 保存と読み直しは 1 つの `batch()` にまとめ、終了済みのトレーニングの終了は
//! 保存済みの値を返す (ADR-002)。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::jst::JstDate;
use warrior_core::ownership::{decide_write, WriteDecision};
use warrior_core::personal_best::candidates;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::{
    summary_from_sets, PersonalBestItem, SessionSet, TrainingSessionSummary,
};
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
    program_day_id: Option<String>,
    custom_program_day_id: Option<String>,
    duration_seconds: Option<i64>,
    total_weight_kg: Option<f64>,
    exercise_count: Option<i64>,
    set_count: Option<i64>,
}

const SESSION_SUMMARY_COLUMNS: &str = "date, started_at, finished_at, owner_terminal_id, \
     program_day_id, custom_program_day_id, duration_seconds, total_weight_kg, \
     exercise_count, set_count";

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
        return saved_summary_response(&db, &authenticated.id, &training_id, &session_row).await;
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
    let sets = match load_sets(&db, &authenticated.id, &training_id).await {
        Ok(sets) => sets,
        Err(error) => {
            console_error!("failed to build the sets: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let now_seconds = current_seconds();
    let finished_at = datetime::format_utc(now_seconds);
    let duration_seconds = now_seconds.saturating_sub(started_seconds);
    let summary = summary_from_sets(achieved_on, duration_seconds, &sets);
    let personal_bests = match apply_personal_bests(
        &db,
        &authenticated.id,
        &training_id,
        achieved_on,
        &sets,
    )
    .await
    {
        Ok(personal_bests) => personal_bests,
        Err(error) => {
            console_error!("failed to update the personal bests: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let saved = match save_summary(
        &db,
        &training_id,
        &authenticated.id,
        &terminal_id,
        &finished_at,
        &summary,
    )
    .await
    {
        Ok(Some(saved)) => saved,
        Ok(None) => {
            return reject_or_return_saved_summary(&db, &training_id, &authenticated.id).await
        }
        Err(error) => {
            console_error!("failed to save the summary: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if let Err(error) =
        crate::ai_job::create_session_jobs(&db, &authenticated.id, &training_id, &saved.date, &sets)
            .await
    {
        console_error!("failed to create the AI generation rows: {error}");
    }
    if let Err(error) =
        crate::rest_push::cancel_training_reservations(&db, &authenticated.id, &training_id).await
    {
        console_error!("failed to cancel the rest push reservations: {error}");
    }
    let advanced = if let Some(program_day_id) = saved.program_day_id.as_deref() {
        crate::program::advance_progress(
            &db,
            &authenticated.id,
            program_day_id,
            saved.custom_program_day_id.as_deref(),
        )
        .await
    } else if let Some(custom_day_id) = saved.custom_program_day_id.as_deref() {
        crate::custom_program::advance_progress(&db, &authenticated.id, custom_day_id).await
    } else {
        Ok(())
    };
    if let Err(error) = advanced {
        console_error!("failed to advance the program progress: {error}");
    }
    summary_response(&saved, personal_bests)
}

async fn load_sets(db: &Db, user_id: &str, training_id: &str) -> Result<Vec<SessionSet>> {
    let exercises = stored_sets::load_training(db, user_id, training_id).await?;
    Ok(stored_sets::session_sets(&exercises))
}

/// 候補を保存し、このトレーニングが保持する自己ベストを返す。
async fn apply_personal_bests(
    db: &Db,
    user_id: &str,
    training_id: &str,
    achieved_on: JstDate,
    sets: &[SessionSet],
) -> Result<Vec<PersonalBestItem>> {
    let candidates = candidates(achieved_on, sets);
    crate::personal_best::apply_candidates(db, user_id, training_id, achieved_on, &candidates)
        .await?;
    crate::personal_best::held_for_training(db, user_id, training_id, &candidates).await
}

/// サマリーを保存し、保存した行を返す。ほかの端末が先に終了したときは None。
async fn save_summary(
    db: &Db,
    training_id: &str,
    user_id: &str,
    terminal_id: &str,
    finished_at: &str,
    summary: &TrainingSessionSummary,
) -> Result<Option<SessionSummaryRow>> {
    let statements = vec![
        finish_statement(training_id, user_id, terminal_id, finished_at, summary),
        select_summary_statement(training_id, user_id),
    ];
    let results = db.batch(&statements).await?;
    let finish_result = results.first().ok_or_else(|| {
        worker::Error::RustError("the finish batch returned no result".to_owned())
    })?;
    if changed_rows(finish_result)? == 0 {
        return Ok(None);
    }
    let saved = results
        .get(1)
        .ok_or_else(|| worker::Error::RustError("the summary select is missing".to_owned()))?
        .results::<SessionSummaryRow>()?
        .into_iter()
        .next();
    saved
        .map(Some)
        .ok_or_else(|| worker::Error::RustError("the summary select returned no row".to_owned()))
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
            saved_summary_response(db, user_id, training_id, &row).await
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
    saved_summary_response(&db, &authenticated.id, &training_id, &session_row).await
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
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE training_sessions SET finished_at = ?, duration_seconds = ?, \
         total_weight_kg = ?, exercise_count = ?, set_count = ?, \
         owner_terminal_id = NULL \
         WHERE id = ? AND user_id = ? AND finished_at IS NULL \
         AND (owner_terminal_id IS NULL OR owner_terminal_id = ?)",
        vec![
            SqlValue::Text(finished_at.to_owned()),
            SqlValue::Integer(summary.duration_seconds),
            SqlValue::Real(summary.total_weight_kg),
            SqlValue::Integer(i64::from(summary.exercise_count)),
            SqlValue::Integer(i64::from(summary.set_count)),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(terminal_id.to_owned()),
        ],
    )
}

async fn saved_summary_response(
    db: &Db,
    user_id: &str,
    training_id: &str,
    row: &SessionSummaryRow,
) -> Result<Response> {
    let personal_bests = match load_personal_bests(db, user_id, training_id, &row.date).await {
        Ok(personal_bests) => personal_bests,
        Err(error) => {
            console_error!("failed to read the personal bests: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    summary_response(row, personal_bests)
}

async fn load_personal_bests(
    db: &Db,
    user_id: &str,
    training_id: &str,
    date: &str,
) -> Result<Vec<PersonalBestItem>> {
    let achieved_on = JstDate::parse(date).ok_or_else(|| {
        worker::Error::RustError("the stored training date is invalid".to_owned())
    })?;
    let sets = load_sets(db, user_id, training_id).await?;
    let candidates = candidates(achieved_on, &sets);
    crate::personal_best::held_for_training(db, user_id, training_id, &candidates).await
}

fn summary_response(
    row: &SessionSummaryRow,
    personal_bests: Vec<PersonalBestItem>,
) -> Result<Response> {
    let Some(summary) = stored_summary(row, personal_bests) else {
        console_error!("the stored summary is incomplete");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    Response::from_json(&summary)
}

fn stored_summary(
    row: &SessionSummaryRow,
    personal_bests: Vec<PersonalBestItem>,
) -> Option<TrainingSessionSummary> {
    Some(TrainingSessionSummary {
        date: JstDate::parse(&row.date)?,
        duration_seconds: row.duration_seconds?,
        total_weight_kg: row.total_weight_kg?,
        exercise_count: u32::try_from(row.exercise_count?).ok()?,
        set_count: u32::try_from(row.set_count?).ok()?,
        personal_bests,
    })
}

#[cfg(test)]
mod tests {
    use warrior_core::exercise::Laterality;
    use warrior_core::jst::JstDate;
    use warrior_core::set_calc::RecordMode;
    use warrior_core::training_session::{BestKind, PersonalBestItem, TrainingSessionSummary};

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
            program_day_id: None,
            custom_program_day_id: None,
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: None,
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
                date: JstDate::parse("2026-11-12").expect("a valid date"),
                duration_seconds: 3_600,
                total_weight_kg: 1_640.0,
                exercise_count: 2,
                set_count: 3,
                personal_bests: Vec::new(),
            },
        );
        assert!(statement.sql().contains("owner_terminal_id = NULL"));
        assert!(statement
            .sql()
            .contains("WHERE id = ? AND user_id = ? AND finished_at IS NULL"));
        assert!(statement
            .sql()
            .contains("(owner_terminal_id IS NULL OR owner_terminal_id = ?)"));
        assert!(!statement.sql().contains("personal_bests"));
        assert_eq!(statement.bindings().len(), 8);
        assert_eq!(
            statement.bindings()[5],
            warrior_core::sql::SqlValue::Text("3f2504e0-4f89-41d3-9a0c-0305e82c6601".to_owned())
        );
        assert_eq!(
            statement.bindings()[7],
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
            program_day_id: None,
            custom_program_day_id: None,
            duration_seconds: Some(3_600),
            total_weight_kg: Some(1_640.0),
            exercise_count: Some(2),
            set_count: Some(3),
        };
        let personal_bests = vec![PersonalBestItem {
            exercise_id: "bench".to_owned(),
            exercise_name: Some("ベンチプレス".to_owned()),
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip: None,
            attachment: None,
            kind: BestKind::MaxWeight,
            value: 100.0,
        }];
        let summary = stored_summary(&row, personal_bests).expect("a stored summary");
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
            program_day_id: None,
            custom_program_day_id: None,
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: None,
        };
        assert_eq!(stored_summary(&row, Vec::new()), None);
    }
}
