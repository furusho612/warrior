//! 記録する種目の行と、送信待ちの列に積む操作の組み立て。
//!
//! 種目の追加のスナップショットは端末で作り、サーバーが検証して保存する (ADR-006)。
//! 操作の ID と順序は保存のときに `frontend/src/storage` が決める。

use warrior_core::exercise::{Exercise, ExerciseSnapshot};
use warrior_core::sync::{
    Operation, OperationKind, TrainingExerciseCreatePayload, TrainingExerciseDeletePayload,
    TrainingExerciseReorderPayload,
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
    let snapshot = ExerciseSnapshot::from(exercise);
    let record = TrainingExerciseRecord {
        id: training_exercise_id.clone(),
        training_id: training_id.to_owned(),
        exercise_id: exercise.id.clone(),
        snapshot: serde_json::to_string(&snapshot)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
        position,
        created_at: now.clone(),
        updated_at: now,
    };
    let payload = TrainingExerciseCreatePayload {
        training_id: training_id.to_owned(),
        training_exercise_id,
        exercise_id: exercise.id.clone(),
        snapshot,
        position,
    };
    let operation = Operation {
        operation_id: new_uuid()?,
        training_id: training_id.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseCreate,
        parent_operation_id: None,
        payload: serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    };
    Ok((record, operation))
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
