//! 左右別のセットとスーパーセットの結合テストが共有する操作の組み立て。

use serde_json::{json, Value};

/// 左右それぞれの値を持つ段。
pub(crate) fn side_segment(
    segment_id: &str,
    position: i64,
    left_weight_kg: f64,
    left_reps: u32,
    right_weight_kg: f64,
    right_reps: u32,
) -> Value {
    json!({
        "segment_id": segment_id,
        "position": position,
        "weight_kg": 0.0,
        "reps": 0,
        "momentum_reps": 0,
        "assisted_reps": 0,
        "duration_seconds": null,
        "left": {
            "weight_kg": left_weight_kg,
            "reps": left_reps,
            "duration_seconds": null
        },
        "right": {
            "weight_kg": right_weight_kg,
            "reps": right_reps,
            "duration_seconds": null
        }
    })
}

/// スーパーセットの組の作成の操作。
pub(crate) fn superset_create_operation(
    operation_id: &str,
    training_id: &str,
    superset_id: &str,
    member_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 1,
        "kind": "superset_create",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "superset_id": superset_id,
            "member_ids": member_ids
        }
    })
}

/// スーパーセットの組の削除の操作。
pub(crate) fn superset_delete_operation(
    operation_id: &str,
    training_id: &str,
    superset_id: &str,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "superset_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "superset_id": superset_id
        }
    })
}

/// スーパーセットの組の並べ替えの操作。
pub(crate) fn superset_reorder_operation(
    operation_id: &str,
    training_id: &str,
    superset_id: &str,
    member_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 3,
        "kind": "superset_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "superset_id": superset_id,
            "member_ids": member_ids
        }
    })
}

/// 種目の削除の操作。
pub(crate) fn exercise_delete_operation(
    operation_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 4,
        "kind": "training_exercise_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": training_exercise_id
        }
    })
}

/// 種目の並べ替えの操作。
pub(crate) fn exercise_reorder_operation(
    operation_id: &str,
    training_id: &str,
    ordered_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 5,
        "kind": "training_exercise_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "ordered_ids": ordered_ids
        }
    })
}
