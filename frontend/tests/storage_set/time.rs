//! 時間で記録するセットの端末内の保存のブラウザのテスト。
//!
//! 対応する要求: `REQ-SET-010`。
//! 時間で記録するセットは、重量と回数を保存せず、左右別の時間を側ごとに保存する。

use warrior_core::set_calc::{RecordMode, SetKind};
use warrior_core::sync::{SetCreatePayload, SetSidePayload};
use warrior_frontend::training_set::{self, SegmentSideValues, SetSegmentValues, SetValues};
use wasm_bindgen_test::wasm_bindgen_test;

use crate::support::set::{CREATE_OPERATION_ID, EXERCISE_ROW_ID};
use crate::support::{create_and_confirm, database_name, open, TRAINING_ID};

#[wasm_bindgen_test]
async fn req_set_010_time_sets_keep_the_duration_without_weight_or_reps() {
    let name = database_name("set-time");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let bilateral = SetValues {
        kind: SetKind::Normal,
        segments: vec![SetSegmentValues {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: Some(90),
            left: None,
            right: None,
        }],
        completed: false,
        body_weight_kg: Some(70.0),
        rpe: Some(8.0),
        grip: None,
    };
    let (bilateral_rows, bilateral_operation) =
        training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 1, &bilateral)
            .expect("a bilateral time set");
    storage
        .save_set_create(
            &bilateral_rows.set,
            &bilateral_rows.segments,
            &bilateral_operation,
        )
        .await
        .expect("the bilateral time set must be saved");

    let unilateral = SetValues {
        kind: SetKind::Normal,
        segments: vec![SetSegmentValues {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left: Some(SegmentSideValues {
                weight_kg: Some(0.0),
                reps: 0,
                duration_seconds: Some(30),
            }),
            right: Some(SegmentSideValues {
                weight_kg: Some(0.0),
                reps: 0,
                duration_seconds: Some(40),
            }),
        }],
        completed: false,
        body_weight_kg: Some(70.0),
        rpe: None,
        grip: None,
    };
    let (unilateral_rows, unilateral_operation) =
        training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 2, &unilateral)
            .expect("a unilateral time set");
    storage
        .save_set_create(
            &unilateral_rows.set,
            &unilateral_rows.segments,
            &unilateral_operation,
        )
        .await
        .expect("the unilateral time set must be saved");

    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 2);
    let bilateral_segment = &stored[0].segments[0];
    assert_eq!(bilateral_segment.weight_kg, None);
    assert_eq!(bilateral_segment.reps, None);
    assert_eq!(bilateral_segment.momentum_reps, 0);
    assert_eq!(bilateral_segment.assisted_reps, 0);
    assert_eq!(bilateral_segment.duration_seconds, Some(90));
    let unilateral_segment = &stored[1].segments[0];
    assert_eq!(unilateral_segment.left_weight_kg, None);
    assert_eq!(unilateral_segment.left_reps, None);
    assert_eq!(unilateral_segment.left_duration_seconds, Some(30));
    assert_eq!(unilateral_segment.right_weight_kg, None);
    assert_eq!(unilateral_segment.right_reps, None);
    assert_eq!(unilateral_segment.right_duration_seconds, Some(40));

    let payload: SetCreatePayload =
        serde_json::from_value(unilateral_operation.payload.clone()).expect("the create payload");
    assert_eq!(payload.segments[0].duration_seconds, None);
    assert_eq!(
        payload.segments[0].left,
        Some(SetSidePayload {
            weight_kg: Some(0.0),
            reps: 0,
            duration_seconds: Some(30),
        })
    );
    assert_eq!(
        payload.segments[0].right,
        Some(SetSidePayload {
            weight_kg: Some(0.0),
            reps: 0,
            duration_seconds: Some(40),
        })
    );
    let record = payload
        .to_record("side-plank", RecordMode::Time)
        .expect("a time record");
    assert_eq!(record.longest_duration_seconds(), Some(40));
    assert_eq!(record.total_duration_seconds(), 70);
    assert_eq!(record.total_weight_kg(), 0.0);
    assert_eq!(record.estimated_one_rep_max(), None);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}
