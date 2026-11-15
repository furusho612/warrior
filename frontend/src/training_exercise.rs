//! 記録する種目の行と、送信待ちの列に積む操作の組み立て。
//!
//! 種目の追加のスナップショットは端末で作り、サーバーが検証して保存する (ADR-006)。
//! 操作の ID と順序は保存のときに `frontend/src/storage` が決める。

use warrior_core::exercise::{Exercise, ExerciseSnapshot};
use warrior_core::sync::{
    Operation, OperationKind, TrainingExerciseCreatePayload, TrainingExerciseDeletePayload,
    TrainingExerciseMemoPayload, TrainingExerciseReorderPayload,
};
use warrior_core::training_record::TrainingExerciseRecord;

use crate::ids::{new_uuid, now_iso};
use crate::storage::StorageError;

/// 種目の行と、追加の操作を作る。
pub fn create(
    training_id: &str,
    exercise: &Exercise,
    position: i64,
) -> Result<(TrainingExerciseRecord, Operation), StorageError> {
    let now = now_iso();
    let training_exercise_id = new_uuid()?;
    let (record, snapshot) = build_record(
        RecordFields {
            training_exercise_id: training_exercise_id.clone(),
            training_id: training_id.to_owned(),
            memo: String::new(),
            confirmed_memo: String::new(),
            position,
            created_at: now.clone(),
            updated_at: now,
        },
        exercise,
    )?;
    let operation = create_operation(
        training_id,
        training_exercise_id,
        &exercise.id,
        snapshot,
        position,
    )?;
    Ok((record, operation))
}

/// 記録中の種目を、選んだ種目へ入れ替える行と操作を作る。
///
/// 行の ID と位置、メモは保つ。セットは行の ID に結び付いているため、そのまま残る。
/// 記録済みのセットがある場合の確認は呼び出し元の画面が担う。
pub fn replace(
    record: &TrainingExerciseRecord,
    exercise: &Exercise,
) -> Result<(TrainingExerciseRecord, Operation), StorageError> {
    let now = now_iso();
    let (updated, snapshot) = build_record(
        RecordFields {
            training_exercise_id: record.id.clone(),
            training_id: record.training_id.clone(),
            memo: record.memo.clone(),
            confirmed_memo: record.confirmed_memo.clone(),
            position: record.position,
            created_at: record.created_at.clone(),
            updated_at: now,
        },
        exercise,
    )?;
    let operation = create_operation(
        &record.training_id,
        record.id.clone(),
        &exercise.id,
        snapshot,
        record.position,
    )?;
    Ok((updated, operation))
}

/// 種目の削除の操作を作る。
pub fn delete(training_id: &str, training_exercise_id: &str) -> Result<Operation, StorageError> {
    let payload = TrainingExerciseDeletePayload {
        training_id: training_id.to_owned(),
        training_exercise_id: training_exercise_id.to_owned(),
    };
    operation(
        training_id,
        OperationKind::TrainingExerciseDelete,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// 種目の並べ替えの操作を作る。
pub fn reorder(training_id: &str, ordered_ids: &[String]) -> Result<Operation, StorageError> {
    let payload = TrainingExerciseReorderPayload {
        training_id: training_id.to_owned(),
        ordered_ids: ordered_ids.to_vec(),
    };
    operation(
        training_id,
        OperationKind::TrainingExerciseReorder,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// 種目のメモの保存の操作を作る。
pub fn memo(
    training_id: &str,
    training_exercise_id: &str,
    memo: &str,
) -> Result<Operation, StorageError> {
    warrior_core::memo::validate(memo)
        .map_err(|_| StorageError::InvalidOperation("memo_too_long".to_owned()))?;
    let payload = TrainingExerciseMemoPayload {
        training_id: training_id.to_owned(),
        training_exercise_id: training_exercise_id.to_owned(),
        memo: memo.to_owned(),
    };
    operation(
        training_id,
        OperationKind::TrainingExerciseMemo,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

fn operation(
    training_id: &str,
    kind: OperationKind,
    payload: serde_json::Value,
) -> Result<Operation, StorageError> {
    Ok(Operation {
        operation_id: new_uuid()?,
        training_id: training_id.to_owned(),
        sequence: 0,
        kind,
        parent_operation_id: None,
        payload,
    })
}

struct RecordFields {
    training_exercise_id: String,
    training_id: String,
    memo: String,
    confirmed_memo: String,
    position: i64,
    created_at: String,
    updated_at: String,
}

fn build_record(
    fields: RecordFields,
    exercise: &Exercise,
) -> Result<(TrainingExerciseRecord, ExerciseSnapshot), StorageError> {
    let snapshot = ExerciseSnapshot::from(exercise);
    let record = TrainingExerciseRecord {
        id: fields.training_exercise_id,
        training_id: fields.training_id,
        exercise_id: exercise.id.clone(),
        snapshot: serde_json::to_string(&snapshot)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
        memo: fields.memo,
        confirmed_memo: fields.confirmed_memo,
        position: fields.position,
        created_at: fields.created_at,
        updated_at: fields.updated_at,
    };
    Ok((record, snapshot))
}

fn create_operation(
    training_id: &str,
    training_exercise_id: String,
    exercise_id: &str,
    snapshot: ExerciseSnapshot,
    position: i64,
) -> Result<Operation, StorageError> {
    let payload = TrainingExerciseCreatePayload {
        training_id: training_id.to_owned(),
        training_exercise_id,
        exercise_id: exercise_id.to_owned(),
        snapshot,
        position,
    };
    operation(
        training_id,
        OperationKind::TrainingExerciseCreate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}
