//! 端末から送られた同期の操作の適用。
//!
//! 操作 ID の保存とトレーニングの適用を同じ `batch()` に入れ、同じ操作 ID の再送では
//! 状態を変えずに成功を返す (ADR-002、ADR-015)。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    validate_operation, Operation, OperationKind, SessionCreatePayload, SessionUpdatePayload,
    ValidatedPayload,
};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};

#[derive(Debug, Deserialize)]
struct AppliedRow {
    user_id: String,
    training_id: String,
}

#[derive(Debug, Deserialize)]
struct SessionOwnerRow {
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct SessionIdRow {
    #[serde(rename = "id")]
    _id: String,
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
    if let Some(existing) = applied(&db, &operation.operation_id).await? {
        if existing.user_id == authenticated.id && existing.training_id == operation.training_id {
            return status_response("already_applied");
        }
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }

    match operation.kind {
        OperationKind::TrainingSessionUpdate => {
            let owned: Option<SessionIdRow> = db
                .first(&SqlStatement::new(
                    "SELECT id FROM training_sessions WHERE id = ? AND user_id = ?",
                    vec![
                        SqlValue::Text(operation.training_id.clone()),
                        SqlValue::Text(authenticated.id.clone()),
                    ],
                ))
                .await?;
            if owned.is_none() {
                return Ok(response(ErrorCode::NotFound, "resource not found"));
            }
        }
        OperationKind::TrainingSessionCreate => {
            let owner: Option<SessionOwnerRow> = db
                .first(&SqlStatement::new(
                    "SELECT user_id FROM training_sessions WHERE id = ?",
                    vec![SqlValue::Text(operation.training_id.clone())],
                ))
                .await?;
            if owner.is_some_and(|row| row.user_id != authenticated.id) {
                return Ok(response(ErrorCode::NotFound, "resource not found"));
            }
        }
    }

    let now = datetime::format_utc(crate::auth::now_seconds());
    let statements = match (&validated.payload, operation.kind) {
        (ValidatedPayload::Create(payload), OperationKind::TrainingSessionCreate) => {
            create_statements(&authenticated.id, &validated.operation, payload, &now)
        }
        (ValidatedPayload::Update(payload), OperationKind::TrainingSessionUpdate) => {
            update_statements(&authenticated.id, &validated.operation, payload, &now)
        }
        _ => return Ok(response(ErrorCode::BadRequest, "operation is invalid")),
    };

    if let Err(error) = db.batch(&statements).await {
        console_error!("failed to apply the operation: {error}");
        // 同じ操作が同時に送られた場合、先に適用した要求が操作 ID を残している。
        if let Some(existing) = applied(&db, &operation.operation_id).await? {
            if existing.user_id == authenticated.id && existing.training_id == operation.training_id
            {
                return status_response("already_applied");
            }
        }
        return Ok(response(ErrorCode::Internal, "internal error"));
    }

    status_response("applied")
}

async fn applied(db: &Db, operation_id: &str) -> Result<Option<AppliedRow>> {
    db.first(&SqlStatement::new(
        "SELECT user_id, training_id FROM applied_operations WHERE operation_id = ?",
        vec![SqlValue::Text(operation_id.to_owned())],
    ))
    .await
}

fn create_statements(
    user_id: &str,
    operation: &Operation,
    payload: &SessionCreatePayload,
    now: &str,
) -> Vec<SqlStatement> {
    vec![
        SqlStatement::new(
            "INSERT INTO applied_operations (operation_id, user_id, training_id, kind, applied_at) \
             VALUES (?, ?, ?, ?, ?)",
            vec![
                SqlValue::Text(operation.operation_id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(operation.training_id.clone()),
                SqlValue::Text(operation.kind.as_str().to_owned()),
                SqlValue::Text(now.to_owned()),
            ],
        ),
        SqlStatement::new(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, routine_id, program_day_id, \
             custom_program_day_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
             date = excluded.date, started_at = excluded.started_at, memo = excluded.memo, \
             source_kind = excluded.source_kind, routine_id = excluded.routine_id, \
             program_day_id = excluded.program_day_id, \
             custom_program_day_id = excluded.custom_program_day_id, \
             updated_at = excluded.updated_at \
             WHERE training_sessions.user_id = excluded.user_id",
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
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(now.to_owned()),
            ],
        ),
    ]
}

fn optional_text(value: &Option<String>) -> SqlValue {
    value.clone().map_or(SqlValue::Null, SqlValue::Text)
}

fn update_statements(
    user_id: &str,
    operation: &Operation,
    payload: &SessionUpdatePayload,
    now: &str,
) -> Vec<SqlStatement> {
    vec![
        SqlStatement::new(
            "INSERT INTO applied_operations (operation_id, user_id, training_id, kind, applied_at) \
             VALUES (?, ?, ?, ?, ?)",
            vec![
                SqlValue::Text(operation.operation_id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(operation.training_id.clone()),
                SqlValue::Text(operation.kind.as_str().to_owned()),
                SqlValue::Text(now.to_owned()),
            ],
        ),
        SqlStatement::new(
            "UPDATE training_sessions SET date = ?, started_at = ?, memo = ?, updated_at = ? \
             WHERE id = ? AND user_id = ?",
            vec![
                SqlValue::Text(payload.date.clone()),
                SqlValue::Text(payload.started_at.clone()),
                SqlValue::Text(payload.memo.clone()),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(operation.training_id.clone()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ),
    ]
}

fn status_response(status: &str) -> Result<Response> {
    Response::from_json(&serde_json::json!({ "status": status }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use warrior_core::sync::{Operation, OperationKind};

    use super::{create_statements, update_statements};
    use warrior_core::training_session::SessionSource;

    const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
    const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";

    #[test]
    fn the_create_batch_saves_the_operation_and_the_session_together() {
        let operation = Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 1,
            kind: OperationKind::TrainingSessionCreate,
            parent_operation_id: None,
            payload: json!({}),
        };
        let payload = warrior_core::sync::SessionCreatePayload {
            id: TRAINING_ID.to_owned(),
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: "メモ".to_owned(),
            source_kind: SessionSource::Empty,
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
        };
        let statements = create_statements(USER_ID, &operation, &payload, "2026-11-12T00:00:00Z");
        assert_eq!(statements.len(), 2);
        assert!(statements[0].sql().contains("applied_operations"));
        assert!(statements[1].sql().contains("ON CONFLICT(id)"));
        assert!(statements[1]
            .sql()
            .contains("source_kind = excluded.source_kind"));
        assert_eq!(
            statements[1].bindings()[5],
            warrior_core::sql::SqlValue::Text("empty".to_owned())
        );
        assert_eq!(
            statements[1].bindings()[6],
            warrior_core::sql::SqlValue::Null
        );
    }

    #[test]
    fn the_update_batch_scopes_the_update_to_the_user() {
        let operation = Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 2,
            kind: OperationKind::TrainingSessionUpdate,
            parent_operation_id: None,
            payload: json!({}),
        };
        let payload = warrior_core::sync::SessionUpdatePayload {
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: "メモ".to_owned(),
        };
        let statements = update_statements(USER_ID, &operation, &payload, "2026-11-12T00:00:00Z");
        assert_eq!(statements.len(), 2);
        assert!(statements[0].sql().contains("applied_operations"));
        assert!(statements[1].sql().contains("WHERE id = ? AND user_id = ?"));
    }
}
