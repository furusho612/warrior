//! セットの種別ごとの集計の性質テスト。
//!
//! 対応する要求: `REQ-SET-002`、`REQ-LIB-004`、`REQ-PRF-005`。
//! 通常、ウォームアップ、ドロップセットの総重量、推定 1RM、自己ベスト、部位別の
//! セット数の扱いが warrior_core::set_calc の計算規則と一致することを確かめる。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::set_calc::{BestKey, PartRole, PersonalBest, RecordMode, SetKind, SetRecord};
use warrior_core::sync::{SetCreatePayload, SetSegmentPayload};

fn key() -> BestKey {
    BestKey {
        exercise_id: "bench-press".to_owned(),
        grip: None,
        attachment: None,
    }
}

fn date() -> JstDate {
    JstDate::new(2026, 11, 12).unwrap()
}

fn kind() -> impl Strategy<Value = SetKind> {
    prop_oneof![
        Just(SetKind::Normal),
        Just(SetKind::Warmup),
        Just(SetKind::Drop),
    ]
}

fn breakdown() -> impl Strategy<Value = (u32, u32, u32)> {
    (1u32..=20).prop_flat_map(|reps| {
        (0..=reps).prop_flat_map(move |momentum| {
            (0..=(reps - momentum)).prop_map(move |assisted| (reps, momentum, assisted))
        })
    })
}

fn weight() -> impl Strategy<Value = f64> {
    0.5f64..=500.0
}

fn segments() -> impl Strategy<Value = Vec<SetSegmentPayload>> {
    prop::collection::vec((weight(), breakdown()), 1..=4).prop_map(|rows| {
        rows.into_iter()
            .enumerate()
            .map(
                |(index, (weight_kg, (reps, momentum, assisted)))| SetSegmentPayload {
                    segment_id: format!("00000000-0000-4000-8000-{:012}", index + 1),
                    position: i64::try_from(index).unwrap() + 1,
                    weight_kg,
                    reps,
                    momentum_reps: momentum,
                    assisted_reps: assisted,
                    duration_seconds: None,
                    left: None,
                    right: None,
                },
            )
            .collect()
    })
}

fn payload() -> impl Strategy<Value = SetCreatePayload> {
    (kind(), segments()).prop_map(|(kind, segments)| SetCreatePayload {
        training_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3301".to_owned(),
        training_exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3302".to_owned(),
        set_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3303".to_owned(),
        position: 1,
        kind,
        rpe: None,
        grip: None,
        body_weight_kg: Some(70.0),
        segments,
    })
}

fn record(payload: &SetCreatePayload) -> SetRecord {
    payload
        .to_record("bench-press", RecordMode::WeightAndReps)
        .expect("a record")
}

fn expected_total(payload: &SetCreatePayload) -> f64 {
    if payload.kind == SetKind::Warmup {
        return 0.0;
    }
    payload
        .segments
        .iter()
        .map(|segment| {
            segment.weight_kg * f64::from(segment.reps.saturating_sub(segment.assisted_reps))
        })
        .sum()
}

fn strict_total(payload: &SetCreatePayload) -> f64 {
    payload
        .segments
        .iter()
        .map(|segment| {
            segment.weight_kg
                * f64::from(
                    segment
                        .reps
                        .saturating_sub(segment.momentum_reps)
                        .saturating_sub(segment.assisted_reps),
                )
        })
        .sum()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn total_weight_follows_the_set_kind(payload in payload()) {
        let record = record(&payload);
        let expected = expected_total(&payload);
        prop_assert!((record.total_weight_kg() - expected).abs() < 1e-9);
        if payload.kind == SetKind::Warmup {
            prop_assert_eq!(record.total_weight_kg(), 0.0);
        }
    }

    #[test]
    fn one_rep_max_and_max_weight_use_the_first_segment(payload in payload()) {
        let record = record(&payload);
        let first = payload.segments.first().map(|segment| {
            segment.to_segment(RecordMode::WeightAndReps, Some(70.0))
        });
        if payload.kind == SetKind::Warmup {
            prop_assert!(record.max_weight_kg().is_none());
            prop_assert!(record.estimated_one_rep_max().is_none());
        } else {
            let first = first.expect("a first segment");
            prop_assert_eq!(record.max_weight_kg(), first.max_weight_kg());
            prop_assert_eq!(record.estimated_one_rep_max(), first.estimated_one_rep_max());
            prop_assert_eq!(record.one_rep_max().map(|value| value.is_reference), first.one_rep_max().map(|value| value.is_reference));
        }
    }

    #[test]
    fn part_set_count_follows_the_set_kind(payload in payload()) {
        let record = record(&payload);
        let counted = if payload.kind == SetKind::Warmup { 0.0 } else { 1.0 };
        prop_assert_eq!(record.counted_sets(), counted);
        prop_assert_eq!(record.part_set_count(PartRole::Primary), counted);
        prop_assert_eq!(record.part_set_count(PartRole::Assisted), counted * 0.5);
        prop_assert_eq!(record.part_set_count(PartRole::Unrelated), 0.0);
    }

    #[test]
    fn personal_best_follows_the_set_kind(payload in payload()) {
        let record = record(&payload);
        let mut best = PersonalBest::new(key());
        let updates = best.update(&record, date());
        if payload.kind == SetKind::Warmup {
            prop_assert_eq!(updates, Default::default());
            prop_assert!(best.max_weight().is_none());
            prop_assert!(best.estimated_one_rep_max().is_none());
            prop_assert!(best.total_weight().is_none());
        } else {
            let expected_total = strict_total(&payload);
            prop_assert_eq!(best.max_weight().map(|value| value.value), record.max_weight_kg());
            prop_assert_eq!(best.estimated_one_rep_max().map(|value| value.value), record.estimated_one_rep_max());
            prop_assert_eq!(best.total_weight().map(|value| value.value), (expected_total > 0.0).then_some(expected_total));
        }
    }

    #[test]
    fn bodyweight_sets_add_the_recorded_body_weight(
        added in 0.0f64..=300.0,
        body in 20.0f64..=300.0,
        reps in 1u32..=20,
    ) {
        let payload = SetCreatePayload {
            training_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3301".to_owned(),
            training_exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3302".to_owned(),
            set_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3303".to_owned(),
            position: 1,
            kind: SetKind::Normal,
            rpe: None,
            grip: None,
            body_weight_kg: Some(body),
            segments: vec![SetSegmentPayload {
                segment_id: "00000000-0000-4000-8000-000000000001".to_owned(),
                position: 1,
                weight_kg: added,
                reps,
                momentum_reps: 0,
                assisted_reps: 0,
                duration_seconds: None,
                left: None,
                right: None,
            }],
        };
        let record = payload
            .to_record("push-up", RecordMode::BodyweightAndReps)
            .expect("a bodyweight record");
        prop_assert!((record.total_weight_kg() - (body + added) * f64::from(reps)).abs() < 1e-9);
        prop_assert_eq!(record.max_weight_kg(), Some(body + added));
    }

    #[test]
    fn bodyweight_sets_without_a_body_weight_have_no_weight(added in 0.0f64..=300.0, reps in 1u32..=20) {
        let payload = SetCreatePayload {
            training_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3301".to_owned(),
            training_exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3302".to_owned(),
            set_id: "3f2504e0-4f89-41d3-9a0c-0305e82c3303".to_owned(),
            position: 1,
            kind: SetKind::Normal,
            rpe: None,
            grip: None,
            body_weight_kg: None,
            segments: vec![SetSegmentPayload {
                segment_id: "00000000-0000-4000-8000-000000000001".to_owned(),
                position: 1,
                weight_kg: added,
                reps,
                momentum_reps: 0,
                assisted_reps: 0,
                duration_seconds: None,
                left: None,
                right: None,
            }],
        };
        let record = payload
            .to_record("push-up", RecordMode::BodyweightAndReps)
            .expect("a bodyweight record");
        prop_assert_eq!(record.total_weight_kg(), 0.0);
        prop_assert_eq!(record.max_weight_kg(), None);
        prop_assert_eq!(record.estimated_one_rep_max(), None);
    }
}
