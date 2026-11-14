//! 左右別のセットとスーパーセットの組の性質テスト。
//!
//! 対応する要求: `REQ-SET-004`、`REQ-SET-005`。
//! 左右別のセットの総重量が左右の合算であり、レストタイマーが組の最後の種目の
//! 完了のときだけ起動し、種目の削除と並べ替えの後も組の種目が同じトレーニングに
//! 属し、空にならないことを確かめる。

use proptest::prelude::*;
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};
use warrior_core::superset::{rest_timer_starts, SupersetMembers};

fn key() -> BestKey {
    BestKey {
        exercise_id: "unilateral".to_owned(),
        grip: None,
        attachment: None,
    }
}

fn side(weight_kg: f64, reps: u32) -> SideValue {
    SideValue {
        weight_kg,
        reps,
        duration_seconds: None,
    }
}

fn side_segments() -> impl Strategy<Value = (Vec<SetSegment>, f64)> {
    prop::collection::vec(
        (0.5f64..=300.0, 1u32..=20, 0.5f64..=300.0, 1u32..=20),
        1..=4,
    )
    .prop_map(|rows| {
        let mut expected = 0.0;
        let segments = rows
            .into_iter()
            .map(|(left_weight, left_reps, right_weight, right_reps)| {
                expected += left_weight * f64::from(left_reps);
                expected += right_weight * f64::from(right_reps);
                SetSegment {
                    weight_kg: 0.0,
                    reps: 0,
                    momentum_reps: 0,
                    assisted_reps: 0,
                    left: Some(side(left_weight, left_reps)),
                    right: Some(side(right_weight, right_reps)),
                    duration_seconds: None,
                }
            })
            .collect();
        (segments, expected)
    })
}

fn unique_members() -> impl Strategy<Value = Vec<String>> {
    prop::sample::subsequence(
        (0usize..8)
            .map(|index| format!("exercise-{index}"))
            .collect::<Vec<_>>(),
        2..=8,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_total_of_a_unilateral_set_is_the_sum_of_both_sides(
        (segments, expected) in side_segments(),
        kind in prop_oneof![Just(SetKind::Normal), Just(SetKind::Drop)],
    ) {
        let record = SetRecord::new(
            key(),
            kind,
            segments,
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid unilateral set");
        prop_assert!((record.total_weight_kg() - expected).abs() < 1e-9);
    }

    #[test]
    fn the_rest_timer_starts_only_for_the_last_member(
        members in unique_members(),
        completed_index in 0usize..=7,
        outside in "[a-z]{1,8}",
    ) {
        let completed = members
            .get(completed_index.min(members.len() - 1))
            .expect("a member")
            .clone();
        prop_assert_eq!(
            rest_timer_starts(Some(&members), &completed),
            members.last() == Some(&completed)
        );
        prop_assert!(rest_timer_starts(None, &completed));
        prop_assert!(!rest_timer_starts(Some(&members), &outside));
    }

    #[test]
    fn exercise_changes_keep_the_group_inside_the_training(
        training in unique_members(),
        group_indices in prop::sample::subsequence((0usize..8).collect::<Vec<_>>(), 2..=8),
        order in prop::sample::subsequence((0usize..8).collect::<Vec<_>>(), 2..=8),
        removed in 0usize..8,
    ) {
        let group = group_indices
            .iter()
            .filter_map(|index| training.get(*index).cloned())
            .collect::<Vec<_>>();
        let Ok(members) = SupersetMembers::new(group) else {
            return Ok(());
        };
        let order = order
            .iter()
            .filter_map(|index| training.get(*index).cloned())
            .collect::<Vec<_>>();
        if let Some(updated) = members.after_exercise_change(&order) {
            prop_assert!(updated.len() >= 2);
            prop_assert!(!updated.is_empty());
            for member in updated.as_slice() {
                prop_assert!(order.contains(member));
            }
            let mut expected = order
                .iter()
                .filter(|id| members.as_slice().contains(id))
                .cloned()
                .collect::<Vec<_>>();
            expected.dedup();
            prop_assert_eq!(updated.as_slice(), expected.as_slice());
        } else {
            let remaining = order
                .iter()
                .filter(|id| members.as_slice().contains(id))
                .count();
            prop_assert!(remaining < 2);
        }

        let removed_id = training
            .get(removed.min(training.len() - 1))
            .expect("an exercise")
            .clone();
        let remaining = training
            .iter()
            .filter(|id| **id != removed_id)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(updated) = members.after_exercise_change(&remaining) {
            prop_assert!(!updated.as_slice().contains(&removed_id));
            prop_assert!(updated.len() >= 2);
            for member in updated.as_slice() {
                prop_assert!(remaining.contains(member));
            }
        }
    }
}
