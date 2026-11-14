//! 身体情報の更新と、記録した時点の体重の性質テスト。
//!
//! 対応する要求: `REQ-PRF-002`、`REQ-PRF-005`、`REQ-NFR-018`。
//! 記録時の体重が正の有限の値のときだけ、セットごとの加重を足すこと、
//! BMI と記録時の体重が身長と体重だけで決まること、身体情報を更新しても
//! 記録済みのセットの体重と重量が変わらないことを確かめる。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{ActivityLevel, Goal, Sex};
use warrior_core::profile::{
    bodyweight_set_weight, BodyWeightAtRecord, Profile, TrainingExperience,
};

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

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        .. ProptestConfig::default()
    })]

    #[test]
    fn a_positive_finite_body_weight_is_added_to_the_set_weight(
        body_weight_kg in any::<f64>(),
        added in added_weight(),
    ) {
        let expected = if body_weight_kg.is_finite() && body_weight_kg > 0.0 {
            Some(body_weight_kg + added)
        } else {
            None
        };
        prop_assert_eq!(bodyweight_set_weight(Some(body_weight_kg), added), expected);
    }

    #[test]
    fn a_set_without_a_body_weight_has_no_weight(added in any::<f64>()) {
        prop_assert_eq!(bodyweight_set_weight(None, added), None);
    }

    #[test]
    fn bmi_and_recorded_weight_use_only_the_height_and_weight(
        height_cm in 100.0..=250.0,
        weight_kg in 20.0..=300.0,
        birth_date in prop::option::of(birth_date()),
        sex_value in prop::option::of(sex()),
    ) {
        let input = Profile {
            height_cm: Some(height_cm),
            weight_kg: Some(weight_kg),
            birth_date,
            sex: sex_value,
            ..Profile::default()
        };
        let height_m = height_cm / 100.0;
        let expected_bmi = weight_kg / (height_m * height_m);
        prop_assert_eq!(input.bmi(), Some(expected_bmi));
        prop_assert_eq!(
            BodyWeightAtRecord::from_profile(&input).body_weight_kg(),
            Some(weight_kg)
        );
        let notice = input.safety_notice_on(JstDate::new(2026, 11, 12).expect("a date"));
        prop_assert_eq!(notice.underweight, expected_bmi < 18.5);
    }

    #[test]
    fn updating_the_profile_keeps_the_recorded_set_weight(
        old_weight in body_weight(),
        new_weight in body_weight(),
        added in added_weight(),
    ) {
        let old_profile = complete_profile(old_weight);
        let recorded = BodyWeightAtRecord::from_profile(&old_profile);
        let before = bodyweight_set_weight(recorded.body_weight_kg(), added);

        let mut updated = old_profile;
        updated.weight_kg = Some(new_weight);

        prop_assert_eq!(recorded.body_weight_kg(), Some(old_weight));
        prop_assert_eq!(before, Some(old_weight + added));
        prop_assert_eq!(bodyweight_set_weight(recorded.body_weight_kg(), added), before);
        prop_assert_eq!(
            bodyweight_set_weight(
                BodyWeightAtRecord::from_profile(&updated).body_weight_kg(),
                added,
            ),
            Some(new_weight + added)
        );
    }
}
