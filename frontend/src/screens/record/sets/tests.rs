//! セットの入力と表のブラウザテスト。
//!
//! 対応する要求: `REQ-LIB-004`、`REQ-LIB-011`、`REQ-PRF-005`、`REQ-SET-003`、`REQ-SET-004`、
//! `REQ-SET-006`、`REQ-SET-007`、`REQ-SET-010`、`REQ-SET-011`。

use warrior_core::exercise::{
    Attachment, GripMode, GripSelection, GripStyle, GripWidth, Laterality,
};
use warrior_core::previous::PreviousSet;
use warrior_core::set_calc::{RecordMode, SetKind, SetSegment, SideValue};
use warrior_core::training_record::SetSegmentRecord;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

use super::{
    attachment_styles, attachment_with, attachment_with_style, default_grip, detail_open_label,
    fill_missing_grips, grip_attachments, grip_styles, grip_widths, grip_with_style,
    grip_with_width, needs_segment_confirmation, parse_count_or_zero, previous_response_is_current,
    previous_values, set_numbers, SegmentView, SetView,
};
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
    assert_eq!(values.segments[0].left.unwrap().weight_kg, Some(10.0));
    assert_eq!(values.segments[0].left.unwrap().reps, 5);
    assert_eq!(values.segments[0].right.unwrap().weight_kg, Some(5.0));
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
    assert_eq!(left.weight_kg, Some(0.0));
    assert_eq!(left.reps, 0);
    let right = values.segments[0].right.expect("the right side");
    assert_eq!(right.duration_seconds, Some(40));
    assert_eq!(right.weight_kg, Some(0.0));
    assert_eq!(right.reps, 0);
}

#[wasm_bindgen_test]
fn a_unilateral_weight_needs_an_explicit_weight_to_complete() {
    let mut view = set_view("normal");
    view.segments.push(segment_view("", "5", "", "4"));
    assert!(!view.can_complete(RecordMode::WeightAndReps, true));

    view.segments[0].left_weight = "0".to_owned();
    view.segments[0].right_weight = "0".to_owned();
    assert!(view.can_complete(RecordMode::WeightAndReps, true));

    let mut bodyweight = set_view("normal");
    bodyweight.segments.push(segment_view("", "5", "", "4"));
    assert!(bodyweight.can_complete(RecordMode::BodyweightAndReps, true));
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
fn req_set_008_warmup_sets_have_no_one_rep_max() {
    let mut view = set_view_with_inputs("100", "5");
    view.record.kind = "warmup".to_owned();
    assert_eq!(view.one_rep_max(RecordMode::WeightAndReps, false), None);
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

#[wasm_bindgen_test]
fn set_detail_open_labels_include_the_set_number() {
    assert_eq!(detail_open_label("1", false), "セット 1 の詳細を開く");
    assert_eq!(detail_open_label("2", false), "セット 2 の詳細を開く");
    assert_eq!(
        detail_open_label("D", true),
        "ドロップセット D の詳細を開く"
    );
}

#[wasm_bindgen_test]
fn req_set_006_parse_count_or_zero_only_accepts_empty_or_u32() {
    assert_eq!(parse_count_or_zero(""), Some(0));
    assert_eq!(parse_count_or_zero("0"), Some(0));
    assert_eq!(parse_count_or_zero("-1"), None);
    assert_eq!(parse_count_or_zero("1.5"), None);
    assert_eq!(parse_count_or_zero("4294967296"), None);
    assert_eq!(parse_count_or_zero("x"), None);
}

#[wasm_bindgen_test]
fn req_set_006_the_drawer_rejects_a_breakdown_of_invalid_counts() {
    for (momentum, assisted) in [
        ("-1", "0"),
        ("1.5", "0"),
        ("4294967296", "0"),
        ("x", "0"),
        ("0", "-1"),
    ] {
        let mut segment = breakdown_segment();
        segment.momentum = momentum.to_owned();
        segment.assisted = assisted.to_owned();
        assert!(
            segment.breakdown_invalid(false),
            "the invalid counts {momentum} and {assisted} must be rejected"
        );
    }
    let mut equal = breakdown_segment();
    equal.momentum = "3".to_owned();
    equal.assisted = "2".to_owned();
    assert!(!equal.breakdown_invalid(false));
    let empty = breakdown_segment();
    assert!(!empty.breakdown_invalid(false));
    let mut over = equal.clone();
    over.assisted = "3".to_owned();
    assert!(over.breakdown_invalid(false));
}

fn breakdown_segment() -> SegmentView {
    let mut segment = segment_view("", "5", "", "4");
    segment.reps = "5".to_owned();
    segment
}

fn segment_view(
    left_weight: &str,
    left_reps: &str,
    right_weight: &str,
    right_reps: &str,
) -> SegmentView {
    SegmentView {
        record: SetSegmentRecord {
            id: "segment".to_owned(),
            set_id: "set".to_owned(),
            position: 1,
            weight_kg: Some(0.0),
            reps: Some(0),
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left_weight_kg: None,
            left_reps: Some(0),
            left_duration_seconds: None,
            right_weight_kg: None,
            right_reps: Some(0),
            right_duration_seconds: None,
            created_at: "2026-11-12T00:00:00Z".to_owned(),
            updated_at: "2026-11-12T00:00:00Z".to_owned(),
        },
        weight: String::new(),
        reps: String::new(),
        momentum: String::new(),
        assisted: String::new(),
        duration: String::new(),
        left_weight: left_weight.to_owned(),
        left_reps: left_reps.to_owned(),
        left_duration: String::new(),
        right_weight: right_weight.to_owned(),
        right_reps: right_reps.to_owned(),
        right_duration: String::new(),
    }
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

fn grip_options() -> Vec<GripSelection> {
    let mut options = Vec::new();
    for style in GripStyle::ALL {
        for width in GripWidth::ALL {
            options.push(GripSelection::Grip { style, width });
        }
    }
    options
}

fn attachment_options() -> Vec<GripSelection> {
    let mut options = vec![
        GripSelection::Attachment {
            attachment: Attachment::WideBar,
            style: Some(GripStyle::Pronated),
        },
        GripSelection::Attachment {
            attachment: Attachment::WideBar,
            style: Some(GripStyle::Supinated),
        },
        GripSelection::Attachment {
            attachment: Attachment::StraightBar,
            style: Some(GripStyle::Pronated),
        },
        GripSelection::Attachment {
            attachment: Attachment::StraightBar,
            style: Some(GripStyle::Supinated),
        },
    ];
    for attachment in Attachment::ALL {
        if !attachment.requires_style() {
            options.push(GripSelection::Attachment {
                attachment,
                style: None,
            });
        }
    }
    options
}

#[wasm_bindgen_test]
fn req_set_011_the_default_grip_uses_the_latest_set_selection() {
    let options = grip_options();
    let latest = GripSelection::Grip {
        style: GripStyle::Supinated,
        width: GripWidth::Narrow,
    };
    assert_eq!(
        default_grip(GripMode::Grip, &options, Some(latest)),
        Some(latest)
    );
}

#[wasm_bindgen_test]
fn req_set_011_the_default_grip_falls_back_to_the_first_option() {
    let options = grip_options();
    assert_eq!(
        default_grip(GripMode::Grip, &options, None),
        options.first().copied()
    );
    let outside = GripSelection::Attachment {
        attachment: Attachment::MagWideOver,
        style: None,
    };
    assert_eq!(
        default_grip(GripMode::Grip, &options, Some(outside)),
        options.first().copied()
    );
}

#[wasm_bindgen_test]
fn req_set_011_the_default_grip_is_none_for_an_exercise_without_grip() {
    let options = grip_options();
    assert_eq!(default_grip(GripMode::None, &options, None), None);
    assert_eq!(
        default_grip(GripMode::None, &options, options.first().copied()),
        None
    );
}

#[wasm_bindgen_test]
fn req_set_011_loading_fills_missing_grips_with_the_latest_selection() {
    let options = grip_options();
    let first = set_view("normal");
    let second = set_view("normal");
    let mut sets = vec![first, second];
    fill_missing_grips(&mut sets, GripMode::Grip, &options);
    assert_eq!(sets[0].grip(), options.first().copied());
    assert_eq!(sets[1].grip(), options.first().copied());

    let selected = GripSelection::Grip {
        style: GripStyle::Supinated,
        width: GripWidth::Narrow,
    };
    let mut selected_set = set_view("normal");
    selected_set.set_grip(selected);
    let mut sets = vec![selected_set, set_view("normal")];
    fill_missing_grips(&mut sets, GripMode::Grip, &options);
    assert_eq!(sets[0].grip(), Some(selected));
    assert_eq!(sets[1].grip(), Some(selected));
}

#[wasm_bindgen_test]
fn req_set_011_a_previous_response_is_discarded_when_the_grip_changes() {
    let first = GripSelection::Grip {
        style: GripStyle::Pronated,
        width: GripWidth::Medium,
    };
    let second = GripSelection::Grip {
        style: GripStyle::Supinated,
        width: GripWidth::Narrow,
    };
    assert!(previous_response_is_current(Some(first), first));
    assert!(!previous_response_is_current(Some(second), first));
    assert!(!previous_response_is_current(None, first));
}

#[wasm_bindgen_test]
fn req_set_011_loading_keeps_grip_less_exercises_without_a_grip() {
    let options = grip_options();
    let mut sets = vec![set_view("normal")];
    fill_missing_grips(&mut sets, GripMode::None, &options);
    assert_eq!(sets[0].grip(), None);
}

#[wasm_bindgen_test]
fn req_set_011_loading_keeps_missing_grips_when_the_options_are_empty() {
    let mut sets = vec![set_view("normal")];
    fill_missing_grips(&mut sets, GripMode::Grip, &[]);
    assert_eq!(sets[0].grip(), None);
}

#[wasm_bindgen_test]
fn req_lib_011_the_grip_choices_follow_the_options() {
    let options = grip_options();
    assert_eq!(grip_styles(&options), GripStyle::ALL.to_vec());
    assert_eq!(
        grip_widths(&options, GripStyle::Supinated),
        GripWidth::ALL.to_vec()
    );
    let attachments = attachment_options();
    assert_eq!(
        grip_attachments(&attachments).first().copied(),
        Some(Attachment::WideBar)
    );
    assert_eq!(
        attachment_styles(&attachments, Attachment::StraightBar),
        vec![GripStyle::Pronated, GripStyle::Supinated]
    );
}

#[wasm_bindgen_test]
fn req_lib_011_a_style_change_keeps_a_shared_width() {
    let options = grip_options();
    let selection = GripSelection::Grip {
        style: GripStyle::Pronated,
        width: GripWidth::Medium,
    };
    assert_eq!(
        grip_with_style(selection, GripStyle::Supinated, &options),
        GripSelection::Grip {
            style: GripStyle::Supinated,
            width: GripWidth::Medium,
        }
    );
    assert_eq!(
        grip_with_width(selection, GripWidth::Narrow, &options),
        GripSelection::Grip {
            style: GripStyle::Pronated,
            width: GripWidth::Narrow,
        }
    );
}

#[wasm_bindgen_test]
fn req_lib_011_an_attachment_change_keeps_a_shared_style() {
    let options = attachment_options();
    let selection = GripSelection::Attachment {
        attachment: Attachment::WideBar,
        style: Some(GripStyle::Pronated),
    };
    let changed = attachment_with(selection, Attachment::StraightBar, &options);
    assert_eq!(
        changed,
        GripSelection::Attachment {
            attachment: Attachment::StraightBar,
            style: Some(GripStyle::Pronated),
        }
    );
    assert_eq!(
        attachment_with_style(changed, GripStyle::Supinated, &options),
        GripSelection::Attachment {
            attachment: Attachment::StraightBar,
            style: Some(GripStyle::Supinated),
        }
    );
}

#[wasm_bindgen_test]
fn req_lib_011_the_attachment_choices_keep_the_ten_kinds_in_order() {
    assert_eq!(
        grip_attachments(&attachment_options()),
        Attachment::ALL.to_vec()
    );
}

#[wasm_bindgen_test]
fn req_lib_011_a_styleless_attachment_clears_the_style() {
    let options = attachment_options();
    let selection = GripSelection::Attachment {
        attachment: Attachment::WideBar,
        style: Some(GripStyle::Pronated),
    };
    assert_eq!(
        attachment_with(selection, Attachment::ParallelGrip, &options),
        GripSelection::Attachment {
            attachment: Attachment::ParallelGrip,
            style: None,
        }
    );
    assert!(attachment_styles(&options, Attachment::ParallelGrip).is_empty());
    for attachment in Attachment::ALL
        .into_iter()
        .filter(|attachment| !attachment.requires_style())
    {
        assert!(
            attachment_styles(&options, attachment).is_empty(),
            "{attachment:?} must not have styles"
        );
    }
}

#[wasm_bindgen_test]
fn req_set_011_a_copied_previous_set_keeps_the_grip() {
    let mut previous = previous_set(None, 80.0);
    previous.grip = Some(GripSelection::Grip {
        style: GripStyle::Supinated,
        width: GripWidth::Narrow,
    });
    let values = previous_values(
        &previous,
        RecordMode::WeightAndReps,
        None,
        Laterality::Bilateral,
    )
    .expect("the previous values");
    assert_eq!(values.grip, previous.grip);
}
