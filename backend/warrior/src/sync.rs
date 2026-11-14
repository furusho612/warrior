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
    validate_operation, Operation, OperationKind, SessionCreatePayload, SessionUpdatePayload,
    ValidatedPayload,
};
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

    if let Some(existing) = applied(&db, &operation.operation_id).await? {
        if existing.user_id == authenticated.id && existing.training_id == operation.training_id {
            return status_response("already_applied");
        }
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }

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
                    return status_response("already_applied");
                }
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

    status_response("applied")
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
        OperationKind::TrainingSessionUpdate => match session {
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
        ZeroChangeOutcome::ReapplyFinished => {
            let ValidatedPayload::Update(payload) = &validated.payload else {
                return Ok(response(ErrorCode::BadRequest, "operation is invalid"));
            };
            reapply_finished_update(db, user_id, operation, payload, now).await
        }
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
        ),
        applied_statement(user_id, operation, now),
    ]
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

fn status_response(status: &str) -> Result<Response> {
    Response::from_json(&serde_json::json!({ "status": status }))
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
        create_statements, owner_condition, update_statements, write_decision, zero_change_outcome,
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
