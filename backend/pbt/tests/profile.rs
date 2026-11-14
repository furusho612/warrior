//! 身体情報の更新と、記録した時点の体重の性質テスト。
//!
//! 対応する要求: `REQ-PRF-002`、`REQ-PRF-005`。
//! 身体情報を更新しても、セットを記録した時点の体重と重量が変わらないこと、
//! 4 つの身体情報が揃っているときだけ、記録時の体重を使えることを確かめる。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{ActivityLevel, Goal, Sex};
use warrior_core::profile::{BodyWeightAtRecord, Profile, TrainingExperience};

fn body_weight() -> impl Strategy<Value = f64> {
    20.0..=300.0
}

fn added_weight() -> impl Strategy<Value = f64> {
    0.0..=300.0
}

fn birth_date() -> impl Strategy<Value = JstDate> {
    (0i64..=9999, 1u32..=12, 1u32..=28)
        .prop_map(|(year, month, day)| JstDate::new(year, month, day).expect("a valid date"))
}

fn sex() -> impl Strategy<Value = Sex> {
    prop::sample::select(vec![Sex::Male, Sex::Female, Sex::Other])
}

fn experience() -> impl Strategy<Value = TrainingExperience> {
    prop::sample::select(vec![
        TrainingExperience::Beginner,
        TrainingExperience::Intermediate,
        TrainingExperience::Advanced,
    ])
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

fn complete_profile(weight_kg: f64) -> Profile {
    Profile {
        height_cm: Some(175.0),
        weight_kg: Some(weight_kg),
        birth_date: Some(JstDate::new(1990, 5, 10).expect("a valid date")),
        sex: Some(Sex::Male),
        training_experience: Some(TrainingExperience::Intermediate),
        goal: Some(Goal::Hypertrophy),
        training_days_per_week: Some(4),
        activity_level: Some(ActivityLevel::Normal),
    }
}

fn profile_strategy() -> impl Strategy<Value = Profile> {
    (
        prop::option::of(100.0..=250.0),
        prop::option::of(20.0..=300.0),
        prop::option::of(birth_date()),
        prop::option::of(sex()),
        prop::option::of(experience()),
        prop::option::of(goal()),
        prop::option::of(0u32..=7),
        prop::option::of(activity()),
    )
        .prop_map(
            |(
                height_cm,
                weight_kg,
                birth_date,
                sex,
                training_experience,
                goal,
                training_days_per_week,
                activity_level,
            )| Profile {
                height_cm,
                weight_kg,
                birth_date,
                sex,
                training_experience,
                goal,
                training_days_per_week,
                activity_level,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        .. ProptestConfig::default()
    })]

    #[test]
    fn updating_the_profile_keeps_the_recorded_set_weight(
        old_weight in body_weight(),
        new_weight in body_weight(),
        added in added_weight(),
    ) {
        let old_profile = complete_profile(old_weight);
        let recorded = BodyWeightAtRecord::from_profile(&old_profile);
        let before = recorded.set_weight_kg(added);

        let mut updated = old_profile;
        updated.weight_kg = Some(new_weight);

        prop_assert_eq!(recorded.body_weight_kg(), Some(old_weight));
        prop_assert_eq!(before, Some(old_weight + added));
        prop_assert_eq!(recorded.set_weight_kg(added), before);
        prop_assert_eq!(
            BodyWeightAtRecord::from_profile(&updated).set_weight_kg(added),
            Some(new_weight + added)
        );
    }

    #[test]
    fn the_recorded_body_weight_requires_the_four_body_fields(input in profile_strategy()) {
        let expected = input.body_profile().map(|body| body.weight_kg);
        prop_assert_eq!(
            BodyWeightAtRecord::from_profile(&input).body_weight_kg(),
            expected
        );
    }
}
