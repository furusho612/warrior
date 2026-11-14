//! 反動と補助の回数、RPE の性質テスト。
//!
//! 対応する要求: `REQ-SET-006`、`REQ-SET-007`。
//! 任意の回数と反動と補助に対してストリクト回数が 0 以上で回数以下になり、
//! 0012 の計算 (総重量、推定 1RM) と一致することを確かめる。
//! RPE は 6.0 から 10.0 までの 0.5 刻みだけを受理することを確かめる。

use proptest::prelude::*;
use warrior_core::rpe;
use warrior_core::set_calc::{
    estimated_one_rep_max, strict_reps, BestKey, RecordMode, SetKind, SetRecord, SetSegment,
    SetValidationError,
};

fn key() -> BestKey {
    BestKey {
        exercise_id: "bench-press".to_owned(),
        grip: None,
        attachment: None,
    }
}

fn weight() -> impl Strategy<Value = f64> {
    0.5f64..=500.0
}

fn valid_breakdown() -> impl Strategy<Value = (u32, u32, u32)> {
    (1u32..=30).prop_flat_map(|reps| {
        (0..=reps).prop_flat_map(move |momentum| {
            (0..=(reps - momentum)).prop_map(move |assisted| (reps, momentum, assisted))
        })
    })
}

fn record(reps: u32, momentum: u32, assisted: u32, weight_kg: f64) -> SetRecord {
    SetRecord::new(
        key(),
        SetKind::Normal,
        vec![SetSegment {
            weight_kg,
            reps,
            momentum_reps: momentum,
            assisted_reps: assisted,
            left: None,
            right: None,
            duration_seconds: None,
        }],
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .expect("a valid breakdown")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn req_set_006_strict_reps_is_bounded_for_any_breakdown(
        reps in 0u32..=1_000,
        momentum in 0u32..=1_000,
        assisted in 0u32..=1_000,
    ) {
        let strict = strict_reps(reps, momentum, assisted);
        prop_assert_eq!(strict, reps.saturating_sub(momentum).saturating_sub(assisted));
        prop_assert!(strict <= reps);
    }

    #[test]
    fn req_set_006_total_weight_counts_assistance_but_not_momentum(
        (reps, momentum, assisted) in valid_breakdown(),
        weight_kg in weight(),
    ) {
        let record = record(reps, momentum, assisted, weight_kg);
        let total_reps = f64::from(reps.saturating_sub(assisted));
        let strict = f64::from(reps - momentum - assisted);
        prop_assert!((record.total_weight_kg() - weight_kg * total_reps).abs() < 1e-9);
        prop_assert!((record.strict_total_weight_kg() - weight_kg * strict).abs() < 1e-9);
    }

    #[test]
    fn req_set_006_one_rep_max_uses_the_strict_reps(
        (reps, momentum, assisted) in valid_breakdown(),
        weight_kg in weight(),
    ) {
        let record = record(reps, momentum, assisted, weight_kg);
        let strict = reps - momentum - assisted;
        prop_assert_eq!(
            record.estimated_one_rep_max(),
            estimated_one_rep_max(weight_kg, strict)
        );
    }

    #[test]
    fn req_set_006_a_breakdown_over_the_reps_is_rejected(
        reps in 1u32..=30,
        momentum in 0u32..=30,
        assisted in 0u32..=30,
    ) {
        prop_assume!(momentum + assisted > reps);
        let result = SetRecord::new(
            key(),
            SetKind::Normal,
            vec![SetSegment {
                weight_kg: 100.0,
                reps,
                momentum_reps: momentum,
                assisted_reps: assisted,
                left: None,
                right: None,
                duration_seconds: None,
            }],
            None,
            None,
            RecordMode::WeightAndReps,
        );
        prop_assert_eq!(result, Err(SetValidationError::TooManyAssistedReps));
    }

    #[test]
    fn req_set_007_rpe_accepts_every_half_step(half_steps in 12u32..=20) {
        let value = f64::from(half_steps) / 2.0;
        prop_assert!(rpe::is_valid(value));
        prop_assert_eq!(rpe::parse(&rpe::format(value)), Ok(Some(value)));
    }

    #[test]
    fn req_set_007_rpe_rejects_other_tenths(tenths in 0u32..=200) {
        let value = f64::from(tenths) / 10.0;
        let expected = (60..=100).contains(&tenths) && tenths % 5 == 0;
        prop_assert_eq!(rpe::is_valid(value), expected);
    }
}
