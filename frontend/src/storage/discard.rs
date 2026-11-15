//! 失敗した操作の破棄と、破棄した作成に依存する操作の除去。

use wasm_bindgen::JsValue;

use warrior_core::sync::{discard as discard_transition, Operation, OperationKind, OutboxEntry};
use warrior_core::training_record::TrainingExerciseRecord;

use super::exercise;
use super::set;
use super::{indexeddb, outbox, DiscardOutcome, StorageError, Store};

/// 対象の操作を破棄し、結果を種別ごとの処理へ返す。
pub(super) async fn steps(
    transaction: &indexeddb::Transaction,
    operation_id: &str,
) -> Result<DiscardOutcome, StorageError> {
    let outbox_store = transaction.store(Store::Outbox.name())?;
    let entries = outbox::read(&outbox_store).await?;
    let Some(target) = entries
        .iter()
        .find(|entry| entry.operation.operation_id == operation_id)
        .cloned()
    else {
        return Ok(DiscardOutcome::OperationsRemoved);
    };

    let mut updated = entries.clone();
    let mut removed = discard_transition(&mut updated, operation_id);
    if target.operation.kind == OperationKind::TrainingExerciseCreate {
        let training_exercise_id = exercise::payload_training_exercise_id(&target.operation)?;
        removed.extend(discard_exercise_following(
            &mut updated,
            &target.operation,
            &training_exercise_id,
        ));
    }
    if target.operation.kind == OperationKind::SetCreate {
        let set_id = set::payload_set_id(&target.operation)?;
        let training_exercise_id =
            payload_text(&target.operation, "training_exercise_id").unwrap_or_default();
        removed.extend(discard_set_following(
            &mut updated,
            &target.operation,
            &set_id,
            training_exercise_id,
        ));
    }
    for removed_id in removed {
        indexeddb::delete_entry(&outbox_store, &JsValue::from_str(&removed_id)).await?;
    }

    if target.operation.kind == OperationKind::TrainingSessionCreate {
        exercise::delete_session_tree(transaction, &target.operation.training_id).await?;
        return Ok(DiscardOutcome::SessionDeleted);
    }
    if target.operation.kind == OperationKind::TrainingExerciseCreate {
        let training_exercise_id = exercise::payload_training_exercise_id(&target.operation)?;
        exercise::delete_training_exercise_rows(transaction, &training_exercise_id).await?;
        return Ok(DiscardOutcome::ExerciseDeleted);
    }
    if target.operation.kind == OperationKind::SetCreate {
        let set_id = set::payload_set_id(&target.operation)?;
        set::delete_set_rows(transaction, &set_id).await?;
        return Ok(DiscardOutcome::SetDeleted);
    }
    if target.operation.kind == OperationKind::TrainingExerciseMemo {
        let Some(training_exercise_id) = payload_text(&target.operation, "training_exercise_id")
        else {
            return Err(StorageError::InvalidOperation(
                "the operation has no training exercise id".to_owned(),
            ));
        };
        let has_newer_memo = updated.iter().any(|entry| {
            entry.operation.kind == OperationKind::TrainingExerciseMemo
                && entry.operation.sequence > target.operation.sequence
                && payload_text(&entry.operation, "training_exercise_id")
                    == Some(training_exercise_id)
        });
        if has_newer_memo {
            return Ok(DiscardOutcome::OperationsRemoved);
        }
        return revert_exercise_memo(transaction, training_exercise_id).await;
    }
    if !matches!(
        target.operation.kind,
        OperationKind::TrainingSessionUpdate | OperationKind::TrainingSessionMemo
    ) {
        return Ok(DiscardOutcome::OperationsRemoved);
    }

    let has_newer_update = updated.iter().any(|entry| {
        entry.operation.training_id == target.operation.training_id
            && matches!(
                entry.operation.kind,
                OperationKind::TrainingSessionUpdate | OperationKind::TrainingSessionMemo
            )
            && entry.operation.sequence > target.operation.sequence
    });
    if has_newer_update {
        return Ok(DiscardOutcome::OperationsRemoved);
    }

    revert_session_memo(transaction, &target.operation.training_id).await
}

/// 対象の作成より後の、従属する操作を列から消す。
fn discard_following(
    entries: &mut Vec<OutboxEntry>,
    target: &Operation,
    is_dependent: impl Fn(&Operation) -> bool,
) -> Vec<String> {
    let mut removed = Vec::new();
    entries.retain(|entry| {
        let operation = &entry.operation;
        let dependent = operation.sequence > target.sequence && is_dependent(operation);
        if dependent {
            removed.push(operation.operation_id.clone());
        }
        !dependent
    });
    removed
}

/// セットの作成を破棄したとき、そのセットと種目に依存する後続の操作を消す。
fn discard_set_following(
    entries: &mut Vec<OutboxEntry>,
    target: &Operation,
    set_id: &str,
    training_exercise_id: &str,
) -> Vec<String> {
    discard_following(entries, target, |operation| {
        (matches!(
            operation.kind,
            OperationKind::SetUpdate | OperationKind::SetComplete | OperationKind::SetDelete
        ) && payload_text(operation, "set_id") == Some(set_id))
            || (operation.kind == OperationKind::SetReorder
                && payload_text(operation, "training_exercise_id") == Some(training_exercise_id))
    })
}

/// 種目の作成を破棄したとき、その種目と種目に属するセットの後続の操作を消す。
fn discard_exercise_following(
    entries: &mut Vec<OutboxEntry>,
    target: &Operation,
    training_exercise_id: &str,
) -> Vec<String> {
    let set_ids = entries
        .iter()
        .map(|entry| &entry.operation)
        .filter(|operation| operation.sequence > target.sequence)
        .filter(|operation| operation.kind == OperationKind::SetCreate)
        .filter(|operation| {
            payload_text(operation, "training_exercise_id") == Some(training_exercise_id)
        })
        .filter_map(|operation| payload_text(operation, "set_id").map(str::to_owned))
        .collect::<Vec<String>>();
    discard_following(entries, target, |operation| {
        let same_exercise =
            payload_text(operation, "training_exercise_id") == Some(training_exercise_id);
        let same_set = payload_text(operation, "set_id")
            .is_some_and(|set_id| set_ids.iter().any(|id| id == set_id));
        (matches!(
            operation.kind,
            OperationKind::SetCreate
                | OperationKind::SetReorder
                | OperationKind::TrainingExerciseDelete
                | OperationKind::TrainingExerciseMemo
        ) && same_exercise)
            || operation.kind == OperationKind::TrainingExerciseReorder
                && operation.training_id == target.training_id
            || (matches!(
                operation.kind,
                OperationKind::SetUpdate | OperationKind::SetDelete | OperationKind::SetComplete
            ) && same_set)
    })
}

pub(super) fn payload_text<'a>(operation: &'a Operation, key: &str) -> Option<&'a str> {
    operation.payload.get(key).and_then(|value| value.as_str())
}

/// サーバーが受理したメモへ戻す。
async fn revert_session_memo(
    transaction: &indexeddb::Transaction,
    training_id: &str,
) -> Result<DiscardOutcome, StorageError> {
    let sessions = transaction.store(Store::TrainingSessions.name())?;
    let Some(value) = indexeddb::get(&sessions, &JsValue::from_str(training_id)).await? else {
        return Ok(DiscardOutcome::OperationsRemoved);
    };
    let mut session: warrior_core::sync::SessionRecord = indexeddb::from_js(&value)?;
    session.memo = session.confirmed_memo.clone();
    indexeddb::put(&sessions, &indexeddb::to_js(&session)?).await?;
    Ok(DiscardOutcome::MemoReverted(Box::new(session)))
}

/// 種目のメモを、サーバーが受理した値へ戻す。
async fn revert_exercise_memo(
    transaction: &indexeddb::Transaction,
    training_exercise_id: &str,
) -> Result<DiscardOutcome, StorageError> {
    let exercises = transaction.store(Store::TrainingExercises.name())?;
    let Some(value) = indexeddb::get(&exercises, &JsValue::from_str(training_exercise_id)).await?
    else {
        return Ok(DiscardOutcome::OperationsRemoved);
    };
    let mut exercise: TrainingExerciseRecord = indexeddb::from_js(&value)?;
    exercise.memo = exercise.confirmed_memo.clone();
    indexeddb::put(&exercises, &indexeddb::to_js(&exercise)?).await?;
    Ok(DiscardOutcome::ExerciseMemoReverted(Box::new(exercise)))
}
