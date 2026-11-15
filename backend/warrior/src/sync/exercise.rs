//! 種目の追加、削除、並べ替えの batch、事前検査、データ文の組み立て。
//!
//! 編集権の判定は `sync.rs` の所有者の条件を使い、種目の各データ文にも同じ条件を付ける。
//! 前回の記録の取得は `training_session_exercise` が担う。

use super::*;
use crate::training_session_exercise::ExerciseOwnership;

#[derive(Debug, Deserialize)]
struct ExerciseRow {
    id: String,
}

/// 種目の追加、削除、並べ替えの batch を組み立てる。
///
/// 先頭の文で編集権を確認してトレーニングの更新時刻を進め、その変更の後に操作 ID を
/// 保存する。並べ替えが同じ順序でも、また削除が 0 行でも、操作 ID を残して再送を止める。
pub(super) async fn exercise_batch(
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
    match &validated.payload {
        ValidatedPayload::ExerciseDelete(payload) => {
            if !exercise_in_training(
                db,
                user_id,
                &validated.operation.training_id,
                &payload.training_exercise_id,
            )
            .await?
            {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::NotFound,
                    "resource not found",
                )));
            }
            if crate::training_session_exercise::has_completed_sets(
                db,
                user_id,
                &payload.training_exercise_id,
            )
            .await?
                && !finished
            {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::Conflict,
                    "the exercise has completed sets",
                )));
            }
        }
        ValidatedPayload::ExerciseReorder(payload) => {
            let current =
                training_exercise_ids(db, user_id, &validated.operation.training_id).await?;
            if !same_exercise_ids(&current, &payload.ordered_ids) {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::Conflict,
                    "the exercise order is stale",
                )));
            }
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::SessionMemo(_)
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseMemo(_)
        | ValidatedPayload::SetCreate(_)
        | ValidatedPayload::SetUpdate(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::SetComplete(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_)
        | ValidatedPayload::RoutineCreate(_)
        | ValidatedPayload::RoutineUpdate(_)
        | ValidatedPayload::RoutineDelete(_)
        | ValidatedPayload::RoutineCopy(_) => {}
    }
    let ownership = match owner {
        OwnerCondition::Claim(terminal_id) | OwnerCondition::Owned(terminal_id) => {
            ExerciseOwnership::Terminal(terminal_id)
        }
        OwnerCondition::Finished => ExerciseOwnership::Finished,
    };
    let Some(data) = exercise_statements(user_id, validated, ownership, now) else {
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

/// 種目の操作の種別に対応するデータの文を返す。
fn exercise_statements(
    user_id: &str,
    validated: &warrior_core::sync::ValidatedOperation,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Option<Vec<SqlStatement>> {
    let operation = &validated.operation;
    match &validated.payload {
        ValidatedPayload::ExerciseCreate(payload) => Some(
            crate::training_session_exercise::create_statements(
                user_id,
                &operation.training_id,
                payload,
                ownership,
                now,
            )
            .ok()?,
        ),
        ValidatedPayload::ExerciseDelete(payload) => {
            Some(crate::training_session_exercise::delete_statements(
                user_id,
                &operation.training_id,
                payload,
                ownership,
            ))
        }
        ValidatedPayload::ExerciseReorder(payload) => {
            Some(crate::training_session_exercise::reorder_statements(
                user_id,
                &operation.training_id,
                payload,
                ownership,
                now,
            ))
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::SessionMemo(_)
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
        | ValidatedPayload::ExerciseMemo(_)
        | ValidatedPayload::SetCreate(_)
        | ValidatedPayload::SetUpdate(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::SetComplete(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_)
        | ValidatedPayload::RoutineCreate(_)
        | ValidatedPayload::RoutineUpdate(_)
        | ValidatedPayload::RoutineDelete(_)
        | ValidatedPayload::RoutineCopy(_) => None,
    }
}

/// 種目のメモの保存の batch。
///
/// 先頭の文で編集権を確認してトレーニングの更新時刻を進め、メモの文の後に操作 ID を
/// 保存する。種目がトレーニングに無いときは適用しない。
pub(super) async fn exercise_memo_batch(
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
    let ValidatedPayload::ExerciseMemo(payload) = &validated.payload else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    if !exercise_in_training(
        db,
        user_id,
        &validated.operation.training_id,
        &payload.training_exercise_id,
    )
    .await?
    {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    }
    let ownership = match owner {
        OwnerCondition::Claim(terminal_id) | OwnerCondition::Owned(terminal_id) => {
            ExerciseOwnership::Terminal(terminal_id)
        }
        OwnerCondition::Finished => ExerciseOwnership::Finished,
    };
    Ok(BatchOutcome::Apply(vec![
        ownership_guard_statement(user_id, owner, &validated.operation.training_id, now),
        crate::training_session_exercise::memo_statement(
            user_id,
            &validated.operation.training_id,
            payload,
            ownership,
            now,
        ),
        applied_statement(user_id, &validated.operation, now),
    ]))
}

/// 終了済みのトレーニングに、種目のメモを終了後の書き込みとして適用する。
pub(super) async fn reapply_finished_exercise_memo(
    db: &Db,
    user_id: &str,
    operation: &Operation,
    payload: &warrior_core::sync::TrainingExerciseMemoPayload,
    now: &str,
) -> Result<Response> {
    let statements = vec![
        crate::training_session_exercise::memo_statement(
            user_id,
            &operation.training_id,
            payload,
            ExerciseOwnership::Finished,
            now,
        ),
        applied_statement(user_id, operation, now),
    ];
    let results = match db.batch(&statements).await {
        Ok(results) => results,
        Err(error) => {
            console_error!("failed to reapply the exercise memo: {error}");
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
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    status_response("applied")
}

async fn exercise_in_training(
    db: &Db,
    user_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Result<bool> {
    Ok(db
        .first::<ExerciseRow>(&SqlStatement::new(
            "SELECT id FROM training_exercises \
             WHERE id = ? AND user_id = ? AND training_id = ?",
            vec![
                SqlValue::Text(training_exercise_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?
        .is_some())
}

async fn training_exercise_ids(db: &Db, user_id: &str, training_id: &str) -> Result<Vec<String>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT id FROM training_exercises WHERE user_id = ? AND training_id = ? \
             ORDER BY position, id",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    Ok(result
        .results::<ExerciseRow>()?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

fn same_exercise_ids(current: &[String], ordered: &[String]) -> bool {
    let mut current = current.to_vec();
    let mut ordered = ordered.to_vec();
    current.sort_unstable();
    ordered.sort_unstable();
    current == ordered
}

/// 編集権を確認し、トレーニングの更新時刻を進める文。
pub(crate) fn ownership_guard_statement(
    user_id: &str,
    owner: OwnerCondition<'_>,
    training_id: &str,
    now: &str,
) -> SqlStatement {
    let user = SqlValue::Text(user_id.to_owned());
    let training = SqlValue::Text(training_id.to_owned());
    let updated_at = SqlValue::Text(now.to_owned());
    match owner {
        OwnerCondition::Claim(terminal_id) => SqlStatement::new(
            "UPDATE training_sessions SET owner_terminal_id = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? AND finished_at IS NULL AND owner_terminal_id IS NULL",
            vec![
                SqlValue::Text(terminal_id.to_owned()),
                updated_at,
                training,
                user,
            ],
        ),
        OwnerCondition::Owned(terminal_id) => SqlStatement::new(
            "UPDATE training_sessions SET updated_at = ? \
             WHERE id = ? AND user_id = ? \
             AND (finished_at IS NOT NULL OR owner_terminal_id = ?)",
            vec![
                updated_at,
                training,
                user,
                SqlValue::Text(terminal_id.to_owned()),
            ],
        ),
        OwnerCondition::Finished => SqlStatement::new(
            "UPDATE training_sessions SET updated_at = ? \
             WHERE id = ? AND user_id = ? AND finished_at IS NOT NULL",
            vec![updated_at, training, user],
        ),
    }
}
