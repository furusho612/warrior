//! セットの計算規則の性質テスト。
//!
//! 対応する要求: `REQ-SET-004`、`REQ-SET-006`、`REQ-SET-008`、`REQ-SET-009`。
//! 推定 1RM の単調性、左右別の総重量、総重量の規則、部位別のセット数、自己ベストの
//! 対象、左右別の段で反動と補助を左右それぞれに適用することを確かめる。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::set_calc::{
    estimated_one_rep_max, BestKey, PartRole, PersonalBest, RecordMode, SetKind, SetRecord,
    SetSegment, SetValidationError, Side, SideValue,
};

#[path = "support/set.rs"]
mod set_fixture;

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

fn breakdown() -> impl Strategy<Value = (u32, u32, u32)> {
    set_fixture::valid_breakdown()
}

fn weight() -> impl Strategy<Value = f64> {
    set_fixture::weight()
}

fn segment() -> impl Strategy<Value = SetSegment> {
    (weight(), breakdown()).prop_map(|(weight, (reps, momentum, assisted))| SetSegment {
        weight_kg: weight,
        reps,
        momentum_reps: momentum,
        assisted_reps: assisted,
        left: None,
        right: None,
        duration_seconds: None,
    })
}

fn record() -> impl Strategy<Value = SetRecord> {
    (0usize..=3, prop::collection::vec(segment(), 1..=4)).prop_map(|(kind, segments)| {
        let kind = match kind {
            0 => SetKind::Normal,
            1 => SetKind::Warmup,
            _ => SetKind::Drop,
        };
        SetRecord::new(key(), kind, segments, None, None, RecordMode::WeightAndReps).unwrap()
    })
}

fn unilateral_breakdown() -> impl Strategy<Value = (u32, u32, u32, u32)> {
    (1u32..=30).prop_flat_map(|left_reps| {
        (1u32..=30).prop_flat_map(move |right_reps| {
            let fit = left_reps.min(right_reps);
            (0..=fit).prop_flat_map(move |momentum_reps| {
                (0..=(fit - momentum_reps)).prop_map(move |assisted_reps| {
                    (left_reps, right_reps, momentum_reps, assisted_reps)
                })
            })
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn one_rep_max_for_one_rep_equals_the_weight(weight in weight()) {
        prop_assert_eq!(estimated_one_rep_max(weight, 1), Some(weight));
    }

    #[test]
    fn one_rep_max_is_positive_and_finite(weight in weight(), reps in 1u32..=30) {
        let value = estimated_one_rep_max(weight, reps).expect("a value");
        prop_assert!(value.is_finite());
        prop_assert!(value > 0.0);
    }

    #[test]
    fn one_rep_max_is_monotonic_in_weight(
        first in weight(),
        second in weight(),
        reps in 1u32..=20,
    ) {
        let (low, high) = if first <= second { (first, second) } else { (second, first) };
        let low_value = estimated_one_rep_max(low, reps).unwrap();
        let high_value = estimated_one_rep_max(high, reps).unwrap();
        prop_assert!(low_value <= high_value);
    }

    #[test]
    fn one_rep_max_is_monotonic_in_reps(weight in weight(), first in 1u32..=20, second in 1u32..=20) {
        let (low, high) = if first <= second { (first, second) } else { (second, first) };
        let low_value = estimated_one_rep_max(weight, low).unwrap();
        let high_value = estimated_one_rep_max(weight, high).unwrap();
        prop_assert!(low_value <= high_value);
    }

    #[test]
    fn drop_sets_use_only_the_first_segment_for_best_values(segments in prop::collection::vec(segment(), 2..=4)) {
        let record = SetRecord::new(
            key(),
            SetKind::Drop,
            segments,
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .unwrap();
        let first = &record.segments()[0];
        prop_assert_eq!(record.max_weight_kg(), first.max_weight_kg());
        prop_assert_eq!(record.estimated_one_rep_max(), first.estimated_one_rep_max());
    }

    #[test]
    fn part_set_counts_follow_the_set_kind(record in record()) {
        let expected = if record.kind() == SetKind::Warmup { 0.0 } else { 1.0 };
        prop_assert_eq!(record.part_set_count(PartRole::Primary), expected);
        prop_assert_eq!(record.part_set_count(PartRole::Assisted), expected * 0.5);
        prop_assert_eq!(record.part_set_count(PartRole::Unrelated), 0.0);
    }

    #[test]
    fn personal_best_updates_from_the_same_comparison_unit(record in record()) {
        let mut best = PersonalBest::new(key());
        let updates = best.update(&record, date());
        let expected = record.kind() != SetKind::Warmup;
        prop_assert_eq!(updates.max_weight, expected && record.max_weight_kg().is_some());
        prop_assert_eq!(
            updates.estimated_one_rep_max,
            expected && record.estimated_one_rep_max().is_some()
        );
        prop_assert_eq!(updates.total_weight, record.strict_total_weight_kg() > 0.0);
        prop_assert!(!updates.longest_duration);
    }

    #[test]
    fn time_records_are_excluded_from_weight_and_count_as_one_set(duration in 1u32..=86_400) {
        let record = SetRecord::new(
            key(),
            SetKind::Normal,
            Vec::new(),
            None,
            Some(duration),
            RecordMode::Time,
        )
        .unwrap();
        prop_assert_eq!(record.total_weight_kg(), 0.0);
        prop_assert_eq!(record.estimated_one_rep_max(), None);
        prop_assert_eq!(record.counted_sets(), 1.0);
        prop_assert_eq!(record.longest_duration_seconds(), Some(duration));
    }

    #[test]
    fn unilateral_sides_are_summed_with_the_same_breakdown(
        (left_reps, right_reps, momentum_reps, assisted_reps) in unilateral_breakdown(),
        left_weight in weight(),
        right_weight in weight(),
    ) {
        let record = SetRecord::new(
            key(),
            SetKind::Normal,
            vec![SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps,
                assisted_reps,
                left: Some(SideValue {
                    weight_kg: left_weight,
                    reps: left_reps,
                    duration_seconds: None,
                }),
                right: Some(SideValue {
                    weight_kg: right_weight,
                    reps: right_reps,
                    duration_seconds: None,
                }),
                duration_seconds: None,
            }],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .unwrap();
        let left_strict = left_reps - momentum_reps - assisted_reps;
        let right_strict = right_reps - momentum_reps - assisted_reps;
        let expected_total = left_weight * f64::from(left_reps - assisted_reps)
            + right_weight * f64::from(right_reps - assisted_reps);
        let expected_strict_total =
            left_weight * f64::from(left_strict) + right_weight * f64::from(right_strict);
        let expected_one_rep_max = [
            estimated_one_rep_max(left_weight, left_strict),
            estimated_one_rep_max(right_weight, right_strict),
        ]
        .into_iter()
        .flatten()
        .max_by(|first, second| first.total_cmp(second));
        let segment = &record.segments()[0];
        prop_assert_eq!(
            segment
                .side(Side::Left)
                .unwrap()
                .strict_reps(momentum_reps, assisted_reps),
            left_strict
        );
        prop_assert_eq!(
            segment
                .side(Side::Right)
                .unwrap()
                .strict_reps(momentum_reps, assisted_reps),
            right_strict
        );
        prop_assert!((record.total_weight_kg() - expected_total).abs() < 1e-9);
        prop_assert!((record.strict_total_weight_kg() - expected_strict_total).abs() < 1e-9);
        prop_assert_eq!(record.estimated_one_rep_max(), expected_one_rep_max);
    }

    #[test]
    fn req_set_006_unilateral_assistance_over_either_side_is_rejected(
        left_reps in 1u32..=30,
        right_reps in 1u32..=30,
        assisted_reps in 1u32..=30,
    ) {
        prop_assume!(assisted_reps > left_reps || assisted_reps > right_reps);
        let result = SetRecord::new(
            key(),
            SetKind::Normal,
            vec![SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps,
                left: Some(SideValue {
                    weight_kg: 10.0,
                    reps: left_reps,
                    duration_seconds: None,
                }),
                right: Some(SideValue {
                    weight_kg: 20.0,
                    reps: right_reps,
                    duration_seconds: None,
                }),
                duration_seconds: None,
            }],
            None,
            None,
            RecordMode::WeightAndReps,
        );
        prop_assert_eq!(result, Err(SetValidationError::TooManyAssistedReps));
    }
}
