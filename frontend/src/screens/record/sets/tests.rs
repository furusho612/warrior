//! 対応する要求: `REQ-LIB-004`、`REQ-PRF-005`、`REQ-SET-003`。

use warrior_core::previous::PreviousSet;
use warrior_core::set_calc::{RecordMode, SetKind, SetSegment};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

use super::{needs_segment_confirmation, previous_values, set_numbers, SetView};

wasm_bindgen_test_configure!(run_in_browser);

fn previous_set(body_weight_kg: Option<f64>, weight_kg: f64) -> PreviousSet {
    PreviousSet {
        kind: SetKind::Normal,
        rpe: None,
        grip: None,
        body_weight_kg,
        segments: vec![SetSegment {
            weight_kg,
            reps: 5,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        }],
    }
}

#[wasm_bindgen_test]
fn copies_the_added_weight_from_the_previous_body_weight() {
    let previous = previous_set(Some(70.0), 80.0);
    let values = previous_values(&previous, RecordMode::BodyweightAndReps, Some(75.0))
        .expect("the previous values");
    assert_eq!(values.segments[0].weight_kg, 10.0);
    assert_eq!(values.body_weight_kg, Some(75.0));
    assert_eq!(values.segments[0].reps, 5);
}

#[wasm_bindgen_test]
fn copies_no_added_weight_without_the_previous_body_weight() {
    let previous = previous_set(None, 0.0);
    let values = previous_values(&previous, RecordMode::BodyweightAndReps, Some(75.0))
        .expect("the previous values");
    assert_eq!(values.segments[0].weight_kg, 0.0);
    assert_eq!(values.segments[0].reps, 5);
}

#[wasm_bindgen_test]
fn copies_the_weight_for_weight_and_reps() {
    let previous = previous_set(Some(70.0), 80.0);
    let values = previous_values(&previous, RecordMode::WeightAndReps, Some(75.0))
        .expect("the previous values");
    assert_eq!(values.segments[0].weight_kg, 80.0);
}

#[wasm_bindgen_test]
fn copies_every_segment_of_a_previous_drop_set() {
    let mut previous = previous_set(Some(70.0), 80.0);
    previous.kind = SetKind::Drop;
    previous.segments.push(SetSegment {
        weight_kg: 60.0,
        reps: 8,
        momentum_reps: 0,
        assisted_reps: 0,
        left: None,
        right: None,
        duration_seconds: None,
    });
    let values = previous_values(&previous, RecordMode::WeightAndReps, Some(75.0))
        .expect("the previous values");
    assert_eq!(values.kind, SetKind::Drop);
    assert_eq!(values.segments.len(), 2);
    assert_eq!(values.segments[1].weight_kg, 60.0);
    assert_eq!(values.segments[1].reps, 8);
}

#[wasm_bindgen_test]
fn drop_sets_do_not_take_a_set_number() {
    let rows = vec![set_view("normal"), set_view("drop"), set_view("normal")];
    assert_eq!(set_numbers(&rows), vec![Some(1), None, Some(2)]);
}

#[wasm_bindgen_test]
fn a_drop_set_with_two_segments_asks_before_changing_the_kind() {
    assert!(needs_segment_confirmation(
        SetKind::Drop,
        SetKind::Normal,
        2
    ));
    assert!(needs_segment_confirmation(
        SetKind::Drop,
        SetKind::Warmup,
        3
    ));
    assert!(!needs_segment_confirmation(
        SetKind::Drop,
        SetKind::Normal,
        1
    ));
    assert!(!needs_segment_confirmation(SetKind::Drop, SetKind::Drop, 2));
    assert!(!needs_segment_confirmation(
        SetKind::Normal,
        SetKind::Warmup,
        2
    ));
}

fn set_view(kind: &str) -> SetView {
    SetView {
        record: warrior_core::training_record::SetRecordRow {
            id: "set".to_owned(),
            training_exercise_id: "exercise".to_owned(),
            position: 1,
            kind: kind.to_owned(),
            completed: 0,
            rpe: None,
            grip: None,
            body_weight_kg: None,
            recorded_at: None,
            created_at: "2026-11-12T00:00:00Z".to_owned(),
            updated_at: "2026-11-12T00:00:00Z".to_owned(),
        },
        segments: Vec::new(),
        revision: 0,
    }
}
