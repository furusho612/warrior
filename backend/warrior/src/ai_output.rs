//! AI の評価と次回セットの提案の取得 (REQ-AI-001、REQ-AI-004、REQ-AI-006、REQ-AI-018)。
//!
//! 生成に成功した出力は R2 に置き、D1 にはメタデータだけを持つ (ADR-017)。
//! このモジュールは認証済みの利用者の行だけを読み、R2 の出力を検証済みの型に戻して返す。
//! 生成中の行も状態として返し、画面が条件を待てるようにする。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use warrior_core::ai::output::{AiOutput, Evaluation, MenuChange, MenuChangeKind, SetProposal};
use warrior_core::ai_job::JobKind;
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Env, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};

const BUCKET_BINDING: &str = "BUCKET";

#[derive(Debug, Deserialize)]
struct GenerationRow {
    id: String,
    kind: String,
    training_id: Option<String>,
    exercise_id: Option<String>,
    period_start: Option<String>,
    period_end: Option<String>,
    status: String,
    output_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StoredOutput {
    output: AiOutput,
}

#[derive(Debug, Deserialize)]
struct DetailRow {
    id: String,
    kind: String,
    status: String,
    period_start: Option<String>,
    period_end: Option<String>,
    output_key: Option<String>,
    training_date: Option<String>,
    goal: Option<String>,
}

/// 評価の詳細の応答。
#[derive(Debug, Serialize)]
struct EvaluationDetail {
    id: String,
    kind: &'static str,
    status: &'static str,
    date: Option<String>,
    period_start: Option<String>,
    period_end: Option<String>,
    goal: Option<String>,
    evaluation: Option<Evaluation>,
}

/// 評価の 1 件。
#[derive(Debug, Serialize)]
struct EvaluationEntry {
    id: String,
    kind: &'static str,
    training_id: Option<String>,
    period_start: Option<String>,
    period_end: Option<String>,
    status: &'static str,
    evaluation: Option<Evaluation>,
}

/// 評価の一覧の応答。
#[derive(Debug, Serialize)]
struct EvaluationsResponse {
    evaluations: Vec<EvaluationEntry>,
}

/// 次回セットの提案の 1 件。
#[derive(Debug, Serialize)]
struct ProposalEntry {
    id: String,
    exercise_id: String,
    status: &'static str,
    proposal: Option<SetProposal>,
}

/// 次回セットの提案の一覧の応答。
#[derive(Debug, Serialize)]
struct ProposalsResponse {
    proposals: Vec<ProposalEntry>,
}

/// メニューの最適化の応答。
#[derive(Debug, Serialize)]
struct MenuResponse {
    id: Option<String>,
    status: &'static str,
    period_start: Option<String>,
    period_end: Option<String>,
    changes: Vec<MenuChange>,
}

/// `GET /api/ai/evaluations`
pub async fn evaluations(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let query = request
        .url()?
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    let date = query.get("date").filter(|value| !value.is_empty());
    let db = Db::new(env.d1("DB")?);
    let rows = match load_evaluations(&db, &authenticated.id, date).await {
        Ok(rows) => rows,
        Err(error) => {
            console_error!("failed to load the AI evaluations: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let mut evaluations = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(kind) = JobKind::parse(&row.kind) else {
            continue;
        };
        let evaluation = match (&row.output_key, row.status == "generated") {
            (Some(key), true) => load_output(&env, key)
                .await
                .map(|output| output.evaluation.unwrap_or_default()),
            _ => None,
        };
        evaluations.push(EvaluationEntry {
            id: row.id,
            kind: kind.as_str(),
            training_id: row.training_id,
            period_start: row.period_start,
            period_end: row.period_end,
            status: if row.status == "generated" {
                "generated"
            } else if row.status == "failed" {
                "failed"
            } else {
                "pending"
            },
            evaluation,
        });
    }
    Response::from_json(&EvaluationsResponse { evaluations })
}

/// `GET /api/evaluations/:id`
///
/// 認証済みの利用者の評価だけを返す。別の利用者の ID は 404 にする。
pub async fn evaluation_detail(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let row = match db
        .first::<DetailRow>(&SqlStatement::new(
            "SELECT g.id, g.kind, g.status, g.training_id, g.period_start, g.period_end, \
             o.output_key, t.date AS training_date, p.goal \
             FROM ai_generation g \
             LEFT JOIN ai_outputs o ON o.id = g.id \
             LEFT JOIN training_sessions t ON t.id = g.training_id \
             LEFT JOIN profiles p ON p.user_id = g.user_id \
             WHERE g.id = ? AND g.user_id = ?",
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(authenticated.id.clone()),
            ],
        ))
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return Ok(response(ErrorCode::NotFound, "resource not found")),
        Err(error) => {
            console_error!("failed to load the AI evaluation: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let Some(kind) = JobKind::parse(&row.kind) else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if !matches!(
        kind,
        JobKind::DailyEvaluation | JobKind::WeeklyEvaluation | JobKind::MonthlyEvaluation
    ) {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    let evaluation = match (&row.output_key, row.status.as_str()) {
        (Some(key), "generated") => load_output(&env, key)
            .await
            .and_then(|output| output.evaluation),
        _ => None,
    };
    Response::from_json(&EvaluationDetail {
        id: row.id,
        kind: kind.as_str(),
        status: if row.status == "generated" {
            "generated"
        } else if row.status == "failed" {
            "failed"
        } else {
            "pending"
        },
        date: row.training_date.clone(),
        period_start: row.period_start,
        period_end: row.period_end,
        goal: row.goal,
        evaluation,
    })
}

/// `GET /api/ai/proposals`
pub async fn proposals(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let query = request
        .url()?
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    let exercise_id = query
        .get("exercise_id")
        .filter(|value| !value.is_empty())
        .map(String::as_str);
    let db = Db::new(env.d1("DB")?);
    let rows = match load_proposals(&db, &authenticated.id, exercise_id).await {
        Ok(rows) => rows,
        Err(error) => {
            console_error!("failed to load the AI proposals: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let mut proposals = Vec::with_capacity(rows.len());
    for row in rows {
        let exercise_id = row.exercise_id.clone().unwrap_or_default();
        let proposal = match (&row.output_key, row.status == "generated") {
            (Some(key), true) => load_output(&env, key).await.and_then(|output| {
                output
                    .proposals
                    .into_iter()
                    .find(|proposal| proposal.exercise_id == exercise_id)
            }),
            _ => None,
        };
        proposals.push(ProposalEntry {
            id: row.id,
            exercise_id,
            status: if row.status == "generated" {
                "generated"
            } else if row.status == "failed" {
                "failed"
            } else {
                "pending"
            },
            proposal,
        });
    }
    Response::from_json(&ProposalsResponse { proposals })
}

/// `GET /api/ai/menu`
pub async fn menu(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let row = match load_latest_menu(&db, &authenticated.id).await {
        Ok(row) => row,
        Err(error) => {
            console_error!("failed to load the menu optimization: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let Some(row) = row else {
        return Response::from_json(&MenuResponse {
            id: None,
            status: "none",
            period_start: None,
            period_end: None,
            changes: Vec::new(),
        });
    };
    let changes = match (&row.output_key, row.status == "generated") {
        (Some(key), true) => load_output(&env, key)
            .await
            .map(|output| output.menu_changes)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    Response::from_json(&MenuResponse {
        id: Some(row.id),
        status: if row.status == "generated" {
            "generated"
        } else if row.status == "failed" {
            "failed"
        } else {
            "pending"
        },
        period_start: row.period_start,
        period_end: row.period_end,
        changes,
    })
}

async fn load_latest_menu(db: &Db, user_id: &str) -> Result<Option<GenerationRow>> {
    db.first(&SqlStatement::new(
        "SELECT g.id, g.kind, g.training_id, g.exercise_id, g.period_start, g.period_end, \
         g.status, o.output_key \
         FROM ai_generation g LEFT JOIN ai_outputs o ON o.id = g.id \
         WHERE g.user_id = ? AND g.kind = 'menu_optimization' \
         ORDER BY g.created_at DESC LIMIT 1",
        vec![SqlValue::Text(user_id.to_owned())],
    ))
    .await
}

async fn load_evaluations(
    db: &Db,
    user_id: &str,
    date: Option<&String>,
) -> Result<Vec<GenerationRow>> {
    let filter = if date.is_some() {
        "AND g.unique_key = ?"
    } else {
        ""
    };
    let mut bindings = vec![SqlValue::Text(user_id.to_owned())];
    if let Some(date) = date {
        bindings.push(SqlValue::Text(format!("daily:{date}")));
    }
    db.all(&SqlStatement::new(
        format!(
            "SELECT g.id, g.kind, g.training_id, g.exercise_id, g.period_start, g.period_end, \
             g.status, o.output_key \
             FROM ai_generation g LEFT JOIN ai_outputs o ON o.id = g.id \
             WHERE g.user_id = ? AND g.kind IN \
             ('daily_evaluation', 'weekly_evaluation', 'monthly_evaluation') {filter} \
             ORDER BY COALESCE(g.period_start, g.created_at) DESC, g.created_at DESC LIMIT 100"
        ),
        bindings,
    ))
    .await?
    .results::<GenerationRow>()
}

async fn load_proposals(
    db: &Db,
    user_id: &str,
    exercise_id: Option<&str>,
) -> Result<Vec<GenerationRow>> {
    let filter = if exercise_id.is_some() {
        "AND g.exercise_id = ?"
    } else {
        ""
    };
    let mut bindings = vec![SqlValue::Text(user_id.to_owned())];
    if let Some(exercise_id) = exercise_id {
        bindings.push(SqlValue::Text(exercise_id.to_owned()));
    }
    db.all(&SqlStatement::new(
        format!(
            "SELECT g.id, g.kind, g.training_id, g.exercise_id, g.period_start, g.period_end, \
             g.status, o.output_key \
             FROM ai_generation g LEFT JOIN ai_outputs o ON o.id = g.id \
             WHERE g.user_id = ? AND g.kind = 'set_proposal' {filter} \
             ORDER BY g.created_at DESC LIMIT 200"
        ),
        bindings,
    ))
    .await?
    .results::<GenerationRow>()
}

pub(crate) async fn load_output(env: &Env, key: &str) -> Option<AiOutput> {
    let bucket = env.bucket(BUCKET_BINDING).ok()?;
    let object = bucket.get(key).execute().await.ok()??;
    let body = object.body()?;
    let text = body.text().await.ok()?;
    serde_json::from_str::<StoredOutput>(&text)
        .ok()
        .map(|stored| stored.output)
}

#[derive(Debug, Deserialize, Serialize)]
struct MutableStoredOutput {
    model: String,
    prompt_version: String,
    output: AiOutput,
}

/// 次回セットの提案の修正の入力。
#[derive(Debug, Deserialize)]
struct ProposalUpdate {
    weight_kg: Option<f64>,
    reps: Option<u32>,
    duration_seconds: Option<u32>,
    sets: u32,
    reason: String,
}

/// `PUT /api/ai/proposals/:id`
///
/// 利用者が修正した提案を保存し、次回の候補として使えるようにする。
pub async fn update_proposal(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: ProposalUpdate = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the proposal is invalid")),
    };
    if input.sets == 0
        || input.sets > 20
        || input.reason.trim().is_empty()
        || (input.reps.is_none() && input.duration_seconds.is_none())
        || input.weight_kg.is_some_and(|weight| weight < 0.0)
    {
        return Ok(response(ErrorCode::BadRequest, "the proposal is invalid"));
    }
    let db = Db::new(env.d1("DB")?);
    let Some(output_key) = db
        .first::<ProposalRow>(&SqlStatement::new(
            "SELECT o.output_key FROM ai_generation g \
             JOIN ai_outputs o ON o.id = g.id \
             WHERE g.id = ? AND g.user_id = ? AND g.kind = 'set_proposal'",
            vec![
                SqlValue::Text(id.clone()),
                SqlValue::Text(authenticated.id.clone()),
            ],
        ))
        .await?
        .map(|row| row.output_key)
    else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let bucket = env.bucket(BUCKET_BINDING)?;
    let Some(object) = bucket.get(&output_key).execute().await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(body) = object.body() else {
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let text = body.text().await?;
    let Ok(mut stored) = serde_json::from_str::<MutableStoredOutput>(&text) else {
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let Some(proposal) = stored.output.proposals.first_mut() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    proposal.weight_kg = input.weight_kg;
    proposal.reps = input.reps;
    proposal.duration_seconds = input.duration_seconds;
    proposal.sets = input.sets;
    proposal.reason = input.reason;
    let updated = proposal.clone();
    let Ok(saved) = serde_json::to_string(&stored) else {
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    bucket.put(&output_key, saved).execute().await?;
    Response::from_json(&updated)
}

#[derive(Debug, Deserialize)]
struct ProposalRow {
    output_key: String,
}

/// 承認するメニューの変更。
#[derive(Debug, Deserialize)]
struct ApplyMenuRequest {
    changes: Vec<MenuChange>,
}

/// 承認した変更の応答。
#[derive(Debug, Serialize)]
struct ApplyMenuResponse {
    applied: usize,
}

/// `POST /api/ai/menu/apply`
///
/// 利用者が承認した変更だけをルーティンへ反映する。
/// セット数の変更は対象の種目を含む全てのルーティンへ、種目の追加は先頭のルーティンへ行う。
pub async fn apply_menu(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: ApplyMenuRequest = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the changes are invalid")),
    };
    let db = Db::new(env.d1("DB")?);
    let mut applied = 0;
    for change in input.changes {
        match menu_change_statements(&db, &authenticated.id, &change).await {
            Ok(statements) if !statements.is_empty() => {
                if let Err(error) = db.batch(&statements).await {
                    console_error!("failed to apply the menu change: {error}");
                    return Ok(response(ErrorCode::Internal, "internal error"));
                }
                applied += 1;
            }
            Ok(_) => {}
            Err(error) => {
                console_error!("failed to build the menu change: {error}");
                return Ok(response(ErrorCode::Internal, "internal error"));
            }
        }
    }
    Response::from_json(&ApplyMenuResponse { applied })
}

async fn menu_change_statements(
    db: &Db,
    user_id: &str,
    change: &MenuChange,
) -> Result<Vec<SqlStatement>> {
    let now = crate::auth::now_iso();
    Ok(match change.kind {
        MenuChangeKind::SetIncrease | MenuChangeKind::SetDecrease => {
            let delta = if change.kind == MenuChangeKind::SetIncrease {
                1
            } else {
                -1
            };
            vec![set_count_statement(user_id, change, delta, &now)]
        }
        MenuChangeKind::AddExercise => add_exercise_statements(db, user_id, change, &now).await?,
        MenuChangeKind::ReplaceExercise => {
            replace_exercise_statements(db, user_id, change, &now).await?
        }
        MenuChangeKind::PartBalance => Vec::new(),
    })
}

fn set_count_statement(user_id: &str, change: &MenuChange, delta: i64, now: &str) -> SqlStatement {
    let set_count = match change.sets {
        Some(sets) => SqlValue::Integer(i64::from(sets.clamp(1, 20))),
        None => SqlValue::Null,
    };
    SqlStatement::new(
        "UPDATE routine_exercise_targets \
         SET set_count = COALESCE(?, MAX(1, MIN(20, set_count + ?))), updated_at = ? \
         WHERE user_id = ? AND routine_exercise_id IN \
         (SELECT id FROM routine_exercises WHERE user_id = ? AND exercise_id = ?)",
        vec![
            set_count,
            SqlValue::Integer(delta),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(change.exercise_id.clone()),
        ],
    )
}

async fn add_exercise_statements(
    db: &Db,
    user_id: &str,
    change: &MenuChange,
    now: &str,
) -> Result<Vec<SqlStatement>> {
    let Some(routine_id) = first_routine_id(db, user_id).await? else {
        return Ok(Vec::new());
    };
    let Some(exercise) = crate::exercise::load_master(db, &change.exercise_id).await? else {
        return Ok(Vec::new());
    };
    let Some(position) = next_exercise_position(db, user_id, &routine_id).await? else {
        return Ok(Vec::new());
    };
    let routine_exercise_id = crate::random::random_uuid()?;
    let snapshot = serde_json::to_string(&crate::exercise::snapshot(&exercise))
        .unwrap_or_else(|_| "{}".to_owned());
    Ok(vec![
        SqlStatement::new(
            "INSERT INTO routine_exercises \
             (id, user_id, routine_id, exercise_id, snapshot, position, created_at, updated_at) \
             SELECT ?, ?, ?, ?, ?, ?, ?, ? \
             WHERE EXISTS (SELECT 1 FROM routines WHERE id = ? AND user_id = ?)",
            vec![
                SqlValue::Text(routine_exercise_id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(routine_id.clone()),
                SqlValue::Text(change.exercise_id.clone()),
                SqlValue::Text(snapshot),
                SqlValue::Integer(position),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(routine_id),
                SqlValue::Text(user_id.to_owned()),
            ],
        ),
        routine_target_insert(user_id, &routine_exercise_id, change.sets.unwrap_or(3), now),
    ])
}

async fn replace_exercise_statements(
    db: &Db,
    user_id: &str,
    change: &MenuChange,
    now: &str,
) -> Result<Vec<SqlStatement>> {
    let Some(target_id) = change.target_exercise_id.as_deref() else {
        return Ok(Vec::new());
    };
    let Some(exercise) = crate::exercise::load_master(db, target_id).await? else {
        return Ok(Vec::new());
    };
    let snapshot = serde_json::to_string(&crate::exercise::snapshot(&exercise))
        .unwrap_or_else(|_| "{}".to_owned());
    let mut statements = vec![SqlStatement::new(
        "UPDATE routine_exercises SET exercise_id = ?, snapshot = ?, updated_at = ? \
         WHERE user_id = ? AND exercise_id = ?",
        vec![
            SqlValue::Text(target_id.to_owned()),
            SqlValue::Text(snapshot),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(change.exercise_id.clone()),
        ],
    )];
    if let Some(sets) = change.sets {
        statements.push(SqlStatement::new(
            "UPDATE routine_exercise_targets SET set_count = ?, updated_at = ? \
             WHERE user_id = ? AND routine_exercise_id IN \
             (SELECT id FROM routine_exercises WHERE user_id = ? AND exercise_id = ?)",
            vec![
                SqlValue::Integer(i64::from(sets.clamp(1, 20))),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(target_id.to_owned()),
            ],
        ));
    }
    Ok(statements)
}

async fn first_routine_id(db: &Db, user_id: &str) -> Result<Option<String>> {
    Ok(db
        .first::<IdRow>(&SqlStatement::new(
            "SELECT id FROM routines WHERE user_id = ? ORDER BY position, id LIMIT 1",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .map(|row| row.id))
}

async fn next_exercise_position(db: &Db, user_id: &str, routine_id: &str) -> Result<Option<i64>> {
    Ok(db
        .first::<PositionRow>(&SqlStatement::new(
            "SELECT COALESCE(MAX(position), 0) + 1 AS position FROM routine_exercises \
             WHERE user_id = ? AND routine_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(routine_id.to_owned()),
            ],
        ))
        .await?
        .map(|row| row.position))
}

fn routine_target_insert(
    user_id: &str,
    routine_exercise_id: &str,
    sets: u32,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO routine_exercise_targets \
         (routine_exercise_id, user_id, set_count, target_weight_kg, target_reps, \
          target_duration_seconds, grip, rest_seconds, created_at, updated_at) \
         SELECT ?, ?, ?, NULL, 8, NULL, NULL, 90, ?, ? \
         WHERE EXISTS (SELECT 1 FROM routine_exercises WHERE id = ? AND user_id = ?)",
        vec![
            SqlValue::Text(routine_exercise_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Integer(i64::from(sets.clamp(1, 20))),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(routine_exercise_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

#[derive(Debug, Deserialize)]
struct IdRow {
    id: String,
}

#[derive(Debug, Deserialize)]
struct PositionRow {
    position: i64,
}
