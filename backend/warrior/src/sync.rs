//! 端末から送られた同期の操作の適用。
//!
//! 操作 ID の保存とトレーニングの適用を同じ `batch()` に入れ、同じ操作 ID の再送では
//! 状態を変えずに成功を返す (ADR-002、ADR-015)。
//! 記録中のトレーニングへの書き込みは、編集権を持つ端末だけに許可する (ADR-003)。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::ownership::{decide_write, WriteDecision};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    validate_operation, Operation, OperationKind, OperationResponse, SessionBackfillPayload,
    SessionCreatePayload, SessionMemoPayload, SessionUpdatePayload, ValidatedPayload,
};
use warrior_core::training_session::{PersonalBestItem, SessionSource};
use worker::d1::D1Result;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::{changed_rows, Db};
use crate::error::{response, ErrorCode};
use crate::ownership;

mod exercise;

pub(crate) use exercise::ownership_guard_statement;

#[derive(Debug, Deserialize)]
struct AppliedRow {
    user_id: String,
    training_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SessionOwnerRow {
    pub(crate) user_id: String,
    pub(crate) owner_terminal_id: Option<String>,
    pub(crate) finished_at: Option<String>,
}
/// 更新の SQL が所有者に課す条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnerCondition<'a> {
    /// 所有者が未設定の記録中の行を、この端末のものにする。
    Claim(&'a str),
    /// この端末が所有者である行と、終了した行を更新する。
    Owned(&'a str),
    /// 終了した行だけを更新する。端末トークンの無い要求で使う。
    Finished,
}

/// 操作の適用の結果。書き込みを拒否する場合は応答を返す。
pub(crate) enum BatchOutcome {
    /// 適用する文の組。
    Apply(Vec<SqlStatement>),
    /// 適用しない理由の応答。
    Reject(Response),
}

/// `POST /api/sync/operations`
pub async fn apply_operation(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };

    let operation: Operation = match request.json().await {
        Ok(operation) => operation,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "operation is invalid")),
    };
    let validated = match validate_operation(&operation) {
        Ok(validated) => validated,
        Err(error) => return Ok(response(ErrorCode::BadRequest, error.code())),
    };

    let db = Db::new(env.d1("DB")?);
    let terminal = ownership::ensure(&db, &request, &authenticated.id).await?;
    let now = datetime::format_utc(crate::auth::now_seconds());

    // 影響する自己ベストの組は、変更を当てる前の行から決める。
    // 記録中のトレーニングは自己ベストを表に入れる途中なので、終了したトレーニングと、
    // トレーニングの削除だけを計算し直す。

    if let Some(existing) = applied(&db, &operation.operation_id).await? {
        if existing.user_id == authenticated.id && existing.training_id == operation.training_id {
            let bests =
                crate::set::personal_bests_after_completion(&db, &authenticated.id, &validated)
                    .await;
            return applied_response("already_applied", bests.held);
        }
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }

    let finished = session_row(&db, &operation.training_id)
        .await
        .ok()
        .flatten()
        .is_some_and(|session| session.finished_at.is_some());
    let rebuild_groups =
        match crate::personal_best::groups_before_operation(&db, &authenticated.id, &validated)
            .await
        {
            Ok(groups) => groups,
            Err(error) => {
                console_error!("failed to read the affected personal best groups: {error}");
                Vec::new()
            }
        };
    let rebuild_groups = if finished || operation.kind == OperationKind::TrainingSessionDelete {
        rebuild_groups
    } else {
        Vec::new()
    };
    let statements = match operation.kind {
        OperationKind::TrainingSessionCreate => {
            create_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::TrainingSessionBackfill => {
            backfill_batch(&db, &authenticated.id, &validated, &now).await?
        }
        OperationKind::TrainingSessionDelete => {
            delete_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::TrainingSessionUpdate => {
            update_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::TrainingSessionMemo => {
            session_memo_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::TrainingExerciseCreate
        | OperationKind::TrainingExerciseDelete
        | OperationKind::TrainingExerciseReorder => {
            exercise::exercise_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::TrainingExerciseMemo => {
            exercise::exercise_memo_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::SetCreate
        | OperationKind::SetUpdate
        | OperationKind::SetDelete
        | OperationKind::SetReorder
        | OperationKind::SetComplete => {
            crate::set::set_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::SupersetCreate
        | OperationKind::SupersetDelete
        | OperationKind::SupersetReorder => {
            crate::superset::superset_batch(
                &db,
                &authenticated.id,
                terminal.as_deref(),
                &validated,
                &now,
            )
            .await?
        }
        OperationKind::RoutineCreate
        | OperationKind::RoutineUpdate
        | OperationKind::RoutineDelete
        | OperationKind::RoutineCopy => {
            crate::routine::routine_batch(&db, &authenticated.id, &validated, &now).await?
        }
    };
    let statements = match statements {
        BatchOutcome::Apply(statements) => statements,
        BatchOutcome::Reject(rejection) => return Ok(rejection),
    };

    let results = match db.batch(&statements).await {
        Ok(results) => results,
        Err(error) => {
            console_error!("failed to apply the operation: {error}");
            // 同じ操作が同時に送られた場合、先に適用した要求が操作 ID を残している。
            if let Some(existing) = applied(&db, &operation.operation_id).await? {
                if existing.user_id == authenticated.id
                    && existing.training_id == operation.training_id
                {
                    let bests = crate::set::personal_bests_after_completion(
                        &db,
                        &authenticated.id,
                        &validated,
                    )
                    .await;
                    return applied_response("already_applied", bests.held);
                }
            }
            if is_superset_member_conflict(&error) {
                return Ok(response(
                    ErrorCode::Conflict,
                    "a superset member already belongs to a group",
                ));
            }
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let changed = match batch_changes(&results) {
        Ok(changed) => changed,
        Err(error) => {
            console_error!("failed to read the changed rows: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if changed == 0 {
        return reject_or_reapply(&db, &authenticated.id, &operation, &validated, &now).await;
    }

    let bests = crate::set::personal_bests_after_operation(
        &db,
        &authenticated.id,
        &validated,
        &rebuild_groups,
    )
    .await;
    applied_response("applied", bests.updated)
}

/// スーパーセットの種目の一意制約に違反したか。
fn is_superset_member_conflict(error: &worker::Error) -> bool {
    let message = error.to_string();
    message.contains("UNIQUE constraint failed") && message.contains("superset_members")
}

/// batch の最初の文が変更した行数を返す。
///
/// 作成と更新はトレーニングへの適用、種目とセットは編集権の確認とトレーニングの
/// 更新時刻の更新を最初の文に置く。
fn batch_changes(results: &[D1Result]) -> Result<usize> {
    let result = results
        .first()
        .ok_or_else(|| worker::Error::RustError("the batch returned no result".into()))?;
    changed_rows(result)
}

/// 適用が 0 行だったときに、再取得した行から次の応答を決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZeroChangeOutcome {
    /// 終了済みである。終了後の更新として再適用する。
    ReapplyFinished,
    /// 同じ利用者の行をほかの端末が所有している。
    Forbidden,
    /// 行が無い、または別の利用者の行である。
    NotFound,
}

fn zero_change_outcome(
    kind: OperationKind,
    session: Option<&SessionOwnerRow>,
    user_id: &str,
) -> ZeroChangeOutcome {
    match kind {
        OperationKind::TrainingSessionCreate => match create_rejection(session, user_id) {
            CreateRejection::Forbidden => ZeroChangeOutcome::Forbidden,
            CreateRejection::NotFound => ZeroChangeOutcome::NotFound,
        },
        OperationKind::TrainingSessionBackfill => match create_rejection(session, user_id) {
            CreateRejection::Forbidden => ZeroChangeOutcome::Forbidden,
            CreateRejection::NotFound => ZeroChangeOutcome::NotFound,
        },
        OperationKind::TrainingSessionDelete => ZeroChangeOutcome::NotFound,
        OperationKind::TrainingSessionUpdate
        | OperationKind::TrainingSessionMemo
        | OperationKind::TrainingExerciseMemo => match session {
            Some(session) if session.user_id == user_id && session.finished_at.is_some() => {
                ZeroChangeOutcome::ReapplyFinished
            }
            Some(session) if session.user_id == user_id => ZeroChangeOutcome::Forbidden,
            _ => ZeroChangeOutcome::NotFound,
        },
        OperationKind::TrainingExerciseCreate
        | OperationKind::TrainingExerciseDelete
        | OperationKind::TrainingExerciseReorder
        | OperationKind::SetCreate
        | OperationKind::SetUpdate
        | OperationKind::SetDelete
        | OperationKind::SetReorder
        | OperationKind::SetComplete
        | OperationKind::SupersetCreate
        | OperationKind::SupersetDelete
        | OperationKind::SupersetReorder => match session {
            Some(session) if session.user_id == user_id => ZeroChangeOutcome::Forbidden,
            _ => ZeroChangeOutcome::NotFound,
        },
        OperationKind::RoutineCreate
        | OperationKind::RoutineUpdate
        | OperationKind::RoutineDelete
        | OperationKind::RoutineCopy => ZeroChangeOutcome::NotFound,
    }
}

/// 適用が 0 行だった操作を、再取得した行から拒否するか、終了後の更新として再適用する。
///
/// 判定の後、文を実行するまでにほかの端末が所有者を変えるか終了していると、
/// 更新の条件が 0 行になる。所有者の無い記録を claim している間に終了した場合は、
/// 終了後の更新として適用できる。
async fn reject_or_reapply(
    db: &Db,
    user_id: &str,
    operation: &Operation,
    validated: &warrior_core::sync::ValidatedOperation,
    now: &str,
) -> Result<Response> {
    let session = session_row(db, &operation.training_id).await?;
    match zero_change_outcome(operation.kind, session.as_ref(), user_id) {
        ZeroChangeOutcome::ReapplyFinished => match &validated.payload {
            ValidatedPayload::Update(payload) => {
                reapply_finished_update(db, user_id, operation, payload, now).await
            }
            ValidatedPayload::SessionMemo(payload) => {
                reapply_finished_session_memo(db, user_id, operation, payload, now).await
            }
            ValidatedPayload::ExerciseMemo(payload) => {
                exercise::reapply_finished_exercise_memo(db, user_id, operation, payload, now).await
            }
            _ => Ok(response(ErrorCode::BadRequest, "operation is invalid")),
        },
        ZeroChangeOutcome::Forbidden => Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        )),
        ZeroChangeOutcome::NotFound => Ok(response(ErrorCode::NotFound, "resource not found")),
    }
}

/// 終了済みのトレーニングに、更新を終了後の書き込みとして適用する。
async fn reapply_finished_update(
    db: &Db,
    user_id: &str,
    operation: &Operation,
    payload: &SessionUpdatePayload,
    now: &str,
) -> Result<Response> {
    let statements = update_statements(user_id, OwnerCondition::Finished, operation, payload, now);
    let results = match db.batch(&statements).await {
        Ok(results) => results,
        Err(error) => {
            console_error!("failed to reapply the update: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let changed = match batch_changes(&results) {
        Ok(changed) => changed,
        Err(error) => {
            console_error!("failed to read the changed rows of the reapplied update: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if changed == 0 {
        return Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        ));
    }
    status_response("applied")
}

/// 0 行だった作成の拒否。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateRejection {
    /// 同じ利用者の行をほかの端末が所有している。
    Forbidden,
    /// 行が無い、または別の利用者の行である。
    NotFound,
}

fn create_rejection(session: Option<&SessionOwnerRow>, user_id: &str) -> CreateRejection {
    match session {
        Some(session) if session.user_id == user_id => CreateRejection::Forbidden,
        _ => CreateRejection::NotFound,
    }
}

async fn create_batch(
    db: &Db,
    user_id: &str,
    terminal_id: Option<&str>,
    validated: &warrior_core::sync::ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    let Some(terminal_id) = terminal_id else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "the terminal token is required",
        )));
    };
    let session = session_row(db, &validated.operation.training_id).await?;
    if let Some(session) = &session {
        if session.user_id != user_id {
            return Ok(BatchOutcome::Reject(response(
                ErrorCode::NotFound,
                "resource not found",
            )));
        }
        // 既に存在する行への作成は、更新と同じく編集権を検査する。
        let decision = write_decision(
            session.finished_at.is_some(),
            session.owner_terminal_id.as_deref(),
            Some(terminal_id),
        );
        if decision == WriteDecision::Forbid {
            return Ok(BatchOutcome::Reject(response(
                ErrorCode::Forbidden,
                "the training is being edited on another terminal",
            )));
        }
    }
    let ValidatedPayload::Create(payload) = &validated.payload else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    Ok(BatchOutcome::Apply(create_statements(
        user_id,
        terminal_id,
        &validated.operation,
        payload,
        now,
    )))
}

/// 日付を指定したトレーニングの追加の batch。
///
/// 過去の記録は終了済みとして作るため、作成の後はどの端末からでも編集できる。
/// 開始時刻は保存せず、指定した日付と所要時間を持つ。
async fn backfill_batch(
    db: &Db,
    user_id: &str,
    validated: &warrior_core::sync::ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    let ValidatedPayload::Backfill(payload) = &validated.payload else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    if let Some(session) = session_row(db, &validated.operation.training_id).await? {
        if session.user_id != user_id {
            return Ok(BatchOutcome::Reject(response(
                ErrorCode::NotFound,
                "resource not found",
            )));
        }
    }
    Ok(BatchOutcome::Apply(backfill_statements(
        user_id,
        &validated.operation,
        payload,
        now,
    )))
}

fn backfill_statements(
    user_id: &str,
    operation: &Operation,
    payload: &SessionBackfillPayload,
    now: &str,
) -> Vec<SqlStatement> {
    vec![
        SqlStatement::new(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, finished_at, duration_seconds, total_weight_kg, \
              exercise_count, set_count, memo, source_kind, routine_id, program_day_id, \
              custom_program_day_id, owner_terminal_id, created_at, updated_at) \
             VALUES (?, ?, ?, NULL, ?, ?, 0, 0, 0, ?, ?, NULL, NULL, NULL, NULL, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
             date = excluded.date, finished_at = excluded.finished_at, \
             duration_seconds = excluded.duration_seconds, memo = excluded.memo, \
             updated_at = excluded.updated_at \
             WHERE training_sessions.user_id = excluded.user_id",
            vec![
                SqlValue::Text(payload.id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(payload.date.clone()),
                SqlValue::Text(now.to_owned()),
                SqlValue::Integer(payload.duration_seconds),
                SqlValue::Text(payload.memo.clone()),
                SqlValue::Text(SessionSource::Empty.as_str().to_owned()),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(now.to_owned()),
            ],
        ),
        applied_statement(user_id, operation, now),
    ]
}

/// トレーニングの削除の batch。
///
/// 先頭の文で編集権を確認しながらトレーニングを削除し、その変更の後に操作 ID を保存する。
/// 記録中のトレーニングは編集権を持つ端末だけが削除できる。
async fn delete_batch(
    db: &Db,
    user_id: &str,
    terminal_id: Option<&str>,
    validated: &warrior_core::sync::ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    let ValidatedPayload::Delete(_) = &validated.payload else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    let Some(session) = session_row(db, &validated.operation.training_id).await? else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    };
    if session.user_id != user_id {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    }
    let finished = session.finished_at.is_some();
    let decision = write_decision(finished, session.owner_terminal_id.as_deref(), terminal_id);
    let Some(owner) = owner_condition(decision, terminal_id) else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        )));
    };
    Ok(BatchOutcome::Apply(delete_statements(
        user_id,
        owner,
        &validated.operation,
        now,
    )))
}

fn delete_statements(
    user_id: &str,
    owner: OwnerCondition<'_>,
    operation: &Operation,
    now: &str,
) -> Vec<SqlStatement> {
    let training_id = SqlValue::Text(operation.training_id.clone());
    let user = SqlValue::Text(user_id.to_owned());
    let mut session_bindings = vec![training_id.clone(), user.clone()];
    let mut session_condition =
        String::from("training_sessions.id = ? AND training_sessions.user_id = ?");
    match owner {
        OwnerCondition::Claim(_) => {
            session_condition.push_str(
                " AND training_sessions.finished_at IS NULL \
                 AND training_sessions.owner_terminal_id IS NULL",
            );
        }
        OwnerCondition::Owned(terminal_id) => {
            session_condition.push_str(
                " AND (training_sessions.finished_at IS NOT NULL \
                 OR training_sessions.owner_terminal_id = ?)",
            );
            session_bindings.push(SqlValue::Text(terminal_id.to_owned()));
        }
        OwnerCondition::Finished => {
            session_condition.push_str(" AND training_sessions.finished_at IS NOT NULL");
        }
    }
    let guard = |sql: &str, mut bindings: Vec<SqlValue>| {
        bindings.push(SqlValue::Text(operation.operation_id.clone()));
        bindings.push(user.clone());
        SqlStatement::new(
            format!(
                "{sql} AND EXISTS (SELECT 1 FROM applied_operations ao \
                 WHERE ao.operation_id = ? AND ao.user_id = ?)"
            ),
            bindings,
        )
    };
    let with_training = || vec![user.clone(), training_id.clone(), user.clone()];
    vec![
        SqlStatement::new(
            format!("DELETE FROM training_sessions WHERE {session_condition}"),
            session_bindings,
        ),
        applied_statement(user_id, operation, now),
        guard(
            "DELETE FROM set_segments WHERE user_id = ? AND set_id IN \
             (SELECT s.id FROM sets s JOIN training_exercises te \
              ON te.id = s.training_exercise_id \
              WHERE te.training_id = ? AND te.user_id = ?)",
            with_training(),
        ),
        guard(
            "DELETE FROM sets WHERE user_id = ? AND training_exercise_id IN \
             (SELECT id FROM training_exercises WHERE training_id = ? AND user_id = ?)",
            with_training(),
        ),
        guard(
            "DELETE FROM superset_members WHERE user_id = ? \
             AND training_exercise_id IN \
             (SELECT id FROM training_exercises WHERE training_id = ? AND user_id = ?)",
            with_training(),
        ),
        guard(
            "DELETE FROM supersets WHERE user_id = ? AND training_id = ?",
            vec![user.clone(), training_id.clone()],
        ),
        guard(
            "DELETE FROM training_exercises WHERE user_id = ? AND training_id = ?",
            vec![user.clone(), training_id.clone()],
        ),
    ]
}

/// トレーニング全体のメモの batch。
///
/// 記録中は編集権を持つ端末だけが書き込み、終了済みはどの端末でも書き込める。
/// 更新が 0 行だったときは、再取得した行から終了後の書き込みとして再適用する。
async fn session_memo_batch(
    db: &Db,
    user_id: &str,
    terminal_id: Option<&str>,
    validated: &warrior_core::sync::ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    let Some(session) = session_row(db, &validated.operation.training_id).await? else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    };
    if session.user_id != user_id {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    }
    let finished = session.finished_at.is_some();
    let decision = write_decision(finished, session.owner_terminal_id.as_deref(), terminal_id);
    let Some(owner) = owner_condition(decision, terminal_id) else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        )));
    };
    let ValidatedPayload::SessionMemo(payload) = &validated.payload else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    Ok(BatchOutcome::Apply(vec![
        session_memo_statement(user_id, owner, &validated.operation, payload, now),
        applied_statement(user_id, &validated.operation, now),
    ]))
}

/// 終了済みのトレーニングに、トレーニング全体のメモを終了後の書き込みとして適用する。
async fn reapply_finished_session_memo(
    db: &Db,
    user_id: &str,
    operation: &Operation,
    payload: &SessionMemoPayload,
    now: &str,
) -> Result<Response> {
    let statements = vec![
        session_memo_statement(user_id, OwnerCondition::Finished, operation, payload, now),
        applied_statement(user_id, operation, now),
    ];
    let results = match db.batch(&statements).await {
        Ok(results) => results,
        Err(error) => {
            console_error!("failed to reapply the session memo: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let changed = match batch_changes(&results) {
        Ok(changed) => changed,
        Err(error) => {
            console_error!("failed to read the changed rows of the reapplied memo: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if changed == 0 {
        return Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        ));
    }
    status_response("applied")
}

async fn update_batch(
    db: &Db,
    user_id: &str,
    terminal_id: Option<&str>,
    validated: &warrior_core::sync::ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    let Some(session) = session_row(db, &validated.operation.training_id).await? else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    };
    if session.user_id != user_id {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    }
    let finished = session.finished_at.is_some();
    let decision = write_decision(finished, session.owner_terminal_id.as_deref(), terminal_id);
    let Some(owner) = owner_condition(decision, terminal_id) else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        )));
    };
    let ValidatedPayload::Update(payload) = &validated.payload else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    Ok(BatchOutcome::Apply(update_statements(
        user_id,
        owner,
        &validated.operation,
        payload,
        now,
    )))
}

/// 端末トークンも含めて、書き込みの許可を判定する。
///
/// 終了したトレーニングは端末トークンの無い要求でも書き込める。
pub(crate) fn write_decision(
    finished: bool,
    owner_terminal_id: Option<&str>,
    terminal_id: Option<&str>,
) -> WriteDecision {
    match terminal_id {
        Some(terminal) => decide_write(finished, owner_terminal_id, terminal),
        None if finished => WriteDecision::Allow,
        None => WriteDecision::Forbid,
    }
}

/// 判定と端末トークンから、更新の SQL に渡す所有者の条件を決める。
///
/// `None` は書き込みを拒否する判定である。
pub(crate) fn owner_condition<'a>(
    decision: WriteDecision,
    terminal_id: Option<&'a str>,
) -> Option<OwnerCondition<'a>> {
    match (decision, terminal_id) {
        (WriteDecision::Claim, Some(terminal_id)) => Some(OwnerCondition::Claim(terminal_id)),
        (WriteDecision::Allow, Some(terminal_id)) => Some(OwnerCondition::Owned(terminal_id)),
        (WriteDecision::Allow, None) => Some(OwnerCondition::Finished),
        (WriteDecision::Claim | WriteDecision::Forbid, None) => None,
        (WriteDecision::Forbid, Some(_)) => None,
    }
}

async fn applied(db: &Db, operation_id: &str) -> Result<Option<AppliedRow>> {
    db.first(&SqlStatement::new(
        "SELECT user_id, training_id FROM applied_operations WHERE operation_id = ?",
        vec![SqlValue::Text(operation_id.to_owned())],
    ))
    .await
}

pub(crate) async fn session_row(db: &Db, training_id: &str) -> Result<Option<SessionOwnerRow>> {
    db.first(&SqlStatement::new(
        "SELECT user_id, owner_terminal_id, finished_at FROM training_sessions WHERE id = ?",
        vec![SqlValue::Text(training_id.to_owned())],
    ))
    .await
}

fn create_statements(
    user_id: &str,
    terminal_id: &str,
    operation: &Operation,
    payload: &SessionCreatePayload,
    now: &str,
) -> Vec<SqlStatement> {
    vec![
        session_create_statement(user_id, terminal_id, payload, now),
        applied_statement(user_id, operation, now),
    ]
}

/// トレーニングの作成の文。ルーティンからの開始もこの文を使う。
pub(crate) fn session_create_statement(
    user_id: &str,
    terminal_id: &str,
    payload: &SessionCreatePayload,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO training_sessions \
         (id, user_id, date, started_at, memo, source_kind, routine_id, program_day_id, \
         custom_program_day_id, owner_terminal_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
         date = excluded.date, started_at = excluded.started_at, memo = excluded.memo, \
         source_kind = excluded.source_kind, routine_id = excluded.routine_id, \
         program_day_id = excluded.program_day_id, \
         custom_program_day_id = excluded.custom_program_day_id, \
         owner_terminal_id = COALESCE(training_sessions.owner_terminal_id, excluded.owner_terminal_id), \
         updated_at = excluded.updated_at \
         WHERE training_sessions.user_id = excluded.user_id \
         AND (training_sessions.finished_at IS NOT NULL \
              OR training_sessions.owner_terminal_id IS NULL \
              OR training_sessions.owner_terminal_id = excluded.owner_terminal_id)",
        vec![
            SqlValue::Text(payload.id.clone()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(payload.date.clone()),
            SqlValue::Text(payload.started_at.clone()),
            SqlValue::Text(payload.memo.clone()),
            SqlValue::Text(payload.source_kind.as_str().to_owned()),
            optional_text(&payload.routine_id),
            optional_text(&payload.program_day_id),
            optional_text(&payload.custom_program_day_id),
            SqlValue::Text(terminal_id.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

/// 直前の適用が変更した行があるときだけ、操作 ID を保存する。
///
/// `changes()` は同じ batch の直前の文が変更した行数を返す。適用と操作 ID の保存を
/// 1 つの batch に入れ、0 行の適用では操作 ID を残さない (ADR-002、ADR-015)。
pub(crate) fn applied_statement(user_id: &str, operation: &Operation, now: &str) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO applied_operations (operation_id, user_id, training_id, kind, applied_at) \
         SELECT ?, ?, ?, ?, ? WHERE changes() > 0",
        vec![
            SqlValue::Text(operation.operation_id.clone()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(operation.training_id.clone()),
            SqlValue::Text(operation.kind.as_str().to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn optional_text(value: &Option<String>) -> SqlValue {
    value.clone().map_or(SqlValue::Null, SqlValue::Text)
}

fn session_memo_statement(
    user_id: &str,
    owner: OwnerCondition<'_>,
    operation: &Operation,
    payload: &SessionMemoPayload,
    now: &str,
) -> SqlStatement {
    let training_id = SqlValue::Text(operation.training_id.clone());
    let user = SqlValue::Text(user_id.to_owned());
    let mut bindings = vec![
        SqlValue::Text(payload.memo.clone()),
        SqlValue::Text(now.to_owned()),
    ];
    let sql = match owner {
        OwnerCondition::Claim(terminal_id) => {
            bindings.push(SqlValue::Text(terminal_id.to_owned()));
            bindings.push(training_id);
            bindings.push(user);
            "UPDATE training_sessions SET memo = ?, updated_at = ?, owner_terminal_id = ? \
             WHERE id = ? AND user_id = ? AND finished_at IS NULL AND owner_terminal_id IS NULL"
        }
        OwnerCondition::Owned(terminal_id) => {
            bindings.push(training_id);
            bindings.push(user);
            bindings.push(SqlValue::Text(terminal_id.to_owned()));
            "UPDATE training_sessions SET memo = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? \
             AND (finished_at IS NOT NULL OR owner_terminal_id = ?)"
        }
        OwnerCondition::Finished => {
            bindings.push(training_id);
            bindings.push(user);
            "UPDATE training_sessions SET memo = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? AND finished_at IS NOT NULL"
        }
    };
    SqlStatement::new(sql, bindings)
}

fn update_statements(
    user_id: &str,
    owner: OwnerCondition<'_>,
    operation: &Operation,
    payload: &SessionUpdatePayload,
    now: &str,
) -> Vec<SqlStatement> {
    vec![
        update_statement(user_id, owner, operation, payload, now),
        applied_statement(user_id, operation, now),
    ]
}

fn update_statement(
    user_id: &str,
    owner: OwnerCondition<'_>,
    operation: &Operation,
    payload: &SessionUpdatePayload,
    now: &str,
) -> SqlStatement {
    let training_id = SqlValue::Text(operation.training_id.clone());
    let user = SqlValue::Text(user_id.to_owned());
    let mut bindings = vec![
        SqlValue::Text(payload.date.clone()),
        SqlValue::Text(payload.started_at.clone()),
        SqlValue::Text(payload.memo.clone()),
        SqlValue::Text(now.to_owned()),
    ];
    let sql = match owner {
        OwnerCondition::Claim(terminal_id) => {
            bindings.push(SqlValue::Text(terminal_id.to_owned()));
            bindings.push(training_id);
            bindings.push(user);
            "UPDATE training_sessions SET date = ?, started_at = ?, memo = ?, updated_at = ?, \
             owner_terminal_id = ? \
             WHERE id = ? AND user_id = ? AND finished_at IS NULL AND owner_terminal_id IS NULL"
        }
        OwnerCondition::Owned(terminal_id) => {
            bindings.push(training_id);
            bindings.push(user);
            bindings.push(SqlValue::Text(terminal_id.to_owned()));
            "UPDATE training_sessions SET date = ?, started_at = ?, memo = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? \
             AND (finished_at IS NOT NULL OR owner_terminal_id = ?)"
        }
        OwnerCondition::Finished => {
            bindings.push(training_id);
            bindings.push(user);
            "UPDATE training_sessions SET date = ?, started_at = ?, memo = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? AND finished_at IS NOT NULL"
        }
    };
    SqlStatement::new(sql, bindings)
}

pub(crate) fn status_response(status: &str) -> Result<Response> {
    Response::from_json(&serde_json::json!({ "status": status }))
}

fn applied_response(status: &str, personal_bests: Vec<PersonalBestItem>) -> Result<Response> {
    Response::from_json(&OperationResponse {
        status: status.to_owned(),
        personal_bests,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use warrior_core::ownership::WriteDecision;
    use warrior_core::sql::SqlValue;
    use warrior_core::sync::{Operation, OperationKind};
    use warrior_core::training_session::SessionSource;

    use super::exercise::ownership_guard_statement;

    use super::{
        backfill_statements, create_statements, delete_statements, owner_condition,
        session_memo_statement, update_statements, write_decision, zero_change_outcome,
        OwnerCondition, SessionOwnerRow, ZeroChangeOutcome,
    };

    const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
    const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";
    const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3304";

    fn create_operation() -> Operation {
        Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 1,
            kind: OperationKind::TrainingSessionCreate,
            parent_operation_id: None,
            payload: json!({}),
        }
    }

    fn update_operation() -> Operation {
        Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 2,
            kind: OperationKind::TrainingSessionUpdate,
            parent_operation_id: None,
            payload: json!({}),
        }
    }

    fn create_payload() -> warrior_core::sync::SessionCreatePayload {
        warrior_core::sync::SessionCreatePayload {
            id: TRAINING_ID.to_owned(),
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: "メモ".to_owned(),
            source_kind: SessionSource::Empty,
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
        }
    }

    fn update_payload() -> warrior_core::sync::SessionUpdatePayload {
        warrior_core::sync::SessionUpdatePayload {
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: "メモ".to_owned(),
        }
    }

    #[test]
    fn the_session_memo_statement_only_writes_the_memo_and_the_timestamp() {
        let statement = session_memo_statement(
            USER_ID,
            OwnerCondition::Owned(TERMINAL_ID),
            &update_operation(),
            &warrior_core::sync::SessionMemoPayload {
                training_id: TRAINING_ID.to_owned(),
                memo: "腰に痛みがある".to_owned(),
            },
            "2026-11-12T00:00:00Z",
        );
        assert!(statement
            .sql()
            .contains("UPDATE training_sessions SET memo = ?, updated_at = ?"));
        assert!(!statement.sql().contains("date = ?"));
        assert!(!statement.sql().contains("started_at = ?"));
        assert!(statement.sql().contains("owner_terminal_id = ?"));
        assert_eq!(
            statement.bindings()[0],
            SqlValue::Text("腰に痛みがある".to_owned())
        );
        assert_eq!(
            statement.bindings()[2],
            SqlValue::Text(TRAINING_ID.to_owned())
        );
        assert_eq!(
            statement.bindings()[4],
            SqlValue::Text(TERMINAL_ID.to_owned())
        );
    }

    #[test]
    fn a_zero_change_operation_reapplies_only_a_finished_update() {
        let recording = SessionOwnerRow {
            user_id: USER_ID.to_owned(),
            owner_terminal_id: Some("another-terminal".to_owned()),
            finished_at: None,
        };
        let mut finished = recording.clone();
        finished.finished_at = Some("2026-11-12T04:00:00Z".to_owned());
        let foreign = SessionOwnerRow {
            user_id: "another-user".to_owned(),
            owner_terminal_id: Some(TERMINAL_ID.to_owned()),
            finished_at: Some("2026-11-12T04:00:00Z".to_owned()),
        };
        assert_eq!(
            zero_change_outcome(
                OperationKind::TrainingSessionCreate,
                Some(&recording),
                USER_ID
            ),
            ZeroChangeOutcome::Forbidden
        );
        assert_eq!(
            zero_change_outcome(
                OperationKind::TrainingSessionCreate,
                Some(&foreign),
                USER_ID
            ),
            ZeroChangeOutcome::NotFound
        );
        assert_eq!(
            zero_change_outcome(OperationKind::TrainingSessionCreate, None, USER_ID),
            ZeroChangeOutcome::NotFound
        );
        assert_eq!(
            zero_change_outcome(
                OperationKind::TrainingSessionUpdate,
                Some(&finished),
                USER_ID
            ),
            ZeroChangeOutcome::ReapplyFinished
        );
        assert_eq!(
            zero_change_outcome(
                OperationKind::TrainingSessionUpdate,
                Some(&recording),
                USER_ID
            ),
            ZeroChangeOutcome::Forbidden
        );
        assert_eq!(
            zero_change_outcome(
                OperationKind::TrainingSessionUpdate,
                Some(&foreign),
                USER_ID
            ),
            ZeroChangeOutcome::NotFound
        );
        assert_eq!(
            zero_change_outcome(OperationKind::TrainingSessionUpdate, None, USER_ID),
            ZeroChangeOutcome::NotFound
        );
    }

    #[test]
    fn the_create_batch_applies_the_session_before_the_operation_id() {
        let statements = create_statements(
            USER_ID,
            TERMINAL_ID,
            &create_operation(),
            &create_payload(),
            "2026-11-12T00:00:00Z",
        );
        assert_eq!(statements.len(), 2);
        assert!(statements[0].sql().contains("ON CONFLICT(id)"));
        assert!(statements[0].sql().contains("owner_terminal_id"));
        assert!(statements[0]
            .sql()
            .contains("training_sessions.owner_terminal_id = excluded.owner_terminal_id"));
        assert!(statements[0]
            .sql()
            .contains("training_sessions.owner_terminal_id IS NULL"));
        assert!(statements[0]
            .sql()
            .contains("training_sessions.finished_at IS NOT NULL"));
        assert!(statements[1].sql().contains("applied_operations"));
        assert!(statements[1].sql().contains("changes() > 0"));
        assert_eq!(
            statements[0].bindings()[5],
            SqlValue::Text("empty".to_owned())
        );
        assert_eq!(
            statements[0].bindings()[9],
            SqlValue::Text(TERMINAL_ID.to_owned())
        );
    }

    #[test]
    fn the_update_batch_records_the_operation_and_scopes_the_update_to_the_user() {
        let statements = update_statements(
            USER_ID,
            OwnerCondition::Owned(TERMINAL_ID),
            &update_operation(),
            &update_payload(),
            "2026-11-12T00:00:00Z",
        );
        assert_eq!(statements.len(), 2);
        assert!(statements[0].sql().contains("WHERE id = ? AND user_id = ?"));
        assert!(statements[0].sql().contains("owner_terminal_id = ?"));
        assert_eq!(
            statements[0].bindings()[4],
            SqlValue::Text(TRAINING_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[5],
            SqlValue::Text(USER_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[6],
            SqlValue::Text(TERMINAL_ID.to_owned()),
            "the owner condition takes the terminal id after the user and the training"
        );
        assert!(statements[1].sql().contains("applied_operations"));
        assert!(statements[1].sql().contains("changes() > 0"));
        assert_eq!(
            statements[1].bindings()[0],
            SqlValue::Text(OPERATION_ID.to_owned())
        );
    }

    #[test]
    fn a_claiming_update_sets_the_owner_only_on_an_unowned_recording_session() {
        let statements = update_statements(
            USER_ID,
            OwnerCondition::Claim(TERMINAL_ID),
            &update_operation(),
            &update_payload(),
            "2026-11-12T00:00:00Z",
        );
        let sql = statements[0].sql();
        assert!(sql.contains("owner_terminal_id = ?"));
        assert!(sql.contains("finished_at IS NULL AND owner_terminal_id IS NULL"));
        assert_eq!(
            statements[0].bindings()[4],
            SqlValue::Text(TERMINAL_ID.to_owned())
        );
        assert_eq!(statements[0].bindings().len(), 7);
    }

    #[test]
    fn an_update_without_a_terminal_only_touches_a_finished_session() {
        let statements = update_statements(
            USER_ID,
            OwnerCondition::Finished,
            &update_operation(),
            &update_payload(),
            "2026-11-12T00:00:00Z",
        );
        assert!(statements[0].sql().contains("finished_at IS NOT NULL"));
        assert_eq!(statements[0].bindings().len(), 6);
    }

    #[test]
    fn the_backfill_batch_creates_a_finished_session_without_a_start_time() {
        let operation = Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 1,
            kind: OperationKind::TrainingSessionBackfill,
            parent_operation_id: None,
            payload: json!({
                "id": TRAINING_ID,
                "date": "2026-11-12",
                "duration_seconds": 3600,
                "memo": "過去の記録"
            }),
        };
        let payload = warrior_core::sync::SessionBackfillPayload {
            id: TRAINING_ID.to_owned(),
            date: "2026-11-12".to_owned(),
            duration_seconds: 3_600,
            memo: "過去の記録".to_owned(),
        };
        let statements = backfill_statements(USER_ID, &operation, &payload, "2026-11-12T00:00:00Z");
        assert_eq!(statements.len(), 2);
        let sql = statements[0].sql();
        assert!(sql.contains("VALUES (?, ?, ?, NULL, ?"));
        assert!(sql.contains("duration_seconds"));
        assert!(sql.contains("training_sessions.user_id = excluded.user_id"));
        assert_eq!(
            statements[0].bindings()[2],
            SqlValue::Text("2026-11-12".to_owned())
        );
        assert_eq!(
            statements[0].bindings()[4],
            SqlValue::Integer(3_600),
            "the duration follows the date and the finish time"
        );
        assert!(statements[1].sql().contains("changes() > 0"));
    }

    #[test]
    fn the_delete_batch_records_the_operation_before_the_children() {
        let operation = Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 6,
            kind: OperationKind::TrainingSessionDelete,
            parent_operation_id: None,
            payload: json!({ "training_id": TRAINING_ID }),
        };
        let statements = delete_statements(
            USER_ID,
            OwnerCondition::Finished,
            &operation,
            "2026-11-12T00:00:00Z",
        );
        assert_eq!(statements.len(), 7);
        assert!(statements[0]
            .sql()
            .contains("DELETE FROM training_sessions"));
        assert!(statements[0].sql().contains("finished_at IS NOT NULL"));
        assert!(statements[1].sql().contains("applied_operations"));
        assert!(statements[1].sql().contains("changes() > 0"));
        for statement in &statements[2..] {
            assert!(
                statement.sql().contains("applied_operations ao"),
                "the child statements run only after the operation is recorded"
            );
            let bindings = statement.bindings();
            assert_eq!(
                bindings[bindings.len() - 2],
                SqlValue::Text(OPERATION_ID.to_owned())
            );
            assert_eq!(
                bindings[bindings.len() - 1],
                SqlValue::Text(USER_ID.to_owned())
            );
        }
        assert!(statements[2].sql().contains("DELETE FROM set_segments"));
        assert!(statements[3].sql().contains("DELETE FROM sets"));
        assert!(statements[4].sql().contains("DELETE FROM superset_members"));
        assert!(statements[5].sql().contains("DELETE FROM supersets"));
        assert!(statements[6]
            .sql()
            .contains("DELETE FROM training_exercises"));
    }

    #[test]
    fn a_zero_change_delete_is_not_found() {
        let finished = SessionOwnerRow {
            user_id: USER_ID.to_owned(),
            owner_terminal_id: None,
            finished_at: Some("2026-11-12T04:00:00Z".to_owned()),
        };
        assert_eq!(
            zero_change_outcome(
                OperationKind::TrainingSessionDelete,
                Some(&finished),
                USER_ID
            ),
            ZeroChangeOutcome::NotFound
        );
    }

    #[test]
    fn a_write_without_a_terminal_token_is_allowed_only_after_the_finish() {
        assert_eq!(write_decision(false, None, None), WriteDecision::Forbid);
        assert_eq!(
            write_decision(false, Some(TERMINAL_ID), None),
            WriteDecision::Forbid
        );
        assert_eq!(write_decision(true, None, None), WriteDecision::Allow);
        assert_eq!(
            write_decision(false, None, Some(TERMINAL_ID)),
            WriteDecision::Claim
        );
    }

    #[test]
    fn the_owner_condition_follows_the_write_decision() {
        assert_eq!(
            owner_condition(WriteDecision::Claim, Some(TERMINAL_ID)),
            Some(OwnerCondition::Claim(TERMINAL_ID))
        );
        assert_eq!(
            owner_condition(WriteDecision::Allow, Some(TERMINAL_ID)),
            Some(OwnerCondition::Owned(TERMINAL_ID))
        );
        assert_eq!(
            owner_condition(WriteDecision::Allow, None),
            Some(OwnerCondition::Finished)
        );
        assert_eq!(
            owner_condition(WriteDecision::Forbid, Some(TERMINAL_ID)),
            None
        );
        assert_eq!(owner_condition(WriteDecision::Claim, None), None);
    }

    #[test]
    fn the_ownership_guard_uses_the_same_conditions_as_an_update() {
        let claim = ownership_guard_statement(
            USER_ID,
            OwnerCondition::Claim(TERMINAL_ID),
            TRAINING_ID,
            "2026-11-12T00:00:00Z",
        );
        assert!(claim.sql().contains("owner_terminal_id = ?"));
        assert!(claim
            .sql()
            .contains("finished_at IS NULL AND owner_terminal_id IS NULL"));
        assert_eq!(claim.bindings()[0], SqlValue::Text(TERMINAL_ID.to_owned()));

        let owned = ownership_guard_statement(
            USER_ID,
            OwnerCondition::Owned(TERMINAL_ID),
            TRAINING_ID,
            "2026-11-12T00:00:00Z",
        );
        assert!(owned
            .sql()
            .contains("(finished_at IS NOT NULL OR owner_terminal_id = ?)"));
        assert_eq!(owned.bindings().len(), 4);

        let finished = ownership_guard_statement(
            USER_ID,
            OwnerCondition::Finished,
            TRAINING_ID,
            "2026-11-12T00:00:00Z",
        );
        assert!(finished.sql().contains("finished_at IS NOT NULL"));
        assert_eq!(finished.bindings().len(), 3);
    }

    #[test]
    fn a_zero_change_exercise_operation_is_rejected_by_the_session_state() {
        let recording = SessionOwnerRow {
            user_id: USER_ID.to_owned(),
            owner_terminal_id: Some("another-terminal".to_owned()),
            finished_at: None,
        };
        let foreign = SessionOwnerRow {
            user_id: "another-user".to_owned(),
            owner_terminal_id: None,
            finished_at: None,
        };
        for kind in [
            OperationKind::TrainingExerciseCreate,
            OperationKind::TrainingExerciseDelete,
            OperationKind::TrainingExerciseReorder,
            OperationKind::SetCreate,
            OperationKind::SetUpdate,
            OperationKind::SetDelete,
            OperationKind::SetReorder,
            OperationKind::SetComplete,
            OperationKind::SupersetCreate,
            OperationKind::SupersetDelete,
            OperationKind::SupersetReorder,
        ] {
            assert_eq!(
                zero_change_outcome(kind, Some(&recording), USER_ID),
                ZeroChangeOutcome::Forbidden
            );
            assert_eq!(
                zero_change_outcome(kind, Some(&foreign), USER_ID),
                ZeroChangeOutcome::NotFound
            );
            assert_eq!(
                zero_change_outcome(kind, None, USER_ID),
                ZeroChangeOutcome::NotFound
            );
        }
    }
}
