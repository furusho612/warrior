//! 対応する要求: `REQ-LIB-004`、`REQ-PRF-005`、`REQ-SET-003`、`REQ-SET-006`、`REQ-SET-007`、
//! `REQ-SET-010`。

use warrior_core::exercise::Laterality;
use warrior_core::previous::PreviousSet;
use warrior_core::set_calc::{RecordMode, SetKind, SetSegment, SideValue};
use warrior_core::training_record::SetSegmentRecord;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

use super::{previous_values, set_numbers, SetView};
use crate::text::Key;

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
    let values = previous_values(
        &previous,
        RecordMode::BodyweightAndReps,
        Some(75.0),
        Laterality::Bilateral,
    )
    .expect("the previous values");
    assert_eq!(values.segments[0].weight_kg, 10.0);
    assert_eq!(values.body_weight_kg, Some(75.0));
    assert_eq!(values.segments[0].reps, 5);
}

#[wasm_bindgen_test]
fn copies_no_added_weight_without_the_previous_body_weight() {
    let previous = previous_set(None, 0.0);
    let values = previous_values(
        &previous,
        RecordMode::BodyweightAndReps,
        Some(75.0),
        Laterality::Bilateral,
    )
    .expect("the previous values");
    assert_eq!(values.segments[0].weight_kg, 0.0);
    assert_eq!(values.segments[0].reps, 5);
}

#[wasm_bindgen_test]
fn copies_the_weight_for_weight_and_reps() {
    let previous = previous_set(Some(70.0), 80.0);
    let values = previous_values(
        &previous,
        RecordMode::WeightAndReps,
        Some(75.0),
        Laterality::Bilateral,
    )
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
    let values = previous_values(
        &previous,
        RecordMode::WeightAndReps,
        Some(75.0),
        Laterality::Bilateral,
    )
    .expect("the previous values");
    assert_eq!(values.kind, SetKind::Drop);
    assert_eq!(values.segments.len(), 2);
    assert_eq!(values.segments[1].weight_kg, 60.0);
    assert_eq!(values.segments[1].reps, 8);
}

#[wasm_bindgen_test]
fn copies_both_sides_of_a_unilateral_set() {
    let mut previous = previous_set(Some(70.0), 0.0);
    previous.segments[0].left = Some(SideValue {
        weight_kg: 80.0,
        reps: 5,
        duration_seconds: None,
    });
    previous.segments[0].right = Some(SideValue {
        weight_kg: 75.0,
        reps: 4,
        duration_seconds: None,
    });
    let values = previous_values(
        &previous,
        RecordMode::BodyweightAndReps,
        Some(70.0),
        Laterality::Unilateral,
    )
    .expect("the previous values");
    assert_eq!(values.segments[0].left.unwrap().weight_kg, 10.0);
    assert_eq!(values.segments[0].left.unwrap().reps, 5);
    assert_eq!(values.segments[0].right.unwrap().weight_kg, 5.0);
    assert_eq!(values.segments[0].right.unwrap().reps, 4);
}

#[wasm_bindgen_test]
fn req_set_010_copies_the_longest_duration_of_a_previous_time_set() {
    let mut previous = previous_set(None, 0.0);
    previous.segments[0].reps = 0;
    previous.segments[0].duration_seconds = Some(90);
    let values = previous_values(&previous, RecordMode::Time, None, Laterality::Bilateral)
        .expect("the previous values");
    assert_eq!(values.segments[0].duration_seconds, Some(90));
    assert_eq!(values.segments[0].weight_kg, 0.0);
    assert_eq!(values.segments[0].reps, 0);
    assert_eq!(values.segments[0].left, None);
    assert_eq!(values.segments[0].right, None);
}

#[wasm_bindgen_test]
fn req_set_010_copies_both_sides_of_a_previous_time_set() {
    let mut previous = previous_set(None, 0.0);
    previous.segments[0].reps = 0;
    previous.segments[0].left = Some(SideValue {
        weight_kg: 0.0,
        reps: 0,
        duration_seconds: Some(30),
    });
    previous.segments[0].right = Some(SideValue {
        weight_kg: 0.0,
        reps: 0,
        duration_seconds: Some(40),
    });
    let values = previous_values(&previous, RecordMode::Time, None, Laterality::Unilateral)
        .expect("the previous values");
    assert_eq!(values.segments[0].duration_seconds, None);
    let left = values.segments[0].left.expect("the left side");
    assert_eq!(left.duration_seconds, Some(30));
    assert_eq!(left.weight_kg, 0.0);
    assert_eq!(left.reps, 0);
    let right = values.segments[0].right.expect("the right side");
    assert_eq!(right.duration_seconds, Some(40));
    assert_eq!(right.weight_kg, 0.0);
    assert_eq!(right.reps, 0);
}

#[wasm_bindgen_test]
fn drop_sets_do_not_take_a_set_number() {
    let rows = vec![set_view("normal"), set_view("drop"), set_view("normal")];
    assert_eq!(set_numbers(&rows), vec![Some(1), None, Some(2)]);
}

#[wasm_bindgen_test]
fn req_set_007_the_drawer_formats_and_validates_rpe() {
    let mut record = set_view("normal").record;
    record.rpe = Some(6.5);
    let mut view = SetView::from_rows(record, Vec::new());
    assert_eq!(view.rpe, "6.5");
    assert_eq!(view.validation_error(false), None);
    view.rpe = "10.0".to_owned();
    assert_eq!(view.validation_error(false), None);
    view.rpe = "7.25".to_owned();
    assert_eq!(view.validation_error(false), Some(Key::RecordSetRpeInvalid));
    view.rpe = String::new();
    assert_eq!(view.validation_error(false), None);
}

#[wasm_bindgen_test]
fn req_set_008_one_rep_max_uses_the_weight_for_one_rep() {
    let view = set_view_with_inputs("100", "1");
    let value = view
        .one_rep_max(RecordMode::WeightAndReps, false)
        .expect("the estimated one rep max");

    assert_eq!(value.value_kg, 100.0);
    assert!(!value.is_reference);
}

#[wasm_bindgen_test]
fn req_set_008_one_rep_max_uses_the_formula_for_more_reps() {
    let view = set_view_with_inputs("100", "5");
    let value = view
        .one_rep_max(RecordMode::WeightAndReps, false)
        .expect("the estimated one rep max");
    let expected = warrior_core::set_calc::estimated_one_rep_max(100.0, 5).expect("the formula");

    assert_eq!(value.value_kg, expected);
    assert!(!value.is_reference);
}

#[wasm_bindgen_test]
fn req_set_008_zero_reps_have_no_one_rep_max_and_more_than_ten_reps_are_a_reference() {
    let zero = set_view_with_inputs("100", "0");
    assert_eq!(zero.one_rep_max(RecordMode::WeightAndReps, false), None);

    let eleven = set_view_with_inputs("100", "11");
    let value = eleven
        .one_rep_max(RecordMode::WeightAndReps, false)
        .expect("the reference value");
    assert!(value.is_reference);
}

#[wasm_bindgen_test]
fn req_set_008_time_records_have_no_one_rep_max() {
    let view = set_view_with_inputs("100", "5");
    assert_eq!(view.one_rep_max(RecordMode::Time, false), None);
}

#[wasm_bindgen_test]
fn req_set_008_a_unilateral_set_uses_the_larger_side() {
    let mut view = set_view_with_inputs("0", "0");
    view.segments[0].left_weight = "80".to_owned();
    view.segments[0].left_reps = "5".to_owned();
    view.segments[0].right_weight = "100".to_owned();
    view.segments[0].right_reps = "5".to_owned();
    let value = view
        .one_rep_max(RecordMode::WeightAndReps, true)
        .expect("the larger side");

    assert_eq!(
        value.value_kg,
        warrior_core::set_calc::estimated_one_rep_max(100.0, 5).unwrap()
    );
}

fn set_view_with_inputs(weight: &str, reps: &str) -> SetView {
    let mut view = SetView::from_rows(
        set_view("normal").record,
        vec![SetSegmentRecord {
            id: "segment".to_owned(),
            set_id: "set".to_owned(),
            position: 1,
            weight_kg: Some(0.0),
            reps: Some(0),
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left_weight_kg: None,
            left_reps: None,
            left_duration_seconds: None,
            right_weight_kg: None,
            right_reps: None,
            right_duration_seconds: None,
            created_at: "2026-11-12T00:00:00Z".to_owned(),
            updated_at: "2026-11-12T00:00:00Z".to_owned(),
        }],
    );
    view.segments[0].weight = weight.to_owned();
    view.segments[0].reps = reps.to_owned();
    view
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
        rpe: String::new(),
        revision: 0,
    }
}
