//! プログラムの推奨と進行の性質テスト (REQ-PRG-003、REQ-PRG-019、REQ-PRG-020)。

use proptest::prelude::*;
use warrior_core::program::{next_position, recommend, ProgramChoice};

fn choice(id: &str, goal: &str, level: &str, days: u32, sort_order: i64) -> ProgramChoice {
    ProgramChoice {
        id: id.to_owned(),
        goal: goal.to_owned(),
        level: level.to_owned(),
        days_per_week: days,
        sort_order,
    }
}

proptest! {
    #[test]
    fn the_next_position_stays_in_range_and_wraps(
        position in 1i64..=30,
        total in 1i64..=30,
    ) {
        let start = 1 + (position - 1) % total;
        let next = next_position(start, total);
        prop_assert!((1..=total).contains(&next));
        let expected = if start >= total { 1 } else { start + 1 };
        prop_assert_eq!(next, expected);
        let mut current = start;
        for _ in 0..total {
            current = next_position(current, total);
        }
        prop_assert_eq!(current, start);
    }

    #[test]
    fn the_recommendation_only_returns_matching_goals_and_levels(
        days in 0u32..=7,
        choices in prop::collection::vec(
            (
                "[a-z]{1,4}",
                prop::sample::select(vec!["hypertrophy", "diet", "strength"]),
                prop::sample::select(vec!["beginner", "intermediate", "advanced"]),
                0u32..=7,
                0i64..=20,
            ),
            0..20,
        ),
    ) {
        let choices = choices
            .into_iter()
            .enumerate()
            .map(|(index, (_, goal, level, days, order))| {
                choice(&format!("p{index}"), goal, level, days, order)
            })
            .collect::<Vec<_>>();
        let result = recommend(&choices, "hypertrophy", "beginner", days);
        for id in result.recommended.iter().chain(result.references.iter()) {
            let selected = choices.iter().find(|choice| &choice.id == id).expect("a known id");
            prop_assert_eq!(selected.goal.as_str(), "hypertrophy");
            prop_assert_eq!(selected.level.as_str(), "beginner");
        }
        let matching = choices
            .iter()
            .filter(|choice| choice.goal == "hypertrophy" && choice.level == "beginner")
            .collect::<Vec<_>>();
        if days == 0 {
            prop_assert!(result.recommended.is_empty());
        } else {
            let exact = matching
                .iter()
                .filter(|choice| choice.days_per_week == days)
                .collect::<Vec<_>>();
            if exact.is_empty() {
                prop_assert!(result.recommended.is_empty());
                prop_assert_eq!(result.references.len(), matching.len());
            } else {
                prop_assert_eq!(result.recommended.len(), exact.len());
                for choice in exact {
                    prop_assert!(result.recommended.contains(&choice.id));
                }
            }
        }
    }
}
