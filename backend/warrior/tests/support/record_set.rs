use serde_json::{json, Value};

use super::*;
use support::record_common;

pub(crate) use support::record_common::{
    applied_count, cookie, count, count_with, first_row, status,
};

pub(crate) const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5501";
pub(crate) const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5502";
pub(crate) const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5510";
pub(crate) const FINISHED_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5511";
pub(crate) const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5512";
pub(crate) const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5520";
pub(crate) const OTHER_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5521";
pub(crate) const BENCH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5530";
pub(crate) const BODYWEIGHT_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5531";
pub(crate) const FINISHED_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5532";
pub(crate) const OTHER_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5533";
pub(crate) const BENCH_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5540";
pub(crate) const SECOND_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5541";
pub(crate) const BODY_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5542";
pub(crate) const BODY_NO_WEIGHT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5543";
pub(crate) const FINISHED_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5544";
pub(crate) const OTHER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5545";
pub(crate) const BENCH_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5550";
pub(crate) const EXTRA_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5551";
pub(crate) const BODY_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5552";
pub(crate) const BODY_NO_WEIGHT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5553";
pub(crate) const FINISHED_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5554";
pub(crate) const OTHER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5555";
pub(crate) const BENCH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5560";
pub(crate) const BODYWEIGHT_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5561";
pub(crate) const ORIGIN: &str = "http://localhost:8787";

pub(crate) fn seed_base(
    server: &DevServer,
    token: &str,
    terminal_token: &str,
    other_terminal_token: &str,
) {
    record_common::seed_user(
        server,
        USER_ID,
        "sub-record-set",
        "record-set@example.test",
        "セットの利用者",
    );
    record_common::seed_user(
        server,
        OTHER_USER_ID,
        "sub-record-set-other",
        "other@example.test",
        "別の利用者",
    );
    record_common::seed_auth_session(
        server,
        "3f2504e0-4f89-41d3-9a0c-0305e82c5570",
        USER_ID,
        token,
    );
    record_common::seed_terminal(server, TERMINAL_ID, USER_ID, terminal_token);
    record_common::seed_terminal(server, OTHER_TERMINAL_ID, USER_ID, other_terminal_token);
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, weight_kg, created_at, updated_at) \
             VALUES ('{USER_ID}', 70.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
    record_common::seed_session(
        server,
        TRAINING_ID,
        USER_ID,
        "2026-11-12",
        Some(TERMINAL_ID),
        None,
    );
    record_common::seed_session(
        server,
        FINISHED_TRAINING_ID,
        USER_ID,
        "2026-11-11",
        None,
        Some("2026-11-11T04:00:00Z"),
    );
    record_common::seed_session(
        server,
        OTHER_TRAINING_ID,
        OTHER_USER_ID,
        "2026-11-12",
        None,
        None,
    );
    record_common::seed_exercise(
        server,
        record_common::ExerciseSeed {
            row_id: BENCH_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: BENCH_EXERCISE_ID,
            name: "ベンチプレス",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "weight_and_reps",
            position: 1,
        },
    );
    record_common::seed_exercise(
        server,
        record_common::ExerciseSeed {
            row_id: BODYWEIGHT_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: BODYWEIGHT_EXERCISE_ID,
            name: "プッシュアップ",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "bodyweight_and_reps",
            position: 2,
        },
    );
    record_common::seed_exercise(
        server,
        record_common::ExerciseSeed {
            row_id: FINISHED_ROW_ID,
            user_id: USER_ID,
            training_id: FINISHED_TRAINING_ID,
            exercise_id: BENCH_EXERCISE_ID,
            name: "ベンチプレス",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "weight_and_reps",
            position: 1,
        },
    );
    record_common::seed_exercise(
        server,
        record_common::ExerciseSeed {
            row_id: OTHER_ROW_ID,
            user_id: OTHER_USER_ID,
            training_id: OTHER_TRAINING_ID,
            exercise_id: BENCH_EXERCISE_ID,
            name: "ベンチプレス",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "weight_and_reps",
            position: 1,
        },
    );
    record_common::seed_set(
        server,
        FINISHED_SET_ID,
        USER_ID,
        FINISHED_ROW_ID,
        1,
        "normal",
        1,
    );
    record_common::seed_segment(
        server,
        record_common::SegmentSeed {
            segment_id: FINISHED_SEGMENT_ID,
            user_id: USER_ID,
            set_id: FINISHED_SET_ID,
            position: 1,
            weight_kg: Some(60.0),
            reps: Some(5),
            duration_seconds: None,
        },
    );
    record_common::seed_set(
        server,
        OTHER_SET_ID,
        OTHER_USER_ID,
        OTHER_ROW_ID,
        1,
        "normal",
        1,
    );
    record_common::seed_segment(
        server,
        record_common::SegmentSeed {
            segment_id: OTHER_SEGMENT_ID,
            user_id: OTHER_USER_ID,
            set_id: OTHER_SET_ID,
            position: 1,
            weight_kg: Some(60.0),
            reps: Some(5),
            duration_seconds: None,
        },
    );
}

pub(crate) fn segment(
    segment_id: &str,
    position: i64,
    weight_kg: f64,
    reps: u32,
) -> serde_json::Value {
    json!({
        "segment_id": segment_id,
        "position": position,
        "weight_kg": weight_kg,
        "reps": reps,
        "momentum_reps": 0,
        "assisted_reps": 0,
        "duration_seconds": null,
        "left": null,
        "right": null
    })
}

/// セットの追加の操作に載せるセットの指定。
pub(crate) struct SetSpec<'a> {
    pub(crate) position: i64,
    pub(crate) kind: &'a str,
    pub(crate) body_weight_kg: Option<f64>,
}

pub(crate) fn create_operation(
    operation_id: &str,
    training_id: &str,
    training_exercise_id: &str,
    set_id: &str,
    spec: SetSpec<'_>,
    segments: Vec<serde_json::Value>,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 1,
        "kind": "set_create",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": training_exercise_id,
            "set_id": set_id,
            "position": spec.position,
            "kind": spec.kind,
            "rpe": null,
            "grip": null,
            "body_weight_kg": spec.body_weight_kg,
            "segments": segments
        }
    })
}

/// セットの更新の操作に載せるセットの指定。
pub(crate) struct UpdateSpec<'a> {
    pub(crate) position: i64,
    pub(crate) kind: &'a str,
    pub(crate) completed: bool,
    pub(crate) body_weight_kg: Option<f64>,
}

pub(crate) fn update_operation(
    operation_id: &str,
    training_id: &str,
    set_id: &str,
    spec: UpdateSpec<'_>,
    segments: Vec<serde_json::Value>,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "set_update",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "set_id": set_id,
            "position": spec.position,
            "kind": spec.kind,
            "completed": spec.completed,
            "rpe": null,
            "grip": null,
            "body_weight_kg": spec.body_weight_kg,
            "segments": segments
        }
    })
}

pub(crate) fn delete_operation(operation_id: &str, training_id: &str, set_id: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 3,
        "kind": "set_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "set_id": set_id
        }
    })
}

pub(crate) fn reorder_operation(
    operation_id: &str,
    training_id: &str,
    training_exercise_id: &str,
    ordered_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 4,
        "kind": "set_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": training_exercise_id,
            "ordered_ids": ordered_ids
        }
    })
}

pub(crate) fn complete_operation(
    operation_id: &str,
    training_id: &str,
    set_id: &str,
    completed: bool,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 5,
        "kind": "set_complete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "set_id": set_id,
            "completed": completed
        }
    })
}

pub(crate) fn post(server: &DevServer, cookie: &str, operation: &Value) -> support::HttpResponse {
    record_common::post_operation(server, cookie, operation, ORIGIN)
}
