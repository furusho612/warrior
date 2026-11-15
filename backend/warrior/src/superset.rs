//! スーパーセットの組の作成、削除、並べ替えの操作の適用。
//!
//! 対応する要求: `REQ-SET-005`。
//! 操作は端末の送信待ちの列から届き、`backend/warrior/src/sync.rs` が編集権を検査する。
//! このモジュールは、同じ編集権の `EXISTS` 条件を各データ文に付け、組の適用と操作 ID の
//! 保存を同じ `batch()` に入れる (ADR-002、ADR-003)。組の種目は同じトレーニングの
//! 種目だけを許し、2 つ未満の組と、既にほかの組に属する種目を含む組を拒否する。
//! 並べ替えは組の種目の順序だけを変え、種目の追加、削除、移動を拒否する。
//! 削除は、種目の削除で解散した組の後始末のため、既に無い組でも成功として扱う。

use serde::Deserialize;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    SupersetCreatePayload, SupersetDeletePayload, SupersetReorderPayload, ValidatedOperation,
    ValidatedPayload,
};
use worker::{Response, Result};

use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::sync::{
    applied_statement, owner_condition, ownership_guard_statement, session_row, write_decision,
    BatchOutcome, OwnerCondition,
};
use crate::training_session_exercise::{ownership_condition, ExerciseOwnership};

/// 件数を読む行。
#[derive(Debug, Deserialize)]
struct CountRow {
    /// 件数。
    count: i64,
}

/// 組の種目の行。
#[derive(Debug, Deserialize)]
struct SupersetMemberRow {
    /// 種目の行の ID。
    training_exercise_id: String,
}

/// スーパーセットの操作の batch を組み立てる。
///
/// 先頭の文で編集権を確認してトレーニングの更新時刻を進め、その変更の後に操作 ID を
/// 保存する。組のデータ文は操作 ID を保存した後に置く。組が既に無い削除でも操作 ID を
/// 残し、種目の削除に続く後始末の再送を止める。
pub(crate) async fn superset_batch(
    db: &Db,
    user_id: &str,
    terminal_id: Option<&str>,
    validated: &ValidatedOperation,
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
    if let Some(rejection) = precheck(db, user_id, validated).await? {
        return Ok(BatchOutcome::Reject(rejection));
    }
    let ownership = match owner {
        OwnerCondition::Claim(terminal_id) | OwnerCondition::Owned(terminal_id) => {
            ExerciseOwnership::Terminal(terminal_id)
        }
        OwnerCondition::Finished => ExerciseOwnership::Finished,
    };
    let Some(data) = superset_statements(user_id, validated, ownership, now) else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    let mut statements = vec![
        ownership_guard_statement(user_id, owner, &validated.operation.training_id, now),
        applied_statement(user_id, &validated.operation, now),
    ];
    statements.extend(data);
    Ok(BatchOutcome::Apply(statements))
}

/// 適用の前に、組の種目が同じトレーニングに属することを確かめる。
async fn precheck(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
) -> Result<Option<Response>> {
    let training_id = &validated.operation.training_id;
    let member_ids = match &validated.payload {
        ValidatedPayload::SupersetCreate(payload) => &payload.member_ids,
        ValidatedPayload::SupersetReorder(payload) => &payload.member_ids,
        ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SetCreate(_)
        | ValidatedPayload::SetUpdate(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::SetComplete(_) => return Ok(None),
    };
    if !members_in_training(db, user_id, training_id, member_ids).await? {
        return Ok(Some(response(
            ErrorCode::BadRequest,
            "a superset member is not in the training",
        )));
    }
    if let ValidatedPayload::SupersetReorder(payload) = &validated.payload {
        let current = current_member_ids(db, user_id, training_id, &payload.superset_id).await?;
        if current.is_empty() {
            return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
        }
        if !same_member_ids(&current, member_ids) {
            return Ok(Some(response(
                ErrorCode::BadRequest,
                "a reorder cannot change the superset members",
            )));
        }
    }
    if let ValidatedPayload::SupersetCreate(payload) = &validated.payload {
        if !members_without_group(db, user_id, training_id, &payload.superset_id, member_ids)
            .await?
        {
            return Ok(Some(response(
                ErrorCode::Conflict,
                "a superset member already belongs to a group",
            )));
        }
    }
    Ok(None)
}

async fn members_in_training(
    db: &Db,
    user_id: &str,
    training_id: &str,
    member_ids: &[String],
) -> Result<bool> {
    if member_ids.is_empty() {
        return Ok(false);
    }
    let placeholders = vec!["?"; member_ids.len()].join(", ");
    let mut bindings = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    bindings.extend(member_ids.iter().map(|id| SqlValue::Text(id.clone())));
    let count = db
        .first::<CountRow>(&SqlStatement::new(
            format!(
                "SELECT COUNT(*) AS count FROM training_exercises \
                 WHERE user_id = ? AND training_id = ? AND id IN ({placeholders})"
            ),
            bindings,
        ))
        .await?
        .map_or(0, |row| row.count);
    Ok(count == i64::try_from(member_ids.len()).unwrap_or(i64::MAX))
}

/// 組の種目が、同じトレーニングのほかの組に属していないか。
///
/// 同じ操作の再送は、作った組自身を除くことで通す。
async fn members_without_group(
    db: &Db,
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    member_ids: &[String],
) -> Result<bool> {
    if member_ids.is_empty() {
        return Ok(true);
    }
    let placeholders = vec!["?"; member_ids.len()].join(", ");
    let mut bindings = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(superset_id.to_owned()),
    ];
    bindings.extend(member_ids.iter().map(|id| SqlValue::Text(id.clone())));
    let count = db
        .first::<CountRow>(&SqlStatement::new(
            format!(
                "SELECT COUNT(*) AS count FROM superset_members sm \
                 JOIN supersets ss ON ss.id = sm.superset_id \
                 WHERE sm.user_id = ? AND ss.training_id = ? AND ss.id != ? \
                 AND sm.training_exercise_id IN ({placeholders})"
            ),
            bindings,
        ))
        .await?
        .map_or(0, |row| row.count);
    Ok(count == 0)
}

/// 組が持つ種目の行の ID を、組の中の順序で返す。
async fn current_member_ids(
    db: &Db,
    user_id: &str,
    training_id: &str,
    superset_id: &str,
) -> Result<Vec<String>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT sm.training_exercise_id FROM superset_members sm \
             JOIN supersets ss ON ss.id = sm.superset_id \
             WHERE sm.user_id = ? AND ss.training_id = ? AND ss.id = ? \
             ORDER BY sm.position, sm.training_exercise_id",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(superset_id.to_owned()),
            ],
        ))
        .await?;
    Ok(result
        .results::<SupersetMemberRow>()?
        .into_iter()
        .map(|row| row.training_exercise_id)
        .collect())
}

/// 2 つの種目の並びが同じ集合か。
fn same_member_ids(left: &[String], right: &[String]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_unstable();
    right.sort_unstable();
    left == right
}

fn superset_statements(
    user_id: &str,
    validated: &ValidatedOperation,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Option<Vec<SqlStatement>> {
    let training_id = &validated.operation.training_id;
    match &validated.payload {
        ValidatedPayload::SupersetCreate(payload) => Some(create_statements(
            user_id,
            training_id,
            payload,
            ownership,
            now,
        )),
        ValidatedPayload::SupersetDelete(payload) => {
            Some(delete_statements(user_id, training_id, payload, ownership))
        }
        ValidatedPayload::SupersetReorder(payload) => Some(reorder_statements(
            user_id,
            training_id,
            payload,
            ownership,
            now,
        )),
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SetCreate(_)
        | ValidatedPayload::SetUpdate(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::SetComplete(_) => None,
    }
}

fn create_statements(
    user_id: &str,
    training_id: &str,
    payload: &SupersetCreatePayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Vec<SqlStatement> {
    let mut statements = vec![
        superset_insert_statement(user_id, training_id, &payload.superset_id, ownership, now),
        member_delete_statement(user_id, training_id, &payload.superset_id, ownership),
    ];
    statements.extend(member_insert_statements(
        user_id,
        training_id,
        &payload.superset_id,
        &payload.member_ids,
        ownership,
    ));
    statements
}

fn delete_statements(
    user_id: &str,
    training_id: &str,
    payload: &SupersetDeletePayload,
    ownership: ExerciseOwnership<'_>,
) -> Vec<SqlStatement> {
    vec![
        member_delete_statement(user_id, training_id, &payload.superset_id, ownership),
        superset_delete_statement(user_id, training_id, &payload.superset_id, ownership),
    ]
}

fn reorder_statements(
    user_id: &str,
    training_id: &str,
    payload: &SupersetReorderPayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Vec<SqlStatement> {
    let mut statements = vec![
        superset_touch_statement(user_id, training_id, &payload.superset_id, ownership, now),
        member_delete_statement(user_id, training_id, &payload.superset_id, ownership),
    ];
    statements.extend(member_insert_statements(
        user_id,
        training_id,
        &payload.superset_id,
        &payload.member_ids,
        ownership,
    ));
    statements
}

fn superset_insert_statement(
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(now.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "INSERT INTO supersets (id, user_id, training_id, position, created_at, updated_at) \
             SELECT ?, ?, ?, \
              COALESCE((SELECT MAX(position) FROM supersets WHERE user_id = ? AND training_id = ?), 0) + 1, \
              ?, ? \
             WHERE {condition} \
             ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at \
             WHERE supersets.user_id = excluded.user_id \
             AND supersets.training_id = excluded.training_id"
        ),
        bindings,
    )
}

fn superset_touch_statement(
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "UPDATE supersets SET updated_at = ? \
             WHERE id = ? AND user_id = ? AND training_id = ? AND {condition}"
        ),
        bindings,
    )
}

fn member_delete_statement(
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    ownership: ExerciseOwnership<'_>,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "DELETE FROM superset_members WHERE user_id = ? AND superset_id = ? \
             AND EXISTS (SELECT 1 FROM supersets ss \
              WHERE ss.id = ? AND ss.user_id = ? AND ss.training_id = ?) \
             AND {condition}"
        ),
        bindings,
    )
}

fn superset_delete_statement(
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    ownership: ExerciseOwnership<'_>,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "DELETE FROM supersets WHERE id = ? AND user_id = ? AND training_id = ? \
             AND {condition}"
        ),
        bindings,
    )
}

fn member_insert_statements(
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    member_ids: &[String],
    ownership: ExerciseOwnership<'_>,
) -> Vec<SqlStatement> {
    member_ids
        .iter()
        .enumerate()
        .map(|(index, member_id)| {
            member_insert_statement(
                user_id,
                training_id,
                superset_id,
                member_id,
                i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
                ownership,
            )
        })
        .collect()
}

fn member_insert_statement(
    user_id: &str,
    training_id: &str,
    superset_id: &str,
    member_id: &str,
    position: i64,
    ownership: ExerciseOwnership<'_>,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(member_id.to_owned()),
        SqlValue::Integer(position),
        SqlValue::Text(superset_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(member_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "INSERT INTO superset_members (user_id, superset_id, training_exercise_id, position) \
             SELECT ?, ?, ?, ? \
             WHERE EXISTS (SELECT 1 FROM supersets ss \
              WHERE ss.id = ? AND ss.user_id = ? AND ss.training_id = ?) \
             AND EXISTS (SELECT 1 FROM training_exercises te \
              WHERE te.id = ? AND te.user_id = ? AND te.training_id = ?) \
             AND {condition} \
             ON CONFLICT(superset_id, training_exercise_id) \
             DO UPDATE SET position = excluded.position"
        ),
        bindings,
    )
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-SET-005`。

    use serde_json::json;
    use warrior_core::sql::SqlValue;
    use warrior_core::sync::{
        Operation, OperationKind, SupersetCreatePayload, SupersetDeletePayload,
        SupersetReorderPayload, ValidatedOperation, ValidatedPayload,
    };

    use super::{create_statements, delete_statements, reorder_statements};
    use crate::training_session_exercise::ExerciseOwnership;

    const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5901";
    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5902";
    const SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5903";
    const FIRST_MEMBER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5904";
    const SECOND_MEMBER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5905";
    const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5906";
    const NOW: &str = "2026-11-12T00:00:00Z";

    fn create_payload() -> SupersetCreatePayload {
        SupersetCreatePayload {
            training_id: TRAINING_ID.to_owned(),
            superset_id: SUPERSET_ID.to_owned(),
            member_ids: vec![FIRST_MEMBER_ID.to_owned(), SECOND_MEMBER_ID.to_owned()],
        }
    }

    #[test]
    fn the_create_statements_scope_the_group_and_every_member() {
        let statements = create_statements(
            USER_ID,
            TRAINING_ID,
            &create_payload(),
            ExerciseOwnership::Terminal(TERMINAL_ID),
            NOW,
        );
        assert_eq!(statements.len(), 4);
        assert!(statements[0].sql().contains("INSERT INTO supersets"));
        assert!(statements[0].sql().contains("ON CONFLICT(id) DO UPDATE"));
        assert!(statements[0]
            .sql()
            .contains("supersets.user_id = excluded.user_id"));
        assert!(statements[0]
            .sql()
            .contains("EXISTS (SELECT 1 FROM training_sessions ts"));
        assert_eq!(
            statements[0].bindings()[0],
            SqlValue::Text(SUPERSET_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[1],
            SqlValue::Text(USER_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[2],
            SqlValue::Text(TRAINING_ID.to_owned())
        );
        assert!(statements[1].sql().contains("DELETE FROM superset_members"));
        assert!(statements[2].sql().contains("INSERT INTO superset_members"));
        assert!(statements[3].sql().contains("INSERT INTO superset_members"));
        assert_eq!(statements[2].bindings()[3], SqlValue::Integer(1));
        assert_eq!(statements[3].bindings()[3], SqlValue::Integer(2));
        assert_eq!(
            statements[3].bindings()[2],
            SqlValue::Text(SECOND_MEMBER_ID.to_owned())
        );
    }

    #[test]
    fn the_delete_statements_remove_the_members_before_the_group() {
        let statements = delete_statements(
            USER_ID,
            TRAINING_ID,
            &SupersetDeletePayload {
                training_id: TRAINING_ID.to_owned(),
                superset_id: SUPERSET_ID.to_owned(),
            },
            ExerciseOwnership::Finished,
        );
        assert_eq!(statements.len(), 2);
        assert!(statements[0].sql().contains("DELETE FROM superset_members"));
        assert!(statements[0].sql().contains("ss.training_id = ?"));
        assert!(statements[1].sql().contains("DELETE FROM supersets"));
        assert!(statements[1].sql().contains("finished_at IS NOT NULL"));
        assert_eq!(
            statements[1].bindings()[0],
            SqlValue::Text(SUPERSET_ID.to_owned())
        );
    }

    #[test]
    fn the_reorder_statements_number_the_members_from_one() {
        let statements = reorder_statements(
            USER_ID,
            TRAINING_ID,
            &SupersetReorderPayload {
                training_id: TRAINING_ID.to_owned(),
                superset_id: SUPERSET_ID.to_owned(),
                member_ids: vec![SECOND_MEMBER_ID.to_owned(), FIRST_MEMBER_ID.to_owned()],
            },
            ExerciseOwnership::Terminal(TERMINAL_ID),
            NOW,
        );
        assert_eq!(statements.len(), 4);
        assert!(statements[0]
            .sql()
            .contains("UPDATE supersets SET updated_at"));
        assert!(statements[1].sql().contains("DELETE FROM superset_members"));
        assert_eq!(statements[2].bindings()[3], SqlValue::Integer(1));
        assert_eq!(statements[3].bindings()[3], SqlValue::Integer(2));
        assert_eq!(
            statements[2].bindings()[2],
            SqlValue::Text(SECOND_MEMBER_ID.to_owned())
        );
    }

    #[test]
    fn every_statement_keeps_the_editing_right_condition() {
        let validated = ValidatedOperation {
            operation: Operation {
                operation_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5907".to_owned(),
                training_id: TRAINING_ID.to_owned(),
                sequence: 1,
                kind: OperationKind::SupersetCreate,
                parent_operation_id: None,
                payload: json!({}),
            },
            payload: ValidatedPayload::SupersetCreate(create_payload()),
        };
        let Some(data) = super::superset_statements(
            USER_ID,
            &validated,
            ExerciseOwnership::Terminal(TERMINAL_ID),
            NOW,
        ) else {
            panic!("the payload must produce superset statements");
        };
        for statement in data {
            assert!(
                statement
                    .sql()
                    .contains("EXISTS (SELECT 1 FROM training_sessions ts"),
                "the statement must carry the editing right: {}",
                statement.sql()
            );
        }
    }
}
