//! トレーニングのサマリーの性質テスト。
//!
//! 対応する要求: `REQ-REC-005`。
//! 任意のセットの列について、サマリーの総重量とセット数が `set_calc` の計算と一致し、
//! 種目数がセットを持つ種目の数になることを確かめる。

use proptest::prelude::*;
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};
use warrior_core::training_session::summary_from_sets;

fn key(index: u8) -> BestKey {
    BestKey {
        exercise_id: format!("exercise-{index}"),
        grip: None,
        attachment: None,
    }
}

fn breakdown() -> impl Strategy<Value = (u32, u32, u32)> {
    (1u32..=30).prop_flat_map(|reps| {
        (0..=reps).prop_flat_map(move |momentum| {
            (0..=(reps - momentum)).prop_map(move |assisted| (reps, momentum, assisted))
        })
    })
}

fn weight() -> impl Strategy<Value = f64> {
    0.5f64..=500.0
}

fn segment() -> impl Strategy<Value = SetSegment> {
    (weight(), breakdown()).prop_map(|(weight_kg, (reps, momentum_reps, assisted_reps))| {
        SetSegment {
            weight_kg,
            reps,
            momentum_reps,
            assisted_reps,
            left: None,
            right: None,
            duration_seconds: None,
        }
    })
}

fn unilateral_segment() -> impl Strategy<Value = SetSegment> {
    (weight(), weight(), 1u32..=30, 1u32..=30).prop_map(
        |(left_weight, right_weight, left_reps, right_reps)| SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
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
        },
    )
}

fn weight_record() -> impl Strategy<Value = SetRecord> {
    (
        0u8..3,
        0u8..3,
        prop_oneof![
            prop::collection::vec(segment(), 1..=4),
            unilateral_segment().prop_map(|segment| vec![segment]),
        ],
    )
        .prop_map(|(exercise, kind, segments)| {
            let kind = match kind {
                0 => SetKind::Normal,
                1 => SetKind::Warmup,
                _ => SetKind::Drop,
            };
            SetRecord::new(
                key(exercise),
                kind,
                segments,
                None,
                None,
                RecordMode::WeightAndReps,
            )
            .expect("the generated weight set is valid")
        })
}

fn time_record() -> impl Strategy<Value = SetRecord> {
    (0u8..3, 1u32..=86_400).prop_map(|(exercise, duration)| {
        SetRecord::new(
            key(exercise),
            SetKind::Normal,
            Vec::new(),
            None,
            Some(duration),
            RecordMode::Time,
        )
        .expect("the generated time set is valid")
    })
}

fn record() -> impl Strategy<Value = SetRecord> {
    prop_oneof![weight_record(), time_record()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn summary_total_weight_and_set_count_follow_the_set_calculations(
        sets in prop::collection::vec(record(), 0..=6),
        duration in 0i64..=86_400,
    ) {
        let summary = summary_from_sets(duration, &sets);
        let total_weight = sets.iter().map(SetRecord::total_weight_kg).sum::<f64>();
        let set_count = sets
            .iter()
            .map(|set| set.counted_sets() as u32)
            .sum::<u32>();
        let mut exercise_ids = sets
            .iter()
            .map(|set| set.key().exercise_id.clone())
            .collect::<Vec<_>>();
        exercise_ids.sort();
        exercise_ids.dedup();

        prop_assert_eq!(summary.duration_seconds, duration);
        prop_assert!((summary.total_weight_kg - total_weight).abs() < 1e-9);
        prop_assert_eq!(summary.set_count, set_count);
        prop_assert_eq!(
            summary.exercise_count,
            u32::try_from(exercise_ids.len()).expect("the count fits in u32")
        );
    }

    #[test]
    fn summary_personal_bests_are_positive_and_known_kinds(
        sets in prop::collection::vec(record(), 0..=6),
    ) {
        let summary = summary_from_sets(60, &sets);
        for item in &summary.personal_bests {
            prop_assert!(item.value > 0.0);
            prop_assert!(
                ["max_weight", "estimated_one_rep_max", "total_weight", "longest_duration"]
                    .contains(&item.kind.as_str())
            );
        }
    }
}
