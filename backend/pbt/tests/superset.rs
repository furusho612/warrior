//! スーパーセットの組の性質テスト。
//!
//! 対応する要求: `REQ-SET-005`。
//! レストタイマーが組の最後の種目の完了のときだけ起動し、種目の削除と並べ替えの
//! 後も組の種目が同じトレーニングに属し、2 つ未満にならないことを確かめる。

use proptest::prelude::*;
use warrior_core::superset::{rest_timer_starts, SupersetMembers};

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
            let expected = order
                .iter()
                .filter(|id| members.as_slice().contains(id))
                .cloned()
                .collect::<Vec<_>>();
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
        } else {
            let remaining_count = remaining
                .iter()
                .filter(|id| members.as_slice().contains(id))
                .count();
            prop_assert!(remaining_count < 2);
        }
    }
}
