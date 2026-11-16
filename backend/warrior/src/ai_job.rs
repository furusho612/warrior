//! AI の生成ジョブの Queue の消費、Cron の投入、状態の取得 (ADR-021)。
//!
//! Queue のメッセージには生成の ID だけを入れ、記録の本文を入れない。
//! 消費側は D1 の行が `enqueued` で lease の期限内、または `pending` で再試行の時刻を
//! 過ぎている場合だけ処理する。AI の入出力は R2 に保存し、D1 にはメタデータだけを置く
//! (ADR-017)。ログには記録の本文、メモ、AI の入出力、トークン、購読の情報を出さない。

mod consumer;
mod context;

use serde::{Deserialize, Serialize};
use warrior_core::ai_job::{
    daily_evaluation_key, lease_deadline, set_proposal_key, JobKind, JobStatus,
};
use warrior_core::datetime;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::SessionSet;
use worker::{console_error, Date, Env, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::{changed_rows, Db};
use crate::error::{response, ErrorCode};
use crate::log::{self, AiJobLog};

const QUEUE_BINDING: &str = "JOBS_QUEUE";

const ERROR_CONFIG: &str = "config";
const ERROR_INPUT: &str = "input_unavailable";
const ERROR_INVALID_OUTPUT: &str = "invalid_output";
const ERROR_R2: &str = "r2_unavailable";
const ERROR_INVALID_JOB: &str = "invalid_job";
const ERROR_TIMEOUT: &str = "timeout";
const ERROR_RATE_LIMITED: &str = "http_429";
const ERROR_SERVER: &str = "http_5xx";
const ERROR_CLIENT: &str = "http_4xx";
const ERROR_FREE_TIER: &str = "free_tier_exceeded";
const ERROR_AI: &str = "ai_error";
const ERROR_QUEUE: &str = "queue_unavailable";

/// Queue に送るメッセージ。生成の ID だけを持つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobMessage {
    /// 生成の ID。
    pub generation_id: String,
}

/// 生成の行の読み取りに使う列。
const JOB_COLUMNS: &str =
    "id, user_id, kind, training_id, exercise_id, status, attempts, lease_expires_at, next_attempt_at";

/// 生成の行。
#[derive(Debug, Deserialize)]
struct JobRow {
    id: String,
    user_id: String,
    kind: String,
    training_id: Option<String>,
    exercise_id: Option<String>,
    status: String,
    attempts: u32,
    lease_expires_at: Option<String>,
    next_attempt_at: Option<String>,
}

/// 状態の応答に使う行。
#[derive(Debug, Deserialize)]
struct StatusRow {
    id: String,
    kind: String,
    status: String,
    attempts: u32,
}

/// `GET /api/ai/jobs/:id` の応答。
#[derive(Debug, Serialize)]
struct JobStatusResponse {
    id: String,
    kind: &'static str,
    status: &'static str,
    attempts: u32,
}

/// Cron が投入する `pending` の行。
#[derive(Debug, Deserialize)]
struct PendingRow {
    id: String,
    user_id: String,
    kind: String,
    attempts: u32,
}

/// Queue の 1 つのメッセージを処理する。
///
/// 呼び出し側は [crate::queue] がメッセージの種別を判定して呼ぶ。
pub async fn consume_one(env: &Env, generation_id: &str, correlation_id: &str) -> Result<()> {
    let started = Date::now().as_millis();
    if let Err(error) = consumer::process_message(env, generation_id, correlation_id).await {
        console_error!("failed to process the AI job {generation_id}: {error}");
        log_job(
            "unknown",
            generation_id,
            0,
            "error",
            elapsed_ms(started),
            Some("internal"),
            correlation_id,
        );
    }
    Ok(())
}

fn log_job(
    kind: &str,
    generation_id: &str,
    attempt: u32,
    result: &str,
    duration_ms: u64,
    error_code: Option<&str>,
    correlation_id: &str,
) {
    log::log_ai_job(&AiJobLog::new(
        kind,
        generation_id,
        attempt,
        result,
        duration_ms,
        error_code,
        correlation_id,
    ));
}

fn log_skipped(
    kind: &str,
    generation_id: &str,
    attempt: u32,
    result: &str,
    started: u64,
    correlation_id: &str,
    error_code: Option<&str>,
) {
    log_job(
        kind,
        generation_id,
        attempt,
        result,
        elapsed_ms(started),
        error_code,
        correlation_id,
    );
}

fn elapsed_ms(started: u64) -> u64 {
    Date::now().as_millis().saturating_sub(started)
}

/// `GET /api/ai/jobs/:id`
pub async fn status(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(job_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(row) = db
        .first::<StatusRow>(&status_statement(&job_id, &authenticated.id))
        .await?
    else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let (Some(kind), Some(status)) = (JobKind::parse(&row.kind), JobStatus::parse(&row.status))
    else {
        console_error!("the stored AI job has an unknown kind or status");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    Response::from_json(&JobStatusResponse {
        id: row.id,
        kind: kind.as_str(),
        status: status.as_str(),
        attempts: row.attempts,
    })
}

fn status_statement(job_id: &str, user_id: &str) -> SqlStatement {
    SqlStatement::new(
        "SELECT id, kind, status, attempts FROM ai_generation WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(job_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

/// トレーニングの終了時に、日の評価と記録のある種目ごとの提案の行を作る。
///
/// 同じ日の評価と種目の提案は一意キーで 1 行にまとめ、作り直すときは状態を
/// `pending` に戻す。記録の無い種目には提案を作らない。
pub async fn create_session_jobs(
    db: &Db,
    user_id: &str,
    training_id: &str,
    training_date: &str,
    sets: &[SessionSet],
) -> Result<()> {
    let exercise_ids = recorded_exercise_ids(sets);
    let ids = SessionJobIds::generate(exercise_ids.len())?;
    let now = auth::now_iso();
    let request = SessionJobRequest {
        user_id,
        training_id,
        training_date,
        exercise_ids: &exercise_ids,
        now: &now,
    };
    let statements = session_job_statements(&request, &ids);
    db.batch(&statements).await?;
    Ok(())
}

/// トレーニングの終了で作る生成の行の材料。
struct SessionJobRequest<'a> {
    user_id: &'a str,
    training_id: &'a str,
    training_date: &'a str,
    exercise_ids: &'a [String],
    now: &'a str,
}

/// 生成の行に付ける ID。
struct SessionJobIds {
    daily: String,
    proposals: Vec<String>,
}

impl SessionJobIds {
    fn generate(exercise_count: usize) -> Result<Self> {
        let daily = crate::random::random_uuid()?;
        let mut proposals = Vec::with_capacity(exercise_count);
        for _ in 0..exercise_count {
            proposals.push(crate::random::random_uuid()?);
        }
        Ok(Self { daily, proposals })
    }
}

fn recorded_exercise_ids(sets: &[SessionSet]) -> Vec<String> {
    let mut exercise_ids = Vec::new();
    for set in sets {
        let exercise_id = &set.record.key().exercise_id;
        if !exercise_ids.contains(exercise_id) {
            exercise_ids.push(exercise_id.clone());
        }
    }
    exercise_ids
}

fn session_job_statements(
    request: &SessionJobRequest<'_>,
    ids: &SessionJobIds,
) -> Vec<SqlStatement> {
    let mut statements = Vec::with_capacity(request.exercise_ids.len() + 1);
    statements.push(insert_pending_statement(&NewJob {
        id: &ids.daily,
        user_id: request.user_id,
        kind: JobKind::DailyEvaluation,
        training_id: Some(request.training_id),
        exercise_id: None,
        period_start: None,
        period_end: None,
        unique_key: &daily_evaluation_key(request.training_date),
        now: request.now,
    }));
    for (exercise_id, id) in request.exercise_ids.iter().zip(&ids.proposals) {
        statements.push(insert_pending_statement(&NewJob {
            id,
            user_id: request.user_id,
            kind: JobKind::SetProposal,
            training_id: Some(request.training_id),
            exercise_id: Some(exercise_id),
            period_start: None,
            period_end: None,
            unique_key: &set_proposal_key(exercise_id),
            now: request.now,
        }));
    }
    statements
}

struct NewJob<'a> {
    id: &'a str,
    user_id: &'a str,
    kind: JobKind,
    training_id: Option<&'a str>,
    exercise_id: Option<&'a str>,
    period_start: Option<&'a str>,
    period_end: Option<&'a str>,
    unique_key: &'a str,
    now: &'a str,
}

fn insert_pending_statement(job: &NewJob<'_>) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO ai_generation \
         (id, user_id, kind, training_id, exercise_id, period_start, period_end, unique_key, \
          status, attempts, lease_expires_at, next_attempt_at, output_id, last_error_code, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending', 0, NULL, NULL, NULL, NULL, ?, ?) \
         ON CONFLICT(user_id, unique_key) DO UPDATE SET \
          kind = excluded.kind, training_id = excluded.training_id, \
          exercise_id = excluded.exercise_id, period_start = excluded.period_start, \
          period_end = excluded.period_end, status = 'pending', attempts = 0, \
          lease_expires_at = NULL, next_attempt_at = NULL, output_id = NULL, \
          last_error_code = NULL, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(job.id.to_owned()),
            SqlValue::Text(job.user_id.to_owned()),
            SqlValue::Text(job.kind.as_str().to_owned()),
            optional_text(job.training_id),
            optional_text(job.exercise_id),
            optional_text(job.period_start),
            optional_text(job.period_end),
            SqlValue::Text(job.unique_key.to_owned()),
            SqlValue::Text(job.now.to_owned()),
            SqlValue::Text(job.now.to_owned()),
        ],
    )
}

fn optional_text(value: Option<&str>) -> SqlValue {
    value.map_or(SqlValue::Null, |text| SqlValue::Text(text.to_owned()))
}

/// Cron の実行で、lease の期限を過ぎた `enqueued` を `pending` に戻し、
/// 再試行の時刻を過ぎた `pending` を Queue へ送る。
pub async fn enqueue_pending(env: &Env, now: i64) -> Result<()> {
    let db = Db::new(env.d1("DB")?);
    let now_iso = datetime::format_utc(now);
    db.run(&recover_statement(&now_iso, &now_iso)).await?;
    let pending = load_due_pending(&db, &now_iso).await?;
    let queue = env.queue(QUEUE_BINDING)?;
    let correlation_id = log::next_request_id();
    for row in pending {
        let started = Date::now().as_millis();
        let kind = JobKind::parse(&row.kind);
        let kind_name = kind.map_or("unknown", JobKind::as_str);
        match queue
            .send(JobMessage {
                generation_id: row.id.clone(),
            })
            .await
        {
            Ok(()) => {
                let marked = db
                    .run(&enqueued_statement(
                        &row.id,
                        &row.user_id,
                        &datetime::format_utc(lease_deadline(now)),
                        &now_iso,
                    ))
                    .await?;
                let result = if changed_rows(&marked)? == 1 {
                    "enqueued"
                } else {
                    "skipped"
                };
                log_job(
                    kind_name,
                    &row.id,
                    row.attempts,
                    result,
                    elapsed_ms(started),
                    None,
                    &correlation_id,
                );
            }
            Err(error) => {
                console_error!("failed to enqueue the AI job {}: {error}", row.id);
                log_job(
                    kind_name,
                    &row.id,
                    row.attempts,
                    "enqueue_failed",
                    elapsed_ms(started),
                    Some(ERROR_QUEUE),
                    &correlation_id,
                );
            }
        }
    }
    Ok(())
}

async fn load_due_pending(db: &Db, now_iso: &str) -> Result<Vec<PendingRow>> {
    db.all(&due_pending_statement(now_iso))
        .await?
        .results::<PendingRow>()
}

fn recover_statement(now_iso: &str, updated_at: &str) -> SqlStatement {
    SqlStatement::new(
        "UPDATE ai_generation SET status = 'pending', lease_expires_at = NULL, \
         next_attempt_at = ?, updated_at = ? \
         WHERE status = 'enqueued' AND (lease_expires_at IS NULL OR lease_expires_at < ?)",
        vec![
            SqlValue::Text(now_iso.to_owned()),
            SqlValue::Text(updated_at.to_owned()),
            SqlValue::Text(updated_at.to_owned()),
        ],
    )
}

fn due_pending_statement(now_iso: &str) -> SqlStatement {
    SqlStatement::new(
        "SELECT id, user_id, kind, attempts FROM ai_generation \
         WHERE status = 'pending' AND (next_attempt_at IS NULL OR next_attempt_at <= ?) \
         ORDER BY created_at, id",
        vec![SqlValue::Text(now_iso.to_owned())],
    )
}

fn enqueued_statement(
    generation_id: &str,
    user_id: &str,
    lease_expires_at: &str,
    updated_at: &str,
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE ai_generation SET status = 'enqueued', lease_expires_at = ?, \
         next_attempt_at = NULL, updated_at = ? \
         WHERE id = ? AND user_id = ? AND status = 'pending'",
        vec![
            SqlValue::Text(lease_expires_at.to_owned()),
            SqlValue::Text(updated_at.to_owned()),
            SqlValue::Text(generation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

#[cfg(test)]
mod tests {
    //! 生成の行、Queue のメッセージ、Cron の SQL の例示テスト。
    //!
    //! 対応する要求: `REQ-AI-014`、`REQ-AI-018`。

    use serde_json::Value;
    use warrior_core::ai_job::JobKind;
    use warrior_core::sql::SqlValue;

    use super::{
        due_pending_statement, enqueued_statement, insert_pending_statement, recover_statement,
        session_job_statements, JobMessage, JobStatusResponse, NewJob, SessionJobIds,
        SessionJobRequest,
    };

    #[test]
    fn the_queue_message_carries_only_the_generation_id() {
        let message = JobMessage {
            generation_id: "job-1".to_owned(),
        };
        let value = serde_json::to_value(&message).expect("the message is JSON");
        assert_eq!(value, serde_json::json!({ "generation_id": "job-1" }));
        assert_eq!(value.as_object().expect("a JSON object").keys().count(), 1);
    }

    #[test]
    fn session_jobs_create_one_daily_evaluation_and_one_proposal_per_exercise() {
        let exercise_ids = vec!["bench-press".to_owned(), "plank".to_owned()];
        let request = SessionJobRequest {
            user_id: "user-1",
            training_id: "training-1",
            training_date: "2026-11-12",
            exercise_ids: &exercise_ids,
            now: "2026-11-12T00:00:00Z",
        };
        let ids = SessionJobIds {
            daily: "job-daily".to_owned(),
            proposals: vec!["job-bench-press".to_owned(), "job-plank".to_owned()],
        };
        let statements = session_job_statements(&request, &ids);
        assert_eq!(statements.len(), 3);
        assert_eq!(
            statements[0].bindings()[0],
            SqlValue::Text("job-daily".to_owned())
        );
        assert_eq!(
            statements[1].bindings()[0],
            SqlValue::Text("job-bench-press".to_owned())
        );
        assert_eq!(
            statements[2].bindings()[0],
            SqlValue::Text("job-plank".to_owned())
        );
        assert_eq!(
            statements[0].bindings()[2],
            SqlValue::Text("daily_evaluation".to_owned())
        );
        assert_eq!(
            statements[0].bindings()[7],
            SqlValue::Text("daily:2026-11-12".to_owned())
        );
        assert_eq!(
            statements[1].bindings()[2],
            SqlValue::Text("set_proposal".to_owned())
        );
        assert_eq!(
            statements[1].bindings()[4],
            SqlValue::Text("bench-press".to_owned())
        );
        assert_eq!(
            statements[1].bindings()[7],
            SqlValue::Text("set:bench-press".to_owned())
        );
        assert!(statements[0]
            .sql()
            .contains("ON CONFLICT(user_id, unique_key) DO UPDATE"));
        assert!(statements[0].sql().contains("status = 'pending'"));
        assert!(statements[0].sql().contains("attempts = 0"));
    }

    #[test]
    fn a_new_job_without_a_period_has_null_period_bindings() {
        let statement = insert_pending_statement(&NewJob {
            id: "job-1",
            user_id: "user-1",
            kind: JobKind::DailyEvaluation,
            training_id: Some("training-1"),
            exercise_id: None,
            period_start: None,
            period_end: None,
            unique_key: "daily:2026-11-12",
            now: "2026-11-12T00:00:00Z",
        });
        assert_eq!(
            statement.bindings()[3],
            SqlValue::Text("training-1".to_owned())
        );
        assert_eq!(statement.bindings()[4], SqlValue::Null);
        assert_eq!(statement.bindings()[5], SqlValue::Null);
        assert_eq!(statement.bindings()[6], SqlValue::Null);
        assert_eq!(statement.bindings().len(), 10);
    }

    #[test]
    fn an_expired_lease_returns_to_pending_and_keeps_the_attempt_count() {
        let statement = recover_statement("2026-11-12T00:00:00Z", "2026-11-12T00:00:00Z");
        assert!(statement
            .sql()
            .contains("status = 'pending', lease_expires_at = NULL"));
        assert!(statement.sql().contains("lease_expires_at < ?"));
        assert!(!statement.sql().contains("attempts = 0"));
        assert_eq!(statement.bindings().len(), 3);
    }

    #[test]
    fn only_pending_rows_due_by_now_are_enqueued() {
        let statement = due_pending_statement("2026-11-12T00:00:00Z");
        assert!(statement.sql().contains("status = 'pending'"));
        assert!(statement
            .sql()
            .contains("next_attempt_at IS NULL OR next_attempt_at <= ?"));
        assert_eq!(
            statement.bindings(),
            [SqlValue::Text("2026-11-12T00:00:00Z".to_owned())]
        );
    }

    #[test]
    fn enqueueing_marks_the_row_enqueued_only_while_it_is_pending() {
        let statement = enqueued_statement(
            "job-1",
            "user-1",
            "2026-11-12T00:01:00Z",
            "2026-11-12T00:00:00Z",
        );
        assert!(statement.sql().contains("status = 'enqueued'"));
        assert!(statement.sql().contains("AND status = 'pending'"));
        assert_eq!(statement.bindings().len(), 4);
    }

    #[test]
    fn the_status_response_does_not_expose_the_snapshots() {
        let value = serde_json::to_value(JobStatusResponse {
            id: "job-1".to_owned(),
            kind: "daily_evaluation",
            status: "failed",
            attempts: 4,
        })
        .expect("the response is JSON");
        assert_eq!(
            value,
            serde_json::json!({
                "id": "job-1",
                "kind": "daily_evaluation",
                "status": "failed",
                "attempts": 4
            })
        );
        assert_eq!(
            value
                .as_object()
                .expect("a JSON object")
                .keys()
                .filter(|key| matches!(key.as_str(), "input" | "output" | "memo"))
                .count(),
            0
        );
    }

    #[test]
    fn the_log_kind_is_stable_for_every_job_kind() {
        for kind in JobKind::ALL {
            let value = serde_json::to_value(kind).expect("the kind is JSON");
            assert_eq!(value, Value::String(kind.as_str().to_owned()));
        }
    }
}
