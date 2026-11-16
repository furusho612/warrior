//! AI 生成ジョブの Queue の消費と、失敗の分類、入出力スナップショットの保存。
//!
//! 消費の処理は、生成の行を読み、lease か再試行の時刻を確かめ、状態と試行回数が
//! 一致する 1 つの処理だけを進める。AI の出力は検証してから R2 と D1 に保存する。

use serde::{Deserialize, Serialize};
use warrior_core::ai::output::{AiOutput, Certainty};
use warrior_core::ai::prompt::{build_prompt_for, AiPrompt};
use warrior_core::ai_job::{
    advance, classify_failure, is_due, is_free_tier_exceeded, lease_deadline, next_attempt_at,
    AttemptOutcome, FailureKind, JobKind, JobState, JobStatus, JobTransition,
};
use warrior_core::datetime;
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Date, Env, MessageBuilder, Result};

use super::{
    elapsed_ms, log_job, log_skipped, optional_text, JobMessage, JobRow, JOB_COLUMNS, QUEUE_BINDING,
};

const AI_BINDING: &str = "AI";
const BUCKET_BINDING: &str = "BUCKET";
const MODEL_VAR: &str = "AI_MODEL";
const AI_MAX_TOKENS: u32 = 2_048;

use super::{
    ERROR_AI, ERROR_CLIENT, ERROR_CONFIG, ERROR_FREE_TIER, ERROR_INPUT, ERROR_INVALID_JOB,
    ERROR_INVALID_OUTPUT, ERROR_R2, ERROR_RATE_LIMITED, ERROR_SERVER, ERROR_TIMEOUT,
};

pub(super) async fn process_message(
    env: &Env,
    generation_id: &str,
    correlation_id: &str,
) -> Result<()> {
    let started = Date::now().as_millis();
    let db = crate::db::Db::new(env.d1("DB")?);
    let Some(row) = load_job(&db, generation_id).await? else {
        log_skipped(
            "unknown",
            generation_id,
            0,
            "dropped",
            started,
            correlation_id,
            None,
        );
        return Ok(());
    };
    let Some(kind) = JobKind::parse(&row.kind) else {
        console_error!("the AI job {generation_id} has an unknown kind");
        log_skipped(
            "unknown",
            &row.id,
            row.attempts,
            "skipped",
            started,
            correlation_id,
            Some("unknown"),
        );
        return Ok(());
    };
    let Some(status) = JobStatus::parse(&row.status) else {
        console_error!("the AI job {generation_id} has an unknown status");
        log_skipped(
            "unknown",
            &row.id,
            row.attempts,
            "skipped",
            started,
            correlation_id,
            Some("unknown"),
        );
        return Ok(());
    };
    let now = crate::auth::now_seconds();
    if !is_due(
        status,
        parse_time(row.lease_expires_at.as_deref()),
        parse_time(row.next_attempt_at.as_deref()),
        now,
    ) {
        log_skipped(
            kind.as_str(),
            &row.id,
            row.attempts,
            "skipped",
            started,
            correlation_id,
            None,
        );
        return Ok(());
    }
    if !claim(&db, &row, now).await? {
        log_skipped(
            kind.as_str(),
            &row.id,
            row.attempts,
            "skipped",
            started,
            correlation_id,
            None,
        );
        return Ok(());
    }
    let attempt = row.attempts.saturating_add(1);
    let failure = FailureContext {
        db: &db,
        env,
        row: &row,
        kind,
        attempt,
        started,
        correlation_id,
    };
    let valid_target = match kind {
        JobKind::DailyEvaluation | JobKind::SetProposal => row.training_id.is_some(),
        JobKind::WeeklyEvaluation | JobKind::MonthlyEvaluation | JobKind::MenuOptimization => {
            row.period_start.is_some() && row.period_end.is_some()
        }
    };
    if !valid_target {
        return record_job_failure(&failure, JobFailure::InvalidJob).await;
    }
    generate(&failure).await
}

async fn generate(failure: &FailureContext<'_>) -> Result<()> {
    let context = match match failure.kind {
        JobKind::DailyEvaluation | JobKind::SetProposal => {
            let Some(training_id) = failure.row.training_id.as_deref() else {
                return record_job_failure(failure, JobFailure::InvalidJob).await;
            };
            super::context::build(failure.db, &failure.row.user_id, training_id).await
        }
        JobKind::WeeklyEvaluation | JobKind::MonthlyEvaluation | JobKind::MenuOptimization => {
            let (Some(start), Some(end)) = (
                failure.row.period_start.as_deref(),
                failure.row.period_end.as_deref(),
            ) else {
                return record_job_failure(failure, JobFailure::InvalidJob).await;
            };
            super::context::build_period(failure.db, &failure.row.user_id, start, end).await
        }
    } {
        Ok(Some(context)) => context,
        Ok(None) => return record_job_failure(failure, JobFailure::Input).await,
        Err(error) => {
            console_error!(
                "failed to build the AI context for {}: {error}",
                failure.row.id
            );
            return record_failure(failure, FailureKind::Temporary, ERROR_INPUT).await;
        }
    };
    let model = match crate::auth::var(failure.env, MODEL_VAR) {
        Ok(model) if !model.trim().is_empty() => model,
        _ => return record_job_failure(failure, JobFailure::Config).await,
    };
    let mut prompt = build_prompt_for(failure.kind, &context);
    let previous = match failure.row.redo_of.as_deref() {
        Some(key) => crate::ai_output::load_output(failure.env, key).await,
        None => None,
    };
    if let Some(previous) = &previous {
        prompt.system.push_str(
            "\n\n今回の依頼: 前回の出力を踏まえ、より控えめな内容で作り直してください。\n\
             前回から何を変えたかを最初に 1 文で書いてください。\n\
             評価は certainty を必ず付け、断定を減らすか削除してください。\n\
             提案は重量、セット数、時間の少なくとも 1 つを前回より小さくしてください。",
        );
        if let Ok(previous_json) = serde_json::to_string(previous) {
            prompt.user.push_str("\n\n前回の出力: ");
            prompt.user.push_str(&previous_json);
        }
    }
    let ai = match failure.env.ai(AI_BINDING) {
        Ok(ai) => ai,
        Err(error) => {
            console_error!("failed to open the AI binding: {error}");
            return record_job_failure(failure, JobFailure::Config).await;
        }
    };
    let response = match ai
        .run::<_, AiResponse>(model.clone(), AiRequest::from(&prompt))
        .await
    {
        Ok(response) => response,
        Err(error) => {
            let (failure_kind, error_code) = failure_from_error(&error);
            return record_failure(failure, failure_kind, error_code).await;
        }
    };
    let parsed = match AiOutput::from_json(extract_json(&response.response), &context) {
        Ok(output) => output,
        Err(_error) => {
            console_error!("the AI output for {} was invalid", failure.row.id);
            return record_job_failure(failure, JobFailure::Output).await;
        }
    };
    let Some(output) = filter_output(failure.kind, failure.row.exercise_id.as_deref(), parsed)
    else {
        console_error!(
            "the AI output for {} has no content for the kind",
            failure.row.id
        );
        return record_job_failure(failure, JobFailure::Output).await;
    };
    if let Some(previous) = &previous {
        if !more_conservative(
            failure.kind,
            previous,
            &output,
            failure.row.exercise_id.as_deref(),
        ) {
            console_error!(
                "the redone AI output for {} is not more conservative",
                failure.row.id
            );
            return record_job_failure(failure, JobFailure::Output).await;
        }
    }
    let input_snapshot = InputSnapshot {
        model: &model,
        prompt: PromptSnapshot::from(&prompt),
    };
    let output_snapshot = OutputSnapshot {
        model: &model,
        prompt_version: prompt.version,
        output: &output,
    };
    let Some((input_key, output_key)) = store_snapshots(
        failure.env,
        &failure.row.user_id,
        &failure.row.id,
        &input_snapshot,
        &output_snapshot,
    )
    .await
    else {
        return record_job_failure(failure, JobFailure::R2).await;
    };
    let saved = save_output(
        failure.db,
        &OutputSave {
            row: failure.row,
            kind: failure.kind,
            model: &model,
            prompt_version: prompt.version,
            input_key: &input_key,
            output_key: &output_key,
            attempt: failure.attempt,
        },
    )
    .await?;
    if saved {
        log_job(
            failure.kind.as_str(),
            &failure.row.id,
            failure.attempt,
            "generated",
            elapsed_ms(failure.started),
            None,
            failure.correlation_id,
        );
        if matches!(
            failure.kind,
            JobKind::WeeklyEvaluation | JobKind::MonthlyEvaluation
        ) {
            if let (Some(start), Some(end)) = (
                failure.row.period_start.as_deref(),
                failure.row.period_end.as_deref(),
            ) {
                if let Err(error) =
                    super::create_menu_job(failure.db, &failure.row.user_id, start, end).await
                {
                    console_error!("failed to create the menu optimization job: {error}");
                }
            }
        }
    } else {
        log_skipped(
            failure.kind.as_str(),
            &failure.row.id,
            failure.attempt,
            "skipped",
            failure.started,
            failure.correlation_id,
            None,
        );
    }
    Ok(())
}

/// 生成の種別に合う内容だけを残す。
fn filter_output(
    kind: JobKind,
    exercise_id: Option<&str>,
    mut output: AiOutput,
) -> Option<AiOutput> {
    let safety_advice = output.safety_advice.take();
    match kind {
        JobKind::DailyEvaluation | JobKind::WeeklyEvaluation | JobKind::MonthlyEvaluation => {
            let evaluation = output.evaluation.take()?;
            Some(AiOutput {
                evaluation: Some(evaluation),
                safety_advice,
                ..AiOutput::default()
            })
        }
        JobKind::SetProposal => {
            output
                .proposals
                .retain(|proposal| exercise_id.is_none_or(|id| proposal.exercise_id == id));
            if output.proposals.is_empty() {
                return None;
            }
            Some(AiOutput {
                proposals: output.proposals,
                safety_advice,
                ..AiOutput::default()
            })
        }
        JobKind::MenuOptimization => {
            if output.menu_changes.is_empty() {
                return None;
            }
            Some(AiOutput {
                menu_changes: output.menu_changes,
                safety_advice,
                ..AiOutput::default()
            })
        }
    }
}

/// 作り直した出力が、前回より控えめであるか。
fn more_conservative(
    kind: JobKind,
    previous: &AiOutput,
    current: &AiOutput,
    exercise_id: Option<&str>,
) -> bool {
    match kind {
        JobKind::DailyEvaluation | JobKind::WeeklyEvaluation | JobKind::MonthlyEvaluation => {
            let previous = previous
                .evaluation
                .as_ref()
                .map(assertion_count)
                .unwrap_or(0);
            let current = current
                .evaluation
                .as_ref()
                .map(assertion_count)
                .unwrap_or(0);
            current < previous
        }
        JobKind::SetProposal => {
            let Some(previous) = previous
                .proposals
                .iter()
                .find(|proposal| exercise_id.is_none_or(|id| proposal.exercise_id == id))
            else {
                return true;
            };
            let Some(current) = current
                .proposals
                .iter()
                .find(|proposal| proposal.exercise_id == previous.exercise_id)
            else {
                return false;
            };
            number_smaller(previous.weight_kg, current.weight_kg)
                || current.sets < previous.sets
                || number_smaller(
                    previous.duration_seconds.map(f64::from),
                    current.duration_seconds.map(f64::from),
                )
        }
        JobKind::MenuOptimization => true,
    }
}

fn number_smaller(previous: Option<f64>, current: Option<f64>) -> bool {
    match (previous, current) {
        (Some(previous), Some(current)) => current < previous,
        _ => false,
    }
}

fn assertion_count(evaluation: &warrior_core::ai::output::Evaluation) -> usize {
    evaluation
        .good
        .iter()
        .chain(evaluation.improvements.iter())
        .chain(evaluation.next_actions.iter())
        .filter(|item| item.certainty == Certainty::Assertion)
        .count()
}

/// 失敗の種類ごとの分類とログのコード。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobFailure {
    Input,
    Output,
    R2,
    Config,
    InvalidJob,
}

impl JobFailure {
    fn kind(self) -> FailureKind {
        FailureKind::Permanent
    }

    fn code(self) -> &'static str {
        match self {
            Self::Input => ERROR_INPUT,
            Self::Output => ERROR_INVALID_OUTPUT,
            Self::R2 => ERROR_R2,
            Self::Config => ERROR_CONFIG,
            Self::InvalidJob => ERROR_INVALID_JOB,
        }
    }
}

struct FailureContext<'a> {
    db: &'a crate::db::Db,
    env: &'a Env,
    row: &'a JobRow,
    kind: JobKind,
    attempt: u32,
    started: u64,
    correlation_id: &'a str,
}

async fn record_job_failure(context: &FailureContext<'_>, failure: JobFailure) -> Result<()> {
    record_failure(context, failure.kind(), failure.code()).await
}

async fn record_failure(
    context: &FailureContext<'_>,
    failure_kind: FailureKind,
    error_code: &'static str,
) -> Result<()> {
    let now = crate::auth::now_seconds();
    let outcome = match failure_kind {
        FailureKind::Temporary => AttemptOutcome::TemporaryFailure,
        FailureKind::Permanent => AttemptOutcome::PermanentFailure,
    };
    let transition = advance(
        JobState::new(JobStatus::Enqueued, context.attempt.saturating_sub(1)),
        outcome,
    );
    match transition {
        JobTransition::Retry {
            state,
            delay_seconds,
        } => {
            let due = next_attempt_at(now, delay_seconds);
            let updated = apply_failure(
                context.db,
                context.row,
                state.status,
                state.attempts,
                Some(due),
                error_code,
                now,
            )
            .await?;
            if !updated {
                log_skipped(
                    context.kind.as_str(),
                    &context.row.id,
                    context.attempt,
                    "skipped",
                    context.started,
                    context.correlation_id,
                    Some(error_code),
                );
                return Ok(());
            }
            if let Err(error) = send_retry(context.env, &context.row.id, delay_seconds).await {
                console_error!(
                    "failed to send the retry message for {}: {error}",
                    context.row.id
                );
            }
            log_job(
                context.kind.as_str(),
                &context.row.id,
                context.attempt,
                "retry",
                elapsed_ms(context.started),
                Some(error_code),
                context.correlation_id,
            );
        }
        JobTransition::Terminal(state) => {
            let updated = apply_failure(
                context.db,
                context.row,
                state.status,
                state.attempts,
                None,
                error_code,
                now,
            )
            .await?;
            let result = if updated { "failed" } else { "skipped" };
            log_job(
                context.kind.as_str(),
                &context.row.id,
                context.attempt,
                result,
                elapsed_ms(context.started),
                Some(error_code),
                context.correlation_id,
            );
        }
    }
    Ok(())
}

async fn load_job(db: &crate::db::Db, generation_id: &str) -> Result<Option<JobRow>> {
    db.first(&SqlStatement::new(
        format!("SELECT {JOB_COLUMNS} FROM ai_generation WHERE id = ?"),
        vec![SqlValue::Text(generation_id.to_owned())],
    ))
    .await
}

fn parse_time(text: Option<&str>) -> Option<i64> {
    text.and_then(datetime::parse_utc)
}

/// 消費の処理を始める。状態と試行回数が一致する 1 つの処理だけが成功する。
async fn claim(db: &crate::db::Db, row: &JobRow, now: i64) -> Result<bool> {
    let result = db.run(&claim_statement(row, now)).await?;
    Ok(crate::db::changed_rows(&result)? == 1)
}

fn claim_statement(row: &JobRow, now: i64) -> SqlStatement {
    let now_iso = datetime::format_utc(now);
    let lease = datetime::format_utc(lease_deadline(now));
    SqlStatement::new(
        "UPDATE ai_generation SET status = 'enqueued', \
         lease_expires_at = ?, next_attempt_at = NULL, updated_at = ? \
         WHERE id = ? AND user_id = ? AND attempts = ? \
         AND ((status = 'enqueued' AND lease_expires_at IS NOT NULL AND lease_expires_at >= ?) \
         OR (status = 'pending' AND (next_attempt_at IS NULL OR next_attempt_at <= ?)))",
        vec![
            SqlValue::Text(lease),
            SqlValue::Text(now_iso.clone()),
            SqlValue::Text(row.id.clone()),
            SqlValue::Text(row.user_id.clone()),
            SqlValue::Integer(i64::from(row.attempts)),
            SqlValue::Text(now_iso.clone()),
            SqlValue::Text(now_iso),
        ],
    )
}

async fn apply_failure(
    db: &crate::db::Db,
    row: &JobRow,
    status: JobStatus,
    attempts: u32,
    next_attempt: Option<i64>,
    error_code: &str,
    now: i64,
) -> Result<bool> {
    let result = db
        .run(&failure_statement(
            row,
            status,
            attempts,
            next_attempt,
            error_code,
            now,
        ))
        .await?;
    Ok(crate::db::changed_rows(&result)? == 1)
}

fn failure_statement(
    row: &JobRow,
    status: JobStatus,
    attempts: u32,
    next_attempt: Option<i64>,
    error_code: &str,
    now: i64,
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE ai_generation SET status = ?, attempts = ?, lease_expires_at = NULL, \
         next_attempt_at = ?, last_error_code = ?, updated_at = ? \
         WHERE id = ? AND user_id = ? AND status = 'enqueued' AND attempts = ?",
        vec![
            SqlValue::Text(status.as_str().to_owned()),
            SqlValue::Integer(i64::from(attempts)),
            next_attempt.map_or(SqlValue::Null, |seconds| {
                SqlValue::Text(datetime::format_utc(seconds))
            }),
            SqlValue::Text(error_code.to_owned()),
            SqlValue::Text(datetime::format_utc(now)),
            SqlValue::Text(row.id.clone()),
            SqlValue::Text(row.user_id.clone()),
            SqlValue::Integer(i64::from(row.attempts)),
        ],
    )
}

async fn send_retry(env: &Env, generation_id: &str, delay_seconds: u32) -> Result<()> {
    let queue = env.queue(QUEUE_BINDING)?;
    queue
        .send(
            MessageBuilder::new(JobMessage {
                generation_id: generation_id.to_owned(),
            })
            .delay_seconds(delay_seconds)
            .build(),
        )
        .await
}

struct OutputSave<'a> {
    row: &'a JobRow,
    kind: JobKind,
    model: &'a str,
    prompt_version: &'a str,
    input_key: &'a str,
    output_key: &'a str,
    attempt: u32,
}

async fn save_output(db: &crate::db::Db, save: &OutputSave<'_>) -> Result<bool> {
    let now = crate::auth::now_iso();
    let results = db
        .batch(&[
            insert_output_statement(save, &now),
            mark_generated_statement(&save.row.id, &save.row.user_id, save.attempt, &now),
        ])
        .await?;
    let updated = results.get(1).ok_or_else(|| {
        worker::Error::RustError("the generated status update is missing".to_owned())
    })?;
    Ok(crate::db::changed_rows(updated)? == 1)
}

fn insert_output_statement(save: &OutputSave<'_>, now: &str) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO ai_outputs \
         (id, user_id, kind, training_id, exercise_id, model, prompt_version, \
          input_key, output_key, created_at) \
         SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
         WHERE EXISTS (SELECT 1 FROM ai_generation \
           WHERE id = ? AND user_id = ? AND status = 'enqueued' AND attempts = ?) \
         ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, \
          training_id = excluded.training_id, exercise_id = excluded.exercise_id, \
          model = excluded.model, prompt_version = excluded.prompt_version, \
          input_key = excluded.input_key, output_key = excluded.output_key, \
          created_at = excluded.created_at",
        vec![
            SqlValue::Text(save.row.id.clone()),
            SqlValue::Text(save.row.user_id.clone()),
            SqlValue::Text(save.kind.as_str().to_owned()),
            optional_text(save.row.training_id.as_deref()),
            optional_text(save.row.exercise_id.as_deref()),
            SqlValue::Text(save.model.to_owned()),
            SqlValue::Text(save.prompt_version.to_owned()),
            SqlValue::Text(save.input_key.to_owned()),
            SqlValue::Text(save.output_key.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(save.row.id.clone()),
            SqlValue::Text(save.row.user_id.clone()),
            SqlValue::Integer(i64::from(save.attempt.saturating_sub(1))),
        ],
    )
}

fn mark_generated_statement(
    generation_id: &str,
    user_id: &str,
    attempt: u32,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE ai_generation SET status = 'generated', output_id = ?, attempts = ?, \
         lease_expires_at = NULL, next_attempt_at = NULL, last_error_code = NULL, \
         updated_at = ? \
         WHERE id = ? AND user_id = ? AND status = 'enqueued' AND attempts = ?",
        vec![
            SqlValue::Text(generation_id.to_owned()),
            SqlValue::Integer(i64::from(attempt)),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(generation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Integer(i64::from(attempt.saturating_sub(1))),
        ],
    )
}

/// AI に送るチャットの入力。
#[derive(Debug, Serialize)]
struct AiRequest {
    messages: Vec<AiMessage>,
    max_tokens: u32,
}

impl From<&AiPrompt> for AiRequest {
    fn from(prompt: &AiPrompt) -> Self {
        Self {
            messages: vec![
                AiMessage {
                    role: "system",
                    content: prompt.system.clone(),
                },
                AiMessage {
                    role: "user",
                    content: prompt.user.clone(),
                },
            ],
            max_tokens: AI_MAX_TOKENS,
        }
    }
}

#[derive(Debug, Serialize)]
struct AiMessage {
    role: &'static str,
    content: String,
}

/// Workers AI のチャットの応答。
#[derive(Debug, Deserialize)]
struct AiResponse {
    response: String,
}

#[derive(Debug, Serialize)]
struct InputSnapshot<'a> {
    model: &'a str,
    prompt: PromptSnapshot<'a>,
}

#[derive(Debug, Serialize)]
struct PromptSnapshot<'a> {
    version: &'a str,
    system: &'a str,
    user: &'a str,
}

impl<'a> From<&'a AiPrompt> for PromptSnapshot<'a> {
    fn from(prompt: &'a AiPrompt) -> Self {
        Self {
            version: prompt.version,
            system: &prompt.system,
            user: &prompt.user,
        }
    }
}

#[derive(Debug, Serialize)]
struct OutputSnapshot<'a> {
    model: &'a str,
    prompt_version: &'a str,
    output: &'a AiOutput,
}

async fn store_snapshots(
    env: &Env,
    user_id: &str,
    generation_id: &str,
    input: &InputSnapshot<'_>,
    output: &OutputSnapshot<'_>,
) -> Option<(String, String)> {
    let bucket = match env.bucket(BUCKET_BINDING) {
        Ok(bucket) => bucket,
        Err(error) => {
            console_error!("failed to open the R2 bucket: {error}");
            return None;
        }
    };
    let (input_key, output_key) = snapshot_keys(user_id, generation_id);
    let input_bytes = match serde_json::to_vec(input) {
        Ok(bytes) => bytes,
        Err(error) => {
            console_error!("failed to serialize the AI input snapshot: {error}");
            return None;
        }
    };
    let output_bytes = match serde_json::to_vec(output) {
        Ok(bytes) => bytes,
        Err(error) => {
            console_error!("failed to serialize the AI output snapshot: {error}");
            return None;
        }
    };
    if let Err(error) = bucket.put(input_key.clone(), input_bytes).execute().await {
        console_error!("failed to store the AI input snapshot: {error}");
        return None;
    }
    if let Err(error) = bucket.put(output_key.clone(), output_bytes).execute().await {
        console_error!("failed to store the AI output snapshot: {error}");
        return None;
    }
    Some((input_key, output_key))
}

fn snapshot_keys(user_id: &str, generation_id: &str) -> (String, String) {
    let prefix = format!("users/{user_id}/ai/{generation_id}");
    (
        format!("{prefix}/input.json"),
        format!("{prefix}/output.json"),
    )
}

/// モデルの応答から JSON の本体を取り出す。
///
/// JSON Mode に対応しないモデルは、JSON を説明文やコードブロックで囲むことがある。
/// 最初の `{` から最後の `}` までを検証へ渡す。
fn extract_json(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if start < end => &text[start..=end],
        _ => text,
    }
}

fn failure_from_error(error: &worker::Error) -> (FailureKind, &'static str) {
    let (status, code, timed_out) = error_parts(error);
    let kind = classify_failure(status, code.as_deref(), timed_out);
    let error_code = match kind {
        FailureKind::Temporary => {
            if timed_out {
                ERROR_TIMEOUT
            } else if is_rate_limited(status, code.as_deref()) {
                ERROR_RATE_LIMITED
            } else {
                ERROR_SERVER
            }
        }
        FailureKind::Permanent => {
            if is_free_tier_exceeded(code.as_deref()) {
                ERROR_FREE_TIER
            } else if is_client_error(status, code.as_deref()) {
                ERROR_CLIENT
            } else {
                ERROR_AI
            }
        }
    };
    (kind, error_code)
}

fn error_parts(error: &worker::Error) -> (Option<u16>, Option<String>, bool) {
    match error {
        worker::Error::UnknownJsError {
            name,
            message,
            code,
            ..
        } => (
            first_status(message).or_else(|| code.as_deref().and_then(parse_status)),
            code.clone(),
            is_timeout(name.as_deref(), message),
        ),
        worker::Error::JsError(message) => (first_status(message), None, is_timeout(None, message)),
        worker::Error::Json((_, status)) => (Some(*status), None, false),
        worker::Error::DailyLimitExceeded(_) => {
            (None, Some("E_DAILY_LIMIT_EXCEEDED".to_owned()), false)
        }
        worker::Error::RateLimitExceeded(_) => (Some(429), None, false),
        worker::Error::InternalError(_) => (Some(500), None, false),
        _ => (None, None, false),
    }
}

fn first_status(message: &str) -> Option<u16> {
    message
        .split(|character: char| !character.is_ascii_digit())
        .filter(|token| token.len() == 3)
        .filter_map(|token| token.parse::<u16>().ok())
        .find(|status| (400..=599).contains(status))
}

fn parse_status(code: &str) -> Option<u16> {
    let status = code.trim().parse::<u16>().ok()?;
    (400..=599).contains(&status).then_some(status)
}

fn is_timeout(name: Option<&str>, message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    name.is_some_and(|name| name.to_ascii_lowercase().contains("timeout"))
        || lower.contains("timed out")
        || lower.contains("timeout")
}

fn is_rate_limited(status: Option<u16>, code: Option<&str>) -> bool {
    status == Some(429) || code.is_some_and(|code| code.trim() == "429")
}

fn is_client_error(status: Option<u16>, code: Option<&str>) -> bool {
    let status = status.or_else(|| code.and_then(parse_status));
    matches!(status, Some(400 | 401 | 403 | 422))
}

#[cfg(test)]
mod tests {
    //! 消費の claim、失敗の記録、R2 のキー、応答の解析の例示テスト。
    //!
    //! 対応する要求: `REQ-AI-014`、`REQ-AI-018`。

    use warrior_core::ai_job::{FailureKind, JobStatus};
    use warrior_core::sql::SqlValue;

    use super::{claim_statement, failure_statement, snapshot_keys, JobFailure, JobRow, ERROR_R2};

    fn row() -> JobRow {
        JobRow {
            id: "job-1".to_owned(),
            user_id: "user-1".to_owned(),
            kind: "daily_evaluation".to_owned(),
            training_id: Some("training-1".to_owned()),
            exercise_id: None,
            period_start: None,
            period_end: None,
            redo_of: None,
            status: "enqueued".to_owned(),
            attempts: 1,
            lease_expires_at: Some("2026-11-12T00:01:00Z".to_owned()),
            next_attempt_at: None,
        }
    }

    #[test]
    fn a_claim_accepts_the_lease_and_the_due_pending_retry() {
        let statement = claim_statement(&row(), 1_700_000_000);
        assert!(statement
            .sql()
            .contains("status = 'enqueued' AND lease_expires_at IS NOT NULL"));
        assert!(statement
            .sql()
            .contains("status = 'pending' AND (next_attempt_at IS NULL OR next_attempt_at <= ?)"));
        assert!(statement.sql().contains("AND attempts = ?"));
        assert_eq!(statement.bindings().len(), 7);
    }

    #[test]
    fn a_temporary_failure_writes_pending_with_the_next_attempt_time() {
        let statement = failure_statement(
            &row(),
            JobStatus::Pending,
            2,
            Some(1_700_000_031),
            "http_5xx",
            1_700_000_001,
        );
        assert!(statement.sql().contains("status = ?, attempts = ?"));
        assert!(statement.sql().contains("lease_expires_at = NULL"));
        assert_eq!(
            statement.bindings()[0],
            SqlValue::Text("pending".to_owned())
        );
        assert_eq!(statement.bindings()[1], SqlValue::Integer(2));
        assert_eq!(
            statement.bindings()[2],
            SqlValue::Text("2023-11-14T22:13:51Z".to_owned())
        );
        assert_eq!(
            statement.bindings()[7],
            SqlValue::Integer(1),
            "the update must match the attempt count before the claim"
        );
    }

    #[test]
    fn a_permanent_failure_writes_failed_without_a_next_attempt_time() {
        let statement = failure_statement(
            &row(),
            JobStatus::Failed,
            2,
            None,
            "invalid_output",
            1_700_000_001,
        );
        assert_eq!(statement.bindings()[0], SqlValue::Text("failed".to_owned()));
        assert_eq!(statement.bindings()[2], SqlValue::Null);
    }

    #[test]
    fn the_r2_failure_is_permanent() {
        assert_eq!(JobFailure::R2.kind(), FailureKind::Permanent);
        assert_eq!(JobFailure::R2.code(), ERROR_R2);
    }

    #[test]
    fn snapshot_keys_are_under_the_user_prefix_and_use_the_generation_id() {
        assert_eq!(
            snapshot_keys("user-1", "job-1"),
            (
                "users/user-1/ai/job-1/input.json".to_owned(),
                "users/user-1/ai/job-1/output.json".to_owned()
            )
        );
    }

    #[test]
    fn extract_json_takes_the_object_from_a_wrapped_response() {
        assert_eq!(
            super::extract_json("```json\n{\"evaluation\":null}\n```"),
            "{\"evaluation\":null}"
        );
        assert_eq!(super::extract_json("no json here"), "no json here");
        assert_eq!(super::extract_json("}"), "}");
    }

    #[test]
    fn first_status_reads_only_three_digit_http_codes() {
        assert_eq!(super::first_status("error 503"), Some(503));
        assert_eq!(super::first_status("code 5007"), None);
        assert_eq!(super::first_status("status: 429 too many"), Some(429));
        assert_eq!(super::first_status("no status"), None);
    }

    #[test]
    fn an_unknown_worker_error_is_permanent() {
        let error = worker::Error::RustError("unexpected".to_owned());
        let (kind, code) = super::failure_from_error(&error);
        assert_eq!(kind, FailureKind::Permanent);
        assert_eq!(code, "ai_error");
    }
}
