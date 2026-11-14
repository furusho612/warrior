//! トレーニングの種目の操作の本文の検証。
//!
//! 対応する要求: `REQ-REC-002`。
//! `warrior_core::sync` の操作の検証から呼ぶ。追加、削除、並べ替えの本文を、操作の
//! トレーニングの ID と種目の ID、スナップショット、並び順について確かめる。

use crate::ids::is_valid_uuid_v4;
use crate::sync::{
    Operation, SyncError, TrainingExerciseCreatePayload, TrainingExerciseDeletePayload,
    TrainingExerciseReorderPayload, ValidatedPayload,
};

/// 種目の追加の本文を検証する。
pub(crate) fn validate_create(
    operation: &Operation,
    payload: TrainingExerciseCreatePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.training_exercise_id) || !is_valid_uuid_v4(&payload.exercise_id) {
        return Err(SyncError::InvalidExerciseId);
    }
    if payload.snapshot.exercise_id != payload.exercise_id || payload.snapshot.validate().is_err() {
        return Err(SyncError::InvalidSnapshot);
    }
    if payload.position < 1 {
        return Err(SyncError::InvalidPosition);
    }
    Ok(ValidatedPayload::ExerciseCreate(payload))
}

/// 種目の削除の本文を検証する。
pub(crate) fn validate_delete(
    operation: &Operation,
    payload: TrainingExerciseDeletePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.training_exercise_id) {
        return Err(SyncError::InvalidExerciseId);
    }
    Ok(ValidatedPayload::ExerciseDelete(payload))
}

/// 種目の並べ替えの本文を検証する。
pub(crate) fn validate_reorder(
    operation: &Operation,
    payload: TrainingExerciseReorderPayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if payload.ordered_ids.is_empty() {
        return Err(SyncError::InvalidOrderedIds);
    }
    let mut seen = Vec::new();
    for id in &payload.ordered_ids {
        if !is_valid_uuid_v4(id) || seen.contains(id) {
            return Err(SyncError::InvalidOrderedIds);
        }
        seen.push(id.clone());
    }
    Ok(ValidatedPayload::ExerciseReorder(payload))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{validate_create, validate_delete, validate_reorder};
    use crate::exercise::ExerciseSnapshot;
    use crate::sync::{Operation, OperationKind, SyncError, ValidatedPayload};

    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
    const TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";
    const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3304";
    const OTHER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3305";

    fn operation(kind: OperationKind, payload: Value) -> Operation {
        Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 2,
            kind,
            parent_operation_id: None,
            payload,
        }
    }

    fn snapshot() -> Value {
        json!({
            "exercise_id": EXERCISE_ID,
            "name": "ベンチプレス",
            "body_part": "chest",
            "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
            "record_mode": "weight_and_reps",
            "laterality": "bilateral",
            "grip_mode": "none",
            "grip_options": []
        })
    }

    fn create_operation() -> Operation {
        operation(
            OperationKind::TrainingExerciseCreate,
            json!({
                "training_id": TRAINING_ID,
                "training_exercise_id": TRAINING_EXERCISE_ID,
                "exercise_id": EXERCISE_ID,
                "snapshot": snapshot(),
                "position": 1
            }),
        )
    }

    fn create_payload(operation: &Operation) -> crate::sync::TrainingExerciseCreatePayload {
        serde_json::from_value(operation.payload.clone()).expect("a create payload")
    }

    fn delete_operation() -> Operation {
        operation(
            OperationKind::TrainingExerciseDelete,
            json!({
                "training_id": TRAINING_ID,
                "training_exercise_id": TRAINING_EXERCISE_ID
            }),
        )
    }

    fn delete_payload(operation: &Operation) -> crate::sync::TrainingExerciseDeletePayload {
        serde_json::from_value(operation.payload.clone()).expect("a delete payload")
    }

    fn reorder_operation(ordered_ids: Vec<&str>) -> Operation {
        operation(
            OperationKind::TrainingExerciseReorder,
            json!({
                "training_id": TRAINING_ID,
                "ordered_ids": ordered_ids
            }),
        )
    }

    fn reorder_payload(operation: &Operation) -> crate::sync::TrainingExerciseReorderPayload {
        serde_json::from_value(operation.payload.clone()).expect("a reorder payload")
    }

    #[test]
    fn a_valid_exercise_create_passes_and_keeps_the_snapshot() {
        let operation = create_operation();
        let validated = validate_create(&operation, create_payload(&operation))
            .expect("a valid exercise create");
        let ValidatedPayload::ExerciseCreate(payload) = validated else {
            panic!("the payload must be an exercise create");
        };
        assert_eq!(payload.training_exercise_id, TRAINING_EXERCISE_ID);
        assert_eq!(payload.position, 1);
        assert_eq!(
            payload.snapshot,
            serde_json::from_value::<ExerciseSnapshot>(snapshot()).expect("a snapshot")
        );
    }

    #[test]
    fn a_snapshot_must_name_the_same_exercise() {
        let mut operation = create_operation();
        operation.payload["snapshot"]["exercise_id"] = json!(OTHER_ID);
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::InvalidSnapshot)
        );
    }

    #[test]
    fn a_snapshot_must_be_valid() {
        let mut operation = create_operation();
        operation.payload["snapshot"]["name"] = json!(" ");
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::InvalidSnapshot)
        );
    }

    #[test]
    fn a_position_below_one_is_rejected() {
        let mut operation = create_operation();
        operation.payload["position"] = json!(0);
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::InvalidPosition)
        );
    }

    #[test]
    fn an_exercise_id_must_be_a_uuid_v4() {
        let mut operation = create_operation();
        operation.payload["exercise_id"] = json!("bench-press");
        operation.payload["snapshot"]["exercise_id"] = json!("bench-press");
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::InvalidExerciseId)
        );
    }

    #[test]
    fn a_valid_delete_passes() {
        let operation = delete_operation();
        let validated =
            validate_delete(&operation, delete_payload(&operation)).expect("a valid delete");
        let ValidatedPayload::ExerciseDelete(payload) = validated else {
            panic!("the payload must be an exercise delete");
        };
        assert_eq!(payload.training_exercise_id, TRAINING_EXERCISE_ID);
    }

    #[test]
    fn a_delete_with_a_different_training_is_rejected() {
        let mut operation = delete_operation();
        operation.payload["training_id"] = json!(OTHER_ID);
        assert_eq!(
            validate_delete(&operation, delete_payload(&operation)),
            Err(SyncError::PayloadIdMismatch)
        );
    }

    #[test]
    fn a_valid_reorder_passes_in_the_given_order() {
        let operation = reorder_operation(vec![TRAINING_EXERCISE_ID, OTHER_ID]);
        let validated =
            validate_reorder(&operation, reorder_payload(&operation)).expect("a valid reorder");
        let ValidatedPayload::ExerciseReorder(payload) = validated else {
            panic!("the payload must be an exercise reorder");
        };
        assert_eq!(payload.ordered_ids, vec![TRAINING_EXERCISE_ID, OTHER_ID]);
    }

    #[test]
    fn an_empty_or_duplicated_reorder_is_rejected() {
        let empty = reorder_operation(Vec::new());
        assert_eq!(
            validate_reorder(&empty, reorder_payload(&empty)),
            Err(SyncError::InvalidOrderedIds)
        );

        let duplicated = reorder_operation(vec![TRAINING_EXERCISE_ID, TRAINING_EXERCISE_ID]);
        assert_eq!(
            validate_reorder(&duplicated, reorder_payload(&duplicated)),
            Err(SyncError::InvalidOrderedIds)
        );
    }

    #[test]
    fn a_reorder_with_a_non_uuid_id_is_rejected() {
        let operation = reorder_operation(vec!["bench-press"]);
        assert_eq!(
            validate_reorder(&operation, reorder_payload(&operation)),
            Err(SyncError::InvalidOrderedIds)
        );
    }
}
