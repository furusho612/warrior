//! 目標カロリーと PFC の計算 (PRD の「計算ルールと用語定義」「目標カロリーと PFC」)。
//!
//! 年齢は日本時間の日付で満年齢を求める。2 月 29 日生まれの利用者は、閏年でない年の
//! 2 月 28 日に年を取るものとして扱う。

use serde::Serialize;

use crate::datetime;
use crate::jst::JstDate;

/// 性別。基礎代謝の係数に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Sex {
    /// 男性。
    Male,
    /// 女性。
    Female,
    /// その他。男性と女性の中間の係数を使う。
    Other,
}

impl Sex {
    /// 基礎代謝の式に足す係数。
    pub fn adjustment(self) -> f64 {
        match self {
            Sex::Male => 5.0,
            Sex::Female => -161.0,
            Sex::Other => -78.0,
        }
    }
}

/// 目的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Goal {
    /// 筋肥大。
    Hypertrophy,
    /// ダイエット。
    Diet,
    /// ボディメイク。
    BodyMake,
    /// 筋力向上。
    Strength,
    /// 競技力向上。
    Performance,
}

impl Goal {
    /// 消費カロリーに対する目標カロリーの調整率。
    pub fn calorie_adjustment(self) -> f64 {
        match self {
            Goal::Hypertrophy => 0.15,
            Goal::Diet => -0.15,
            Goal::BodyMake | Goal::Strength | Goal::Performance => 0.0,
        }
    }
}

/// 日常の活動量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ActivityLevel {
    /// 低い。
    Low,
    /// 普通。
    Normal,
    /// 高い。
    High,
}

impl ActivityLevel {
    /// 活動係数の幅。
    pub fn factor_range(self) -> (f64, f64) {
        match self {
            ActivityLevel::Low => (1.40, 1.69),
            ActivityLevel::Normal => (1.70, 1.99),
            ActivityLevel::High => (2.00, 2.39),
        }
    }
}

/// 身体情報。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// 身長 (cm)。
    pub height_cm: f64,
    /// 体重 (kg)。
    pub weight_kg: f64,
    /// 生年月日 (日本時間)。
    pub birth_date: JstDate,
    /// 性別。
    pub sex: Sex,
}

/// PFC の調整値。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroTarget {
    /// たんぱく質 (体重 1 kg あたりのグラム数)。
    pub protein_g_per_kg: f64,
    /// 脂質 (目標カロリーに対する割合)。
    pub fat_ratio: f64,
}

impl Default for MacroTarget {
    fn default() -> Self {
        Self {
            protein_g_per_kg: 2.0,
            fat_ratio: 0.20,
        }
    }
}

impl MacroTarget {
    /// 既定値に戻す。
    pub fn reset(self) -> Self {
        Self::default()
    }

    /// 範囲の境界に直す。直したかどうかも返す。
    pub fn clamp(self) -> (Self, bool) {
        let protein = self.protein_g_per_kg.clamp(1.6, 2.2);
        let fat = self.fat_ratio.clamp(0.0, 1.0);
        let clamped = protein != self.protein_g_per_kg || fat != self.fat_ratio;
        (
            Self {
                protein_g_per_kg: protein,
                fat_ratio: fat,
            },
            clamped,
        )
    }

    /// 指定が範囲内か。
    pub fn is_valid(self) -> bool {
        (1.6..=2.2).contains(&self.protein_g_per_kg) && (0.0..=1.0).contains(&self.fat_ratio)
    }
}

/// 入力の項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MissingField {
    /// 身長。
    Height,
    /// 体重。
    Weight,
    /// 生年月日。
    BirthDate,
    /// 性別。
    Sex,
    /// 目的。
    Goal,
    /// 週あたりのトレーニング日数。
    TrainingDays,
    /// 日常の活動量。
    ActivityLevel,
}

/// 栄養の計算の入力。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NutritionInput {
    /// 身長 (cm)。
    pub height_cm: Option<f64>,
    /// 体重 (kg)。
    pub weight_kg: Option<f64>,
    /// 生年月日 (日本時間)。
    pub birth_date: Option<JstDate>,
    /// 性別。
    pub sex: Option<Sex>,
    /// 目的。
    pub goal: Option<Goal>,
    /// 週あたりのトレーニング日数 (0 から 7)。
    pub training_days_per_week: Option<u32>,
    /// 日常の活動量。
    pub activity_level: Option<ActivityLevel>,
    /// PFC の調整値。
    pub macro_target: MacroTarget,
}

impl NutritionInput {
    /// 未入力の項目。入力の順に並べる。
    pub fn missing_fields(&self) -> Vec<MissingField> {
        let mut missing = Vec::new();
        if self.height_cm.is_none() {
            missing.push(MissingField::Height);
        }
        if self.weight_kg.is_none() {
            missing.push(MissingField::Weight);
        }
        if self.birth_date.is_none() {
            missing.push(MissingField::BirthDate);
        }
        if self.sex.is_none() {
            missing.push(MissingField::Sex);
        }
        if self.goal.is_none() {
            missing.push(MissingField::Goal);
        }
        if self.training_days_per_week.is_none() {
            missing.push(MissingField::TrainingDays);
        }
        if self.activity_level.is_none() {
            missing.push(MissingField::ActivityLevel);
        }
        missing
    }

    /// 4 つの身体情報が揃っているときの身体情報。
    pub fn profile(&self) -> Option<Profile> {
        Some(Profile {
            height_cm: self.height_cm?,
            weight_kg: self.weight_kg?,
            birth_date: self.birth_date?,
            sex: self.sex?,
        })
    }
}

/// ダイエットの減量の速さの目安。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DietLossRange {
    /// 週あたりの最小の減量 (kg)。
    pub min_kg_per_week: f64,
    /// 週あたりの最大の減量 (kg)。
    pub max_kg_per_week: f64,
}

/// 栄養の計算の結果。
#[derive(Debug, Clone, PartialEq)]
pub struct NutritionResult {
    /// 満年齢。
    pub age: u32,
    /// 基礎代謝 (kcal/日)。
    pub basal_metabolism_kcal: f64,
    /// 消費カロリー (kcal/日)。
    pub maintenance_kcal: f64,
    /// 目標カロリー (kcal/日)。
    pub target_kcal: f64,
    /// たんぱく質 (g/日)。
    pub protein_g: f64,
    /// 脂質 (g/日)。
    pub fat_g: f64,
    /// 炭水化物 (g/日)。
    pub carbohydrate_g: f64,
    /// 実際に使った PFC の調整値。
    pub macro_target: MacroTarget,
    /// 調整値を範囲の境界に直したか。
    pub macro_clamped: bool,
    /// ダイエットのときだけの減量の速さの目安。
    pub diet_loss_per_week: Option<DietLossRange>,
}

/// 栄養の計算の誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NutritionError {
    /// 未入力の項目がある。
    MissingFields(Vec<MissingField>),
    /// 身長が正の有限の値でない。
    InvalidHeight,
    /// 体重が正の有限の値でない。
    InvalidWeight,
    /// 生年月日が未来、または年齢を計算できない。
    InvalidBirthDate,
    /// 週あたりのトレーニング日数が 0 から 7 の範囲外である。
    InvalidTrainingDays,
    /// 基礎代謝が正の有限の値にならない。
    InvalidBasalMetabolism,
    /// PFC の調整値が範囲外である。
    InvalidMacroTarget,
    /// たんぱく質と脂質の合計が目標カロリーを超えている。
    MacroCaloriesExceedTarget,
}

/// 生年月日から、指定した日本時間の日付での満年齢を求める。
pub fn age_on(birth_date: JstDate, today: JstDate) -> Result<u32, NutritionError> {
    if birth_date > today {
        return Err(NutritionError::InvalidBirthDate);
    }
    let birthday_day = if birth_date.month() == 2
        && birth_date.day() == 29
        && !datetime::is_leap_year(i64::from(today.year()))
    {
        28
    } else {
        birth_date.day()
    };
    let had_birthday = (today.month(), today.day()) >= (birth_date.month(), birthday_day);
    let age = today.year() - birth_date.year() - i32::from(!had_birthday);
    if age < 0 {
        return Err(NutritionError::InvalidBirthDate);
    }
    Ok(age as u32)
}

/// 活動係数を求める。日数に比例して幅の下限から上限へ動かす。
pub fn activity_factor(level: ActivityLevel, days_per_week: u32) -> Result<f64, NutritionError> {
    if days_per_week > 7 {
        return Err(NutritionError::InvalidTrainingDays);
    }
    let (low, high) = level.factor_range();
    Ok(low + (high - low) * f64::from(days_per_week) / 7.0)
}

/// 入力から目標カロリーと PFC を計算する。
pub fn calculate(
    input: &NutritionInput,
    today: JstDate,
) -> Result<NutritionResult, NutritionError> {
    let missing = input.missing_fields();
    if !missing.is_empty() {
        return Err(NutritionError::MissingFields(missing));
    }

    let profile = input.profile().expect("all profile fields are present");
    if !profile.height_cm.is_finite() || profile.height_cm <= 0.0 {
        return Err(NutritionError::InvalidHeight);
    }
    if !profile.weight_kg.is_finite() || profile.weight_kg <= 0.0 {
        return Err(NutritionError::InvalidWeight);
    }
    let age = age_on(profile.birth_date, today)?;
    let goal = input.goal.expect("the goal is present");
    let activity = input.activity_level.expect("the activity level is present");
    let days = input
        .training_days_per_week
        .expect("the training days are present");
    let factor = activity_factor(activity, days)?;

    let basal_metabolism = basal_metabolism(profile, age);
    if !basal_metabolism.is_finite() || basal_metabolism <= 0.0 {
        return Err(NutritionError::InvalidBasalMetabolism);
    }
    let maintenance = basal_metabolism * factor;
    let target = (maintenance * (1.0 + goal.calorie_adjustment())).max(basal_metabolism);

    let (macro_target, macro_clamped) = input.macro_target.clamp();
    let protein_g = profile.weight_kg * macro_target.protein_g_per_kg;
    let fat_g = target * macro_target.fat_ratio / 9.0;
    let protein_and_fat_kcal = protein_g * 4.0 + fat_g * 9.0;
    if protein_and_fat_kcal > target + 1e-9 {
        return Err(NutritionError::MacroCaloriesExceedTarget);
    }
    let carbohydrate_g = (target - protein_and_fat_kcal) / 4.0;

    let diet_loss_per_week = (goal == Goal::Diet).then_some(DietLossRange {
        min_kg_per_week: profile.weight_kg * 0.005,
        max_kg_per_week: profile.weight_kg * 0.01,
    });

    Ok(NutritionResult {
        age,
        basal_metabolism_kcal: basal_metabolism,
        maintenance_kcal: maintenance,
        target_kcal: target,
        protein_g,
        fat_g,
        carbohydrate_g,
        macro_target,
        macro_clamped,
        diet_loss_per_week,
    })
}

/// 基礎代謝を求める。
pub fn basal_metabolism(profile: Profile, age: u32) -> f64 {
    10.0 * profile.weight_kg + 6.25 * profile.height_cm - 5.0 * f64::from(age)
        + profile.sex.adjustment()
}

#[cfg(test)]
mod tests {
    use super::{
        activity_factor, age_on, calculate, ActivityLevel, Goal, MacroTarget, MissingField,
        NutritionError, NutritionInput, Profile, Sex,
    };
    use crate::jst::JstDate;

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).unwrap()
    }

    fn sample_input() -> NutritionInput {
        NutritionInput {
            height_cm: Some(175.0),
            weight_kg: Some(70.0),
            birth_date: Some(date(1990, 5, 10)),
            sex: Some(Sex::Male),
            goal: Some(Goal::Hypertrophy),
            training_days_per_week: Some(4),
            activity_level: Some(ActivityLevel::Normal),
            macro_target: MacroTarget::default(),
        }
    }

    #[test]
    fn age_uses_the_japan_date() {
        assert_eq!(age_on(date(1990, 5, 10), date(2026, 5, 9)).unwrap(), 35);
        assert_eq!(age_on(date(1990, 5, 10), date(2026, 5, 10)).unwrap(), 36);
    }

    #[test]
    fn february_29_birthday_is_february_28_in_a_non_leap_year() {
        let birth = date(2000, 2, 29);
        assert_eq!(age_on(birth, date(2023, 2, 27)).unwrap(), 22);
        assert_eq!(age_on(birth, date(2023, 2, 28)).unwrap(), 23);
        assert_eq!(age_on(birth, date(2024, 2, 28)).unwrap(), 23);
        assert_eq!(age_on(birth, date(2024, 2, 29)).unwrap(), 24);
    }

    #[test]
    fn future_birth_date_is_rejected() {
        assert_eq!(
            age_on(date(2030, 1, 1), date(2026, 11, 12)),
            Err(NutritionError::InvalidBirthDate)
        );
    }

    #[test]
    fn activity_factor_moves_from_low_to_high_with_days() {
        assert!((activity_factor(ActivityLevel::Low, 0).unwrap() - 1.40).abs() < 1e-9);
        assert!((activity_factor(ActivityLevel::Low, 7).unwrap() - 1.69).abs() < 1e-9);
        assert!(
            (activity_factor(ActivityLevel::Low, 3).unwrap() - (1.40 + 0.29 * 3.0 / 7.0)).abs()
                < 1e-9
        );
    }

    #[test]
    fn goal_adjustments_match_the_prd() {
        assert!(Goal::Hypertrophy.calorie_adjustment() > 0.0);
        assert!(Goal::Diet.calorie_adjustment() < 0.0);
        for goal in [Goal::BodyMake, Goal::Strength, Goal::Performance] {
            assert_eq!(goal.calorie_adjustment(), 0.0);
        }
    }

    #[test]
    fn goals_change_the_target_around_the_maintenance() {
        let today = date(2026, 11, 12);
        let mut input = sample_input();
        input.activity_level = Some(ActivityLevel::High);
        input.training_days_per_week = Some(7);

        input.goal = Some(Goal::Hypertrophy);
        let hypertrophy = calculate(&input, today).unwrap();
        input.goal = Some(Goal::Diet);
        let diet = calculate(&input, today).unwrap();
        assert!(hypertrophy.target_kcal > hypertrophy.maintenance_kcal);
        assert!(diet.target_kcal < diet.maintenance_kcal);

        for goal in [Goal::BodyMake, Goal::Strength, Goal::Performance] {
            input.goal = Some(goal);
            let result = calculate(&input, today).unwrap();
            assert_eq!(result.target_kcal, result.maintenance_kcal);
        }
    }

    #[test]
    fn missing_fields_are_reported_in_input_order() {
        let missing = NutritionInput::default().missing_fields();
        assert_eq!(
            missing,
            vec![
                MissingField::Height,
                MissingField::Weight,
                MissingField::BirthDate,
                MissingField::Sex,
                MissingField::Goal,
                MissingField::TrainingDays,
                MissingField::ActivityLevel
            ]
        );
    }

    #[test]
    fn missing_fields_stop_the_calculation() {
        let mut input = sample_input();
        input.weight_kg = None;
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::MissingFields(vec![MissingField::Weight]))
        );
    }

    #[test]
    fn bmr_coefficients_for_all_sexes() {
        let profile = Profile {
            height_cm: 175.0,
            weight_kg: 70.0,
            birth_date: date(1990, 5, 10),
            sex: Sex::Male,
        };
        let male = super::basal_metabolism(profile, 36);
        let mut female_profile = profile;
        female_profile.sex = Sex::Female;
        let female = super::basal_metabolism(female_profile, 36);
        let mut other_profile = profile;
        other_profile.sex = Sex::Other;
        let other = super::basal_metabolism(other_profile, 36);
        assert_eq!(male - female, 166.0);
        assert_eq!(male - other, 83.0);
    }

    #[test]
    fn macros_default_to_two_grams_per_kilogram_and_twenty_percent() {
        let result = calculate(&sample_input(), date(2026, 11, 12)).unwrap();
        assert_eq!(result.macro_target, MacroTarget::default());
        assert_eq!(result.protein_g, 140.0);
    }

    #[test]
    fn macros_can_be_adjusted_at_the_boundaries() {
        for protein in [1.6, 2.2] {
            let mut input = sample_input();
            input.macro_target = MacroTarget {
                protein_g_per_kg: protein,
                fat_ratio: 0.20,
            };
            let result = calculate(&input, date(2026, 11, 12)).unwrap();
            assert_eq!(result.protein_g, 70.0 * protein);
        }
    }

    #[test]
    fn target_never_goes_below_bmr() {
        let mut input = sample_input();
        input.activity_level = Some(ActivityLevel::Low);
        input.training_days_per_week = Some(0);
        input.goal = Some(Goal::Diet);
        let result = calculate(&input, date(2026, 11, 12)).unwrap();
        assert!(result.target_kcal >= result.basal_metabolism_kcal);
    }

    #[test]
    fn pfc_calories_match_the_target() {
        let result = calculate(&sample_input(), date(2026, 11, 12)).unwrap();
        let total = result.protein_g * 4.0 + result.fat_g * 9.0 + result.carbohydrate_g * 4.0;
        assert!((total - result.target_kcal).abs() < 1e-9);
    }

    #[test]
    fn macro_target_clamps_to_the_boundaries() {
        let (clamped, changed) = MacroTarget {
            protein_g_per_kg: 3.0,
            fat_ratio: -0.1,
        }
        .clamp();
        assert!(changed);
        assert_eq!(
            clamped,
            MacroTarget {
                protein_g_per_kg: 2.2,
                fat_ratio: 0.0
            }
        );
    }

    #[test]
    fn protein_and_fat_over_the_target_is_rejected() {
        let mut input = sample_input();
        input.macro_target = MacroTarget {
            protein_g_per_kg: 2.2,
            fat_ratio: 1.0,
        };
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::MacroCaloriesExceedTarget)
        );
    }

    #[test]
    fn diet_returns_a_loss_range_and_other_goals_do_not() {
        let mut input = sample_input();
        input.goal = Some(Goal::Diet);
        let result = calculate(&input, date(2026, 11, 12)).unwrap();
        let loss = result.diet_loss_per_week.unwrap();
        assert!((loss.min_kg_per_week - 0.35).abs() < 1e-9);
        assert!((loss.max_kg_per_week - 0.70).abs() < 1e-9);

        input.goal = Some(Goal::Hypertrophy);
        let result = calculate(&input, date(2026, 11, 12)).unwrap();
        assert_eq!(result.diet_loss_per_week, None);
    }

    #[test]
    fn negative_bmr_is_rejected() {
        let mut input = sample_input();
        input.height_cm = Some(1.0);
        input.weight_kg = Some(1.0);
        input.birth_date = Some(date(2020, 1, 1));
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::InvalidBasalMetabolism)
        );
    }

    #[test]
    fn a_birth_date_that_makes_bmr_negative_is_rejected() {
        let mut input = sample_input();
        input.height_cm = Some(100.0);
        input.weight_kg = Some(20.0);
        input.birth_date = Some(date(1000, 1, 1));
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::InvalidBasalMetabolism)
        );
    }

    #[test]
    fn invalid_height_weight_and_days_are_rejected() {
        let mut input = sample_input();
        input.height_cm = Some(0.0);
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::InvalidHeight)
        );
        let mut input = sample_input();
        input.weight_kg = Some(0.0);
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::InvalidWeight)
        );
        let mut input = sample_input();
        input.training_days_per_week = Some(8);
        assert_eq!(
            calculate(&input, date(2026, 11, 12)),
            Err(NutritionError::InvalidTrainingDays)
        );
    }

    #[test]
    fn zero_carbohydrate_is_allowed() {
        let today = date(2026, 11, 12);
        let base = calculate(&sample_input(), today).unwrap();
        let protein_g = 70.0 * 2.2;
        let fat_ratio = (base.target_kcal - protein_g * 4.0) / base.target_kcal;
        assert!((0.0..=1.0).contains(&fat_ratio));
        let mut input = sample_input();
        input.macro_target = MacroTarget {
            protein_g_per_kg: 2.2,
            fat_ratio,
        };
        let result = calculate(&input, today).unwrap();
        assert!(result.carbohydrate_g.abs() < 1e-9);
        assert!(result.protein_g * 4.0 + result.fat_g * 9.0 <= result.target_kcal + 1e-9);
    }
}
