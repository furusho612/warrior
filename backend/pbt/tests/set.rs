//! セットの payload から記録への変換の性質テスト。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-PRF-005`。
//! 自重種目のセットが、記録した時点の体重を足した重量になり、体重が無いときは
//! 重量と自己ベストを持たないことを確かめる。セット種別ごとの計算と自己ベストは
//! `set_calc.rs`、反動と補助の回数、RPE は `reps_rpe.rs` が担う。

use proptest::prelude::*;
use warrior_core::set_calc::{RecordMode, SetKind};
use warrior_core::sync::{SetCreatePayload, SetSegmentPayload};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

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
