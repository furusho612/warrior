//! CSV のセルの引用と復号の往復、メモのエスケープの往復、書き出しの行数の性質テスト。
//!
//! 対応する要求: `REQ-MYP-008`。

use proptest::prelude::*;
use warrior_core::csv::{
    column_names, encode_document, encode_row, join_memos, neutralize_cell, parse_document,
    parse_row, split_memos, CsvSet, COLUMNS, UTF8_BOM,
};
use warrior_core::exercise::{Attachment, BodyPart, GripSelection, GripStyle, GripWidth};
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};

fn any_cell() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..=64)
        .prop_map(|characters| characters.into_iter().collect())
}

fn any_set_kind() -> impl Strategy<Value = SetKind> {
    prop::sample::select(vec![SetKind::Normal, SetKind::Warmup, SetKind::Drop])
}

fn any_rpe() -> impl Strategy<Value = Option<f64>> {
    prop_oneof![
        Just(None),
        (12u32..=20).prop_map(|half| Some(f64::from(half) / 2.0)),
    ]
}

fn any_grip() -> impl Strategy<Value = GripSelection> {
    prop_oneof![
        Just(GripSelection::None),
        (0usize..3, 0usize..3).prop_map(|(style, width)| GripSelection::Grip {
            style: GripStyle::ALL[style],
            width: GripWidth::ALL[width],
        }),
        (0usize..10, prop::option::of(0usize..3)).prop_map(|(attachment, style)| {
            GripSelection::Attachment {
                attachment: Attachment::ALL[attachment],
                style: style.map(|style| GripStyle::ALL[style]),
            }
        }),
    ]
}

fn any_body_part() -> impl Strategy<Value = BodyPart> {
    (0usize..BodyPart::ALL.len()).prop_map(|index| BodyPart::ALL[index])
}

fn weight_segment() -> impl Strategy<Value = SetSegment> {
    prop_oneof![
        (0u32..=500, 1u32..=20).prop_map(|(weight_kg, reps)| SetSegment {
            weight_kg: f64::from(weight_kg),
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        }),
        (0u32..=500, 1u32..=20, 0u32..=500, 1u32..=20).prop_map(
            |(left_weight, left_reps, right_weight, right_reps)| SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps: 0,
                left: Some(SideValue {
                    weight_kg: f64::from(left_weight),
                    reps: left_reps,
                    duration_seconds: None,
                }),
                right: Some(SideValue {
                    weight_kg: f64::from(right_weight),
                    reps: right_reps,
                    duration_seconds: None,
                }),
                duration_seconds: None,
            },
        ),
    ]
}

fn time_segment() -> impl Strategy<Value = SetSegment> {
    prop_oneof![
        (1u32..=86_400).prop_map(|duration_seconds| SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: Some(duration_seconds),
        }),
        (1u32..=86_400, 1u32..=86_400).prop_map(|(left_seconds, right_seconds)| SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(left_seconds),
            }),
            right: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(right_seconds),
            }),
            duration_seconds: None,
        }),
    ]
}

fn mode_and_segments() -> impl Strategy<Value = (RecordMode, Vec<SetSegment>)> {
    prop_oneof![
        prop::collection::vec(weight_segment(), 1..=3)
            .prop_map(|segments| (RecordMode::WeightAndReps, segments)),
        prop::collection::vec(weight_segment(), 1..=3)
            .prop_map(|segments| (RecordMode::BodyweightAndReps, segments)),
        prop::collection::vec(time_segment(), 1..=3)
            .prop_map(|segments| (RecordMode::Time, segments)),
    ]
}

fn any_record() -> impl Strategy<Value = SetRecord> {
    (any_set_kind(), mode_and_segments(), any_rpe()).prop_map(|(kind, (mode, segments), rpe)| {
        SetRecord::new(
            BestKey {
                exercise_id: "exercise-1".to_owned(),
                grip: None,
                attachment: None,
            },
            kind,
            segments,
            rpe,
            None,
            mode,
        )
        .expect("the generated set satisfies the calculation rules")
    })
}

fn any_csv_set() -> impl Strategy<Value = CsvSet> {
    (
        any_record(),
        any_cell(),
        any_cell(),
        any_body_part(),
        1u32..=100,
        any_grip(),
        any_cell(),
    )
        .prop_map(
            |(record, date, exercise_name, body_part, set_number, grip, memo)| CsvSet {
                date,
                exercise_name,
                body_part,
                set_number,
                record,
                grip,
                memo,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn req_myp_008_any_cell_round_trips_through_quoting(cell in any_cell()) {
        let expected = neutralize_cell(&cell);
        let row = encode_row(&[cell]);
        let parsed = parse_row(&row).expect("the encoded row is CSV");
        prop_assert_eq!(parsed, vec![expected.clone()]);
        prop_assert!(!matches!(
            expected.chars().next(),
            Some('=' | '+' | '-' | '@')
        ));
    }

    #[test]
    fn req_myp_008_the_memos_round_trip_through_the_escaped_separator(
        training_memo in any_cell(),
        exercise_memo in any_cell(),
    ) {
        let cell = join_memos(&training_memo, &exercise_memo);
        let expected = [training_memo, exercise_memo]
            .into_iter()
            .filter(|memo| !memo.is_empty())
            .collect::<Vec<_>>();
        prop_assert_eq!(split_memos(&cell), expected);
    }

    #[test]
    fn req_myp_008_the_document_has_one_row_per_set(
        sets in prop::collection::vec(any_csv_set(), 0..=16),
    ) {
        let document = encode_document(&sets);
        prop_assert!(document.starts_with(UTF8_BOM.as_bytes()));
        let text = String::from_utf8(document).expect("the document is UTF-8");
        let rows = parse_document(&text).expect("the document is CSV");
        prop_assert_eq!(rows.len(), sets.len() + 1);
        prop_assert_eq!(rows[0].len(), COLUMNS.len());
        prop_assert_eq!(&rows[0], &column_names());
        for (row, set) in rows[1..].iter().zip(sets.iter()) {
            prop_assert_eq!(row.len(), COLUMNS.len());
            prop_assert_eq!(&row[0], &neutralize_cell(&set.date));
            prop_assert_eq!(&row[1], &neutralize_cell(&set.exercise_name));
            prop_assert_eq!(&row[3], &set.set_number.to_string());
            prop_assert_eq!(&row[11], &neutralize_cell(&set.memo));
        }
    }
}
