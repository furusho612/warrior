//! セットの端末内の保存のテストが共有する行と値。

use warrior_core::set_calc::SetKind;
use warrior_core::sync::{Operation, OperationKind, SessionUpdatePayload};
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord, TrainingRecord};
use warrior_frontend::training_set::{SetSegmentValues, SetValues};

use super::TRAINING_ID;

/// テストで使う種目の行の ID。
pub const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5401";
/// テストで使う 2 つ目の段の ID。
pub const SECOND_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5410";
/// テストで使うトレーニングの作成の操作の ID。
pub const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5402";
/// テストで使うメモの更新の操作の ID。
pub const MEMO_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5403";

/// メモの更新の操作を作る。
pub fn memo_operation() -> Operation {
    let payload = SessionUpdatePayload {
        date: "2026-11-12".to_owned(),
        started_at: "2026-11-12T03:00:00Z".to_owned(),
        memo: "pending memo".to_owned(),
    };
    Operation {
        operation_id: MEMO_OPERATION_ID.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionUpdate,
        parent_operation_id: None,
        payload: serde_json::to_value(payload).expect("a memo payload"),
    }
}

/// サーバーに保存済みの 1 つのセットと段を持つ記録を作る。
pub fn server_set(
    set_id: &str,
    segment_id: &str,
    position: i64,
    weight_kg: f64,
    reps: i64,
) -> TrainingRecord {
    let mut record = super::server_record(TRAINING_ID, "");
    record.sets = vec![SetRecordRow {
        id: set_id.to_owned(),
        training_exercise_id: EXERCISE_ROW_ID.to_owned(),
        position,
        kind: "normal".to_owned(),
        completed: 0,
        rpe: None,
        grip: None,
        body_weight_kg: Some(70.0),
        recorded_at: None,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }];
    record.segments = vec![SetSegmentRecord {
        id: segment_id.to_owned(),
        set_id: set_id.to_owned(),
        position: 1,
        weight_kg: Some(weight_kg),
        reps: Some(reps),
        momentum_reps: 0,
        assisted_reps: 0,
        duration_seconds: None,
        left_weight_kg: None,
        left_reps: None,
        left_duration_seconds: None,
        right_weight_kg: None,
        right_reps: None,
        right_duration_seconds: None,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }];
    record
}

/// 重量と回数だけを持つセットの入力を作る。
pub fn values(kind: SetKind, weight_kg: f64, reps: u32) -> SetValues {
    values_for(kind, &[(weight_kg, reps)])
}

/// 段ごとの重量と回数を持つセットの入力を作る。
pub fn values_for(kind: SetKind, segments: &[(f64, u32)]) -> SetValues {
    SetValues {
        kind,
        segments: segments
            .iter()
            .map(|(weight_kg, reps)| SetSegmentValues {
                weight_kg: *weight_kg,
                reps: *reps,
                momentum_reps: 0,
                assisted_reps: 0,
                duration_seconds: None,
                left: None,
                right: None,
            })
            .collect(),
        completed: false,
        body_weight_kg: Some(70.0),
        rpe: None,
        grip: None,
    }
}
