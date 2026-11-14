//! 身体情報と目的の設定の、画面が共有する型、API の呼び出し、選択肢の文言。

use warrior_core::jst::{JstDate, UtcInstant};
use warrior_core::nutrition::{ActivityLevel, Goal, Sex};
use warrior_core::profile::{Profile, ProfileError, TrainingExperience};

use crate::http::{self, HttpError};
use crate::text::Key;

/// 保存済みのプロフィールを読む。未登録は None。
pub async fn fetch() -> Result<Option<Profile>, HttpError> {
    http::request_json("GET", "/api/profile").await
}

/// プロフィールを保存し、保存した値を受け取る。
pub async fn save(profile: &Profile) -> Result<Profile, HttpError> {
    http::request_json_with_body("PUT", "/api/profile", profile).await
}

/// 日本時間の今日。ブラウザの時計が読めないときは None。
pub fn today() -> Option<JstDate> {
    let seconds = (js_sys::Date::now() / 1000.0) as i64;
    UtcInstant::from_epoch_seconds(seconds).jst_date()
}

/// 入力の検証。検証に失敗したら画面の文言のキーを返す。
pub fn validate(profile: &Profile) -> Result<(), Key> {
    let today = today().ok_or(Key::ProfileSaveFailed)?;
    profile.validate(today).map_err(error_key)
}

fn error_key(error: ProfileError) -> Key {
    match error {
        ProfileError::HeightOutOfRange => Key::ProfileInvalidHeight,
        ProfileError::WeightOutOfRange => Key::ProfileInvalidWeight,
        ProfileError::FutureBirthDate => Key::ProfileInvalidBirthDate,
        ProfileError::TrainingDaysOutOfRange => Key::ProfileInvalidTrainingDays,
        ProfileError::BasalMetabolismNotPositive => Key::ProfileInvalidBasalMetabolism,
    }
}

/// 画面の入力値。空の入力は None として持つ。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProfileForm {
    /// 身長の入力。
    pub height_cm: String,
    /// 体重の入力。
    pub weight_kg: String,
    /// 生年月日の入力。
    pub birth_date: String,
    /// 性別の選択。
    pub sex: Option<Sex>,
    /// トレーニング経験の選択。
    pub training_experience: Option<TrainingExperience>,
    /// 目的の選択。
    pub goal: Option<Goal>,
    /// 週あたりのトレーニング可能日数の選択。
    pub training_days_per_week: Option<u32>,
    /// 日常の活動量の選択。
    pub activity_level: Option<ActivityLevel>,
}

impl ProfileForm {
    /// 保存済みのプロフィールから入力値を組み立てる。
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            height_cm: number_text(profile.height_cm),
            weight_kg: number_text(profile.weight_kg),
            birth_date: profile
                .birth_date
                .map(|date| date.to_string())
                .unwrap_or_default(),
            sex: profile.sex,
            training_experience: profile.training_experience,
            goal: profile.goal,
            training_days_per_week: profile.training_days_per_week,
            activity_level: profile.activity_level,
        }
    }

    /// 入力値をプロフィールに変換する。数値と日付の形式だけをここで確かめる。
    pub fn to_profile(&self) -> Result<Profile, Key> {
        Ok(Profile {
            height_cm: parse_number(&self.height_cm, Key::ProfileInvalidHeight)?,
            weight_kg: parse_number(&self.weight_kg, Key::ProfileInvalidWeight)?,
            birth_date: parse_date(&self.birth_date)?,
            sex: self.sex,
            training_experience: self.training_experience,
            goal: self.goal,
            training_days_per_week: self.training_days_per_week,
            activity_level: self.activity_level,
        })
    }
}

fn number_text(value: Option<f64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn parse_number(text: &str, key: Key) -> Result<Option<f64>, Key> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<f64>().map(Some).map_err(|_| key)
}

fn parse_date(text: &str) -> Result<Option<JstDate>, Key> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    JstDate::parse(text)
        .map(Some)
        .ok_or(Key::ProfileInvalidBirthDate)
}

/// 性別の選択肢。
pub fn sex_options() -> [(Sex, Key); 3] {
    [
        (Sex::Male, Key::ProfileSexMale),
        (Sex::Female, Key::ProfileSexFemale),
        (Sex::Other, Key::ProfileSexOther),
    ]
}

/// トレーニング経験の選択肢。
pub fn experience_options() -> [(TrainingExperience, Key); 3] {
    [
        (TrainingExperience::Beginner, Key::ProfileExperienceBeginner),
        (
            TrainingExperience::Intermediate,
            Key::ProfileExperienceIntermediate,
        ),
        (TrainingExperience::Advanced, Key::ProfileExperienceAdvanced),
    ]
}

/// 目的の選択肢。
pub fn goal_options() -> [(Goal, Key); 5] {
    [
        (Goal::Hypertrophy, Key::ProfileGoalHypertrophy),
        (Goal::Diet, Key::ProfileGoalDiet),
        (Goal::BodyMake, Key::ProfileGoalBodyMake),
        (Goal::Strength, Key::ProfileGoalStrength),
        (Goal::Performance, Key::ProfileGoalPerformance),
    ]
}

/// 日常の活動量の選択肢。
pub fn activity_options() -> [(ActivityLevel, Key); 3] {
    [
        (ActivityLevel::Low, Key::ProfileActivityLow),
        (ActivityLevel::Normal, Key::ProfileActivityNormal),
        (ActivityLevel::High, Key::ProfileActivityHigh),
    ]
}

/// 選択肢のボタンの class。
pub fn choice_class(selected: bool) -> &'static str {
    if selected {
        "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary"
    } else {
        "min-h-11 border border-line px-3 text-[13px]"
    }
}

/// `aria-pressed` の値。
pub fn pressed(selected: bool) -> &'static str {
    if selected {
        "true"
    } else {
        "false"
    }
}
