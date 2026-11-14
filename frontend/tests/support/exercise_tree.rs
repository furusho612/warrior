//! 種目の木の端末内の保存と、失敗した操作の破棄のテストが共有する準備。
//!
//! 種目の行、セットの値、サーバーの記録と操作の組み立てを置く。

use serde_json::json;
use warrior_core::set_calc::SetKind;
use warrior_core::sync::{Operation, OperationKind, SessionUpdatePayload};
use warrior_core::training_record::{TrainingExerciseRecord, TrainingRecord};
use warrior_frontend::training_set::{SetSegmentValues, SetValues};

use super::TRAINING_ID;

pub const CREATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4302";
pub const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4307";
pub const OTHER_EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4308";
pub const CREATE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4309";
pub const OTHER_CREATE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4310";
pub const DELETE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4311";
pub const REORDER_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4312";
pub const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4313";
pub const EXERCISE_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4314";
pub const EXERCISE_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4315";
pub const EXERCISE_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4316";
pub const OTHER_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4317";
pub const OTHER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4318";
pub const OTHER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4319";
pub const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4320";

/// 種目を 2 つ持つサーバーの記録。
pub fn two_exercise_server_record() -> TrainingRecord {
    let mut value = serde_json::to_value(super::server_record(TRAINING_ID, "server"))
        .expect("the server record must serialize");
    value["exercises"][0]["id"] = json!(EXERCISE_ROW_ID);
    value["exercises"][0]["exercise_id"] = json!(EXERCISE_ID);
    value["sets"][0]["id"] = json!(EXERCISE_SET_ID);
    value["sets"][0]["training_exercise_id"] = json!(EXERCISE_ROW_ID);
    value["segments"][0]["id"] = json!(EXERCISE_SEGMENT_ID);
    value["segments"][0]["set_id"] = json!(EXERCISE_SET_ID);
    value["supersets"][0]["id"] = json!(EXERCISE_SUPERSET_ID);
    value["supersets"][0]["members"][0]["superset_id"] = json!(EXERCISE_SUPERSET_ID);
    value["supersets"][0]["members"][0]["training_exercise_id"] = json!(EXERCISE_ROW_ID);
    value["exercises"]
        .as_array_mut()
        .expect("the exercises must be an array")
        .push(json!({
            "id": OTHER_EXERCISE_ROW_ID,
            "training_id": TRAINING_ID,
            "exercise_id": OTHER_EXERCISE_ID,
            "snapshot": "{}",
            "position": 2,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }));
    value["sets"]
        .as_array_mut()
        .expect("the sets must be an array")
        .push(json!({
            "id": OTHER_SET_ID,
            "training_exercise_id": OTHER_EXERCISE_ROW_ID,
            "position": 1,
            "kind": "normal",
            "completed": 1,
            "rpe": null,
            "grip": null,
            "recorded_at": null,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }));
    value["segments"]
        .as_array_mut()
        .expect("the segments must be an array")
        .push(json!({
            "id": OTHER_SEGMENT_ID,
            "set_id": OTHER_SET_ID,
            "position": 1,
            "weight_kg": 50.0,
            "reps": 8,
            "momentum_reps": 0,
            "assisted_reps": 0,
            "duration_seconds": null,
            "left_weight_kg": null,
            "left_reps": null,
            "left_duration_seconds": null,
            "right_weight_kg": null,
            "right_reps": null,
            "right_duration_seconds": null,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }));
    value["supersets"][0]["members"]
        .as_array_mut()
        .expect("the members must be an array")
        .push(json!({
            "superset_id": EXERCISE_SUPERSET_ID,
            "training_exercise_id": OTHER_EXERCISE_ROW_ID,
            "position": 2
        }));
    serde_json::from_value(value).expect("the server record must deserialize")
}

/// メモの更新の操作。
pub fn update_operation(operation_id: &str, memo: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionUpdate,
        parent_operation_id: None,
        payload: serde_json::to_value(SessionUpdatePayload {
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: memo.to_owned(),
        })
        .expect("an update payload"),
    }
}

/// 1 段のセットの値。
pub fn set_values(weight_kg: f64, reps: u32) -> SetValues {
    SetValues {
        kind: SetKind::Normal,
        segments: vec![SetSegmentValues {
            weight_kg,
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left: None,
            right: None,
        }],
        completed: false,
        body_weight_kg: None,
        rpe: None,
        grip: None,
    }
}

/// テスト用の種目の内容。
pub fn exercise_snapshot() -> serde_json::Value {
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

/// テスト用の種目の行。
pub fn training_exercise_record(id: &str, position: i64) -> TrainingExerciseRecord {
    TrainingExerciseRecord {
        id: id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        exercise_id: EXERCISE_ID.to_owned(),
        snapshot: exercise_snapshot().to_string(),
        position,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

/// 種目の追加の操作。
pub fn exercise_create_operation(operation_id: &str, row_id: &str, position: i64) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseCreate,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "training_exercise_id": row_id,
            "exercise_id": EXERCISE_ID,
            "snapshot": exercise_snapshot(),
            "position": position
        }),
    }
}

/// 種目の削除の操作。
pub fn exercise_delete_operation(operation_id: &str, row_id: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseDelete,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "training_exercise_id": row_id
        }),
    }
}

/// 種目の並べ替えの操作。
pub fn exercise_reorder_operation(operation_id: &str, ordered_ids: &[String]) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseReorder,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "ordered_ids": ordered_ids
        }),
    }
}
