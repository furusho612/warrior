//! 目標カロリーと PFC の計算の性質テスト。
//!
//! 対応する要求: `REQ-PRG-006`、`REQ-PRG-008`、`REQ-PRG-009`、`REQ-PRG-012`、
//! `REQ-PRG-013`、`REQ-PRG-015`、`REQ-PRG-016`、`REQ-PRG-018`。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{calculate, ActivityLevel, Goal, MacroTarget, NutritionInput, Sex};

fn birth_date() -> impl Strategy<Value = JstDate> {
    (1950i64..=2005, 1u32..=12, 1u32..=28)
        .prop_map(|(year, month, day)| JstDate::new(year, month, day).expect("a valid date"))
}

fn sex() -> impl Strategy<Value = Sex> {
    prop::sample::select(vec![Sex::Male, Sex::Female, Sex::Other])
}

fn goal() -> impl Strategy<Value = Goal> {
    prop::sample::select(vec![
        Goal::Hypertrophy,
        Goal::Diet,
        Goal::BodyMake,
        Goal::Strength,
        Goal::Performance,
    ])
}

fn activity() -> impl Strategy<Value = ActivityLevel> {
    prop::sample::select(vec![
        ActivityLevel::Low,
        ActivityLevel::Normal,
        ActivityLevel::High,
    ])
}

fn input() -> impl Strategy<Value = NutritionInput> {
    (
        100.0f64..=250.0,
        20.0f64..=300.0,
        birth_date(),
        sex(),
        goal(),
        0u32..=7,
        activity(),
        1.6f64..=2.2,
        0.05f64..=0.20,
    )
        .prop_map(
            |(
                height_cm,
                weight_kg,
                birth_date,
                sex,
                goal,
                training_days_per_week,
                activity_level,
                protein_g_per_kg,
                fat_ratio,
            )| NutritionInput {
                height_cm: Some(height_cm),
                weight_kg: Some(weight_kg),
                birth_date: Some(birth_date),
                sex: Some(sex),
                goal: Some(goal),
                training_days_per_week: Some(training_days_per_week),
                activity_level: Some(activity_level),
                macro_target: MacroTarget {
                    protein_g_per_kg,
                    fat_ratio,
                },
            },
        )
}

fn today() -> JstDate {
    JstDate::new(2026, 11, 12).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn target_calories_never_go_below_bmr(input in input()) {
        let result = calculate(&input, today()).expect("a valid input");
        prop_assert!(result.target_kcal >= result.basal_metabolism_kcal);
    }

    #[test]
    fn pfc_calories_match_the_target(input in input()) {
        let result = calculate(&input, today()).expect("a valid input");
        let total = result.protein_g * 4.0 + result.fat_g * 9.0 + result.carbohydrate_g * 4.0;
        prop_assert!((total - result.target_kcal).abs() < 1e-6);
    }

    #[test]
    fn pfc_values_stay_in_the_allowed_ranges(input in input()) {
        let weight = input.weight_kg.unwrap();
        let result = calculate(&input, today()).expect("a valid input");
        prop_assert!((1.6 * weight - 1e-6..=2.2 * weight + 1e-6).contains(&result.protein_g));
        prop_assert!((0.05 - 1e-6..=0.20 + 1e-6).contains(&result.macro_target.fat_ratio));
        prop_assert!(result.carbohydrate_g >= -1e-9);
    }

    #[test]
    fn the_same_input_always_returns_the_same_result(input in input()) {
        let first = calculate(&input, today()).expect("a valid input");
        let second = calculate(&input, today()).expect("a valid input");
        prop_assert_eq!(first, second);
    }

    #[test]
    fn diet_returns_a_loss_range_for_the_weight(input in input()) {
        let weight = input.weight_kg.unwrap();
        let mut input = input;
        input.goal = Some(Goal::Diet);
        let result = calculate(&input, today()).expect("a valid input");
        let loss = result.diet_loss_per_week.expect("a diet loss range");
        prop_assert!((loss.min_kg_per_week - weight * 0.005).abs() < 1e-9);
        prop_assert!((loss.max_kg_per_week - weight * 0.01).abs() < 1e-9);
    }

    #[test]
    fn non_diet_goals_do_not_return_a_loss_range(input in input()) {
        let mut input = input;
        input.goal = Some(Goal::Hypertrophy);
        let result = calculate(&input, today()).expect("a valid input");
        prop_assert_eq!(result.diet_loss_per_week, None);
    }
}
