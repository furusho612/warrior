//! 身体情報と目的の設定 (PRD の「身体情報」「目的」「入力と保存の上限」「安全」)。
//!
//! 保存するのは最新値だけであり、身体情報を更新しても、セットを記録した時点の体重は
//! [`BodyWeightAtRecord`] が保つ。身長、体重、生年月日、性別が揃っているときだけ、
//! 体重を使う計算を行う。

use serde::{Deserialize, Serialize};

use crate::jst::JstDate;
use crate::nutrition::{self, ActivityLevel, Goal, Profile as NutritionProfile, Sex};

/// 身長の下限 (cm)。
pub const HEIGHT_MIN_CM: f64 = 100.0;
/// 身長の上限 (cm)。
pub const HEIGHT_MAX_CM: f64 = 250.0;
/// 体重の下限 (kg)。
pub const WEIGHT_MIN_KG: f64 = 20.0;
/// 体重の上限 (kg)。
pub const WEIGHT_MAX_KG: f64 = 300.0;
/// 週あたりのトレーニング可能日数の下限。
pub const TRAINING_DAYS_MIN: u32 = 0;
/// 週あたりのトレーニング可能日数の上限。
pub const TRAINING_DAYS_MAX: u32 = 7;
/// 注意書きを出す満年齢の境目。この年齢未満に出す。
pub const ADULT_AGE: u32 = 18;
/// 注意書きを出す BMI の境目。この値未満に出す。
pub const LOW_BMI: f64 = 18.5;

/// トレーニング経験。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingExperience {
    /// 初心者。
    Beginner,
    /// 中級者。
    Intermediate,
    /// 上級者。
    Advanced,
}

impl TrainingExperience {
    /// D1 と API に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            TrainingExperience::Beginner => "beginner",
            TrainingExperience::Intermediate => "intermediate",
            TrainingExperience::Advanced => "advanced",
        }
    }
}

/// 利用者の身体情報、目的、日数、活動量。未入力の項目は None で持つ。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// 身長 (cm)。
    pub height_cm: Option<f64>,
    /// 体重 (kg)。
    pub weight_kg: Option<f64>,
    /// 生年月日 (日本時間)。
    pub birth_date: Option<JstDate>,
    /// 性別。
    pub sex: Option<Sex>,
    /// トレーニング経験。
    pub training_experience: Option<TrainingExperience>,
    /// 目的。
    pub goal: Option<Goal>,
    /// 週あたりのトレーニング可能日数 (0 から 7)。
    pub training_days_per_week: Option<u32>,
    /// 日常の活動量。
    pub activity_level: Option<ActivityLevel>,
}

/// 入力の検証の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileError {
    /// 身長が 100 cm 以上 250 cm 以下でない。
    HeightOutOfRange,
    /// 体重が 20 kg 以上 300 kg 以下でない。
    WeightOutOfRange,
    /// 生年月日が未来の日付である。
    FutureBirthDate,
    /// 週あたりのトレーニング可能日数が 0 から 7 の範囲外である。
    TrainingDaysOutOfRange,
    /// 計算した基礎代謝が正の有限の値にならない。
    BasalMetabolismNotPositive,
}

/// 注意書きの要否。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SafetyNotice {
    /// 満年齢が 18 歳未満である。
    pub underage: bool,
    /// BMI が 18.5 未満である。
    pub underweight: bool,
}

impl SafetyNotice {
    /// 注意書きを出すか。
    pub fn needs_attention(self) -> bool {
        self.underage || self.underweight
    }
}

/// セットを記録した時点の体重。
///
/// 身体情報を更新しても、この値は変わらない。体重が無いときは重量を計算しない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyWeightAtRecord {
    body_weight_kg: Option<f64>,
}

impl BodyWeightAtRecord {
    /// 記録した時点の体重から作る。正の有限の値でない体重は無いものとして扱う。
    pub fn new(body_weight_kg: Option<f64>) -> Self {
        Self {
            body_weight_kg: body_weight_kg.filter(|weight| weight.is_finite() && *weight > 0.0),
        }
    }

    /// 身体情報から、セットを記録した時点の体重を取る。
    ///
    /// 4 つの身体情報が揃っていないときは、体重を使う計算をしない。
    pub fn from_profile(profile: &Profile) -> Self {
        Self::new(profile.body_profile().map(|body| body.weight_kg))
    }

    /// 記録した時点の体重。
    pub fn body_weight_kg(self) -> Option<f64> {
        self.body_weight_kg
    }

    /// 体重にセットごとの加重を足した重量。体重が無いときは None。
    pub fn set_weight_kg(self, added_weight_kg: f64) -> Option<f64> {
        self.body_weight_kg.map(|weight| weight + added_weight_kg)
    }
}

impl Profile {
    /// 体重を使う計算に必要な 4 つの身体情報。揃っていないときは None。
    pub fn body_profile(&self) -> Option<NutritionProfile> {
        Some(NutritionProfile {
            height_cm: self.height_cm?,
            weight_kg: self.weight_kg?,
            birth_date: self.birth_date?,
            sex: self.sex?,
        })
    }

    /// 指定した日本時間の日付での満年齢。生年月日が無い、または未来のときは None。
    pub fn age_on(&self, today: JstDate) -> Option<u32> {
        nutrition::age_on(self.birth_date?, today).ok()
    }

    /// BMI。身体情報が揃っていないときは None。
    ///
    /// 丸めずに [`LOW_BMI`] と比較する。
    pub fn bmi(&self) -> Option<f64> {
        let body = self.body_profile()?;
        let height_m = body.height_cm / 100.0;
        Some(body.weight_kg / (height_m * height_m))
    }

    /// 指定した日付での基礎代謝。身体情報が揃っていないときは None。
    pub fn basal_metabolism_on(&self, today: JstDate) -> Option<f64> {
        let body = self.body_profile()?;
        let age = self.age_on(today)?;
        Some(nutrition::basal_metabolism(body, age))
    }

    /// 指定した日付での注意書きの要否。
    pub fn safety_notice_on(&self, today: JstDate) -> SafetyNotice {
        SafetyNotice {
            underage: self.age_on(today).is_some_and(|age| age < ADULT_AGE),
            underweight: self.bmi().is_some_and(|bmi| bmi < LOW_BMI),
        }
    }

    /// 入力の範囲と、計算した基礎代謝が正の値になることを検証する。
    ///
    /// 未入力の項目は検証しない。4 つの身体情報が揃っているときだけ基礎代謝を計算する。
    pub fn validate(&self, today: JstDate) -> Result<(), ProfileError> {
        if let Some(height) = self.height_cm {
            if !(HEIGHT_MIN_CM..=HEIGHT_MAX_CM).contains(&height) {
                return Err(ProfileError::HeightOutOfRange);
            }
        }
        if let Some(weight) = self.weight_kg {
            if !(WEIGHT_MIN_KG..=WEIGHT_MAX_KG).contains(&weight) {
                return Err(ProfileError::WeightOutOfRange);
            }
        }
        if self.birth_date.is_some_and(|birth_date| birth_date > today) {
            return Err(ProfileError::FutureBirthDate);
        }
        if self
            .training_days_per_week
            .is_some_and(|days| !(TRAINING_DAYS_MIN..=TRAINING_DAYS_MAX).contains(&days))
        {
            return Err(ProfileError::TrainingDaysOutOfRange);
        }
        if self.body_profile().is_some() {
            let basal_metabolism = self
                .basal_metabolism_on(today)
                .ok_or(ProfileError::FutureBirthDate)?;
            if !basal_metabolism.is_finite() || basal_metabolism <= 0.0 {
                return Err(ProfileError::BasalMetabolismNotPositive);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! 身体情報と目的の設定の例示テスト。
    //!
    //! 対応する要求: `REQ-PRF-001`、`REQ-PRF-003`、`REQ-PRF-004`、`REQ-PRF-005`、
    //! `REQ-NFR-018`、`REQ-NFR-019`。保存の置き換えは結合テストと性質テストが担う。

    use serde_json::json;

    use super::{
        BodyWeightAtRecord, Profile, ProfileError, SafetyNotice, TrainingExperience, ADULT_AGE,
        HEIGHT_MAX_CM, HEIGHT_MIN_CM, LOW_BMI, WEIGHT_MAX_KG, WEIGHT_MIN_KG,
    };
    use crate::jst::JstDate;
    use crate::nutrition::{ActivityLevel, Goal, Sex};

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }

    fn today() -> JstDate {
        date(2026, 11, 12)
    }

    fn profile() -> Profile {
        Profile {
            height_cm: Some(175.0),
            weight_kg: Some(70.0),
            birth_date: Some(date(1990, 5, 10)),
            sex: Some(Sex::Male),
            training_experience: Some(TrainingExperience::Intermediate),
            goal: Some(Goal::Hypertrophy),
            training_days_per_week: Some(4),
            activity_level: Some(ActivityLevel::Normal),
        }
    }

    #[test]
    fn req_prf_001_accepts_the_body_info_at_the_boundaries() {
        for (height, weight) in [
            (HEIGHT_MIN_CM, WEIGHT_MIN_KG),
            (HEIGHT_MAX_CM, WEIGHT_MAX_KG),
            (175.0, 70.0),
        ] {
            let mut input = profile();
            input.height_cm = Some(height);
            input.weight_kg = Some(weight);
            assert_eq!(input.validate(today()), Ok(()));
        }
    }

    #[test]
    fn req_prf_001_rejects_the_body_info_outside_the_boundaries() {
        for height in [HEIGHT_MIN_CM - 0.1, HEIGHT_MAX_CM + 0.1, f64::NAN] {
            let mut input = profile();
            input.height_cm = Some(height);
            assert_eq!(input.validate(today()), Err(ProfileError::HeightOutOfRange));
        }
        for weight in [WEIGHT_MIN_KG - 0.1, WEIGHT_MAX_KG + 0.1, f64::NAN] {
            let mut input = profile();
            input.weight_kg = Some(weight);
            assert_eq!(input.validate(today()), Err(ProfileError::WeightOutOfRange));
        }
    }

    #[test]
    fn req_prf_001_rejects_a_future_birth_date() {
        let mut input = profile();
        input.birth_date = Some(date(2026, 11, 13));
        assert_eq!(input.validate(today()), Err(ProfileError::FutureBirthDate));

        input.birth_date = Some(today());
        assert_eq!(input.validate(today()), Ok(()));
    }

    #[test]
    fn req_prf_001_rejects_a_basal_metabolism_that_is_not_positive() {
        let mut input = profile();
        input.height_cm = Some(HEIGHT_MIN_CM);
        input.weight_kg = Some(WEIGHT_MIN_KG);
        input.birth_date = Some(date(1000, 1, 1));
        assert_eq!(
            input.validate(today()),
            Err(ProfileError::BasalMetabolismNotPositive)
        );
    }

    #[test]
    fn req_prf_001_computes_the_age_on_the_japan_date() {
        let mut input = profile();
        input.birth_date = Some(date(2008, 11, 12));
        assert_eq!(input.age_on(date(2026, 11, 11)), Some(ADULT_AGE - 1));
        assert_eq!(input.age_on(date(2026, 11, 12)), Some(ADULT_AGE));
    }

    #[test]
    fn req_prf_001_accepts_each_sex_and_experience() {
        for sex in [Sex::Male, Sex::Female, Sex::Other] {
            let mut input = profile();
            input.sex = Some(sex);
            assert_eq!(input.validate(today()), Ok(()));
        }
        for experience in [
            TrainingExperience::Beginner,
            TrainingExperience::Intermediate,
            TrainingExperience::Advanced,
        ] {
            let mut input = profile();
            input.training_experience = Some(experience);
            assert_eq!(input.validate(today()), Ok(()));
        }
    }

    #[test]
    fn req_prf_003_accepts_each_goal() {
        for goal in [
            Goal::Hypertrophy,
            Goal::Diet,
            Goal::BodyMake,
            Goal::Strength,
            Goal::Performance,
        ] {
            let mut input = profile();
            input.goal = Some(goal);
            assert_eq!(input.validate(today()), Ok(()));
        }
    }

    #[test]
    fn req_prf_004_accepts_the_training_days_at_the_boundaries() {
        for days in [0, 7] {
            let mut input = profile();
            input.training_days_per_week = Some(days);
            assert_eq!(input.validate(today()), Ok(()));
        }
        let mut input = profile();
        input.training_days_per_week = Some(8);
        assert_eq!(
            input.validate(today()),
            Err(ProfileError::TrainingDaysOutOfRange)
        );
    }

    #[test]
    fn req_nfr_018_reports_underage_on_the_day_before_the_eighteenth_birthday() {
        let mut input = profile();
        input.birth_date = Some(date(2008, 11, 12));
        assert_eq!(
            input.safety_notice_on(date(2026, 11, 11)),
            SafetyNotice {
                underage: true,
                underweight: false
            }
        );
        assert_eq!(
            input.safety_notice_on(date(2026, 11, 12)),
            SafetyNotice::default()
        );
    }

    #[test]
    fn req_nfr_018_compares_the_unrounded_bmi_with_the_boundary() {
        let mut input = profile();
        input.height_cm = Some(200.0);
        input.weight_kg = Some(74.0);
        assert_eq!(input.bmi(), Some(LOW_BMI));
        assert_eq!(
            input.safety_notice_on(today()),
            SafetyNotice {
                underage: false,
                underweight: false
            }
        );

        input.weight_kg = Some(73.9);
        assert_eq!(
            input.safety_notice_on(today()),
            SafetyNotice {
                underage: false,
                underweight: true
            }
        );

        input.weight_kg = Some(74.1);
        assert_eq!(input.safety_notice_on(today()), SafetyNotice::default());
    }

    #[test]
    fn req_prf_005_body_weight_calculations_need_the_four_body_fields() {
        for field in 0..4 {
            let mut input = profile();
            match field {
                0 => input.height_cm = None,
                1 => input.weight_kg = None,
                2 => input.birth_date = None,
                _ => input.sex = None,
            }
            assert_eq!(input.body_profile(), None);
            assert_eq!(input.bmi(), None);
            assert_eq!(input.basal_metabolism_on(today()), None);
            assert_eq!(input.safety_notice_on(today()), SafetyNotice::default());
            assert_eq!(
                BodyWeightAtRecord::from_profile(&input).set_weight_kg(10.0),
                None
            );
            assert_eq!(input.validate(today()), Ok(()));
        }
    }

    #[test]
    fn req_prf_005_bodyweight_set_weight_adds_the_loaded_weight() {
        let input = profile();
        let recorded = BodyWeightAtRecord::from_profile(&input);
        assert_eq!(recorded.body_weight_kg(), Some(70.0));
        assert_eq!(recorded.set_weight_kg(10.0), Some(80.0));
        assert_eq!(recorded.set_weight_kg(0.0), Some(70.0));
        assert_eq!(BodyWeightAtRecord::new(None).set_weight_kg(10.0), None);
        assert_eq!(BodyWeightAtRecord::new(Some(0.0)).set_weight_kg(10.0), None);
    }

    #[test]
    fn req_prf_005_updating_the_profile_keeps_the_recorded_body_weight() {
        let input = profile();
        let recorded = BodyWeightAtRecord::from_profile(&input);
        let mut updated = input.clone();
        updated.weight_kg = Some(80.0);
        assert_eq!(recorded.body_weight_kg(), Some(70.0));
        assert_eq!(recorded.set_weight_kg(10.0), Some(80.0));
        assert_eq!(
            BodyWeightAtRecord::from_profile(&updated).set_weight_kg(10.0),
            Some(90.0)
        );
    }

    #[test]
    fn req_prf_001_serializes_the_profile_with_snake_case_values() {
        let value = serde_json::to_value(profile()).expect("serializable profile");
        assert_eq!(
            value,
            json!({
                "height_cm": 175.0,
                "weight_kg": 70.0,
                "birth_date": "1990-05-10",
                "sex": "male",
                "training_experience": "intermediate",
                "goal": "hypertrophy",
                "training_days_per_week": 4,
                "activity_level": "normal"
            })
        );
        let restored: Profile = serde_json::from_value(value).expect("deserializable profile");
        assert_eq!(restored, profile());
    }
}
