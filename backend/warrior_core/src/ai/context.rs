//! AI に渡すコンテキスト (ADR-010)。
//!
//! アカウントの識別子、メールアドレス、端末の識別子、プッシュ購読の情報を持たない。
//! 生年月日は年齢に変換し、生年月日そのものを持たない。

use std::collections::HashSet;

use serde::Serialize;

use crate::jst::JstDate;
use crate::nutrition::{Goal, Sex};
use crate::set_calc::{RecordMode, SetKind};

pub use crate::goal::ExerciseGoal;

/// 部位区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PartCategory {
    /// 胸。
    Chest,
    /// 背中。
    Back,
    /// 肩。
    Shoulders,
    /// 二頭・前腕。
    BicepsForearm,
    /// 三頭。
    Triceps,
    /// 脚。
    Legs,
    /// 腹筋。
    Abs,
    /// その他。
    Other,
}

/// トレーニング経験。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingExperience {
    /// 初心者。
    Beginner,
    /// 中級者。
    Intermediate,
    /// 上級者。
    Advanced,
}

/// AI に渡す身体情報。生年月日は含めず、年齢だけを持つ。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AiProfile {
    /// 身長 (cm)。
    pub height_cm: f64,
    /// 体重 (kg)。
    pub weight_kg: f64,
    /// 満年齢。
    pub age: u32,
    /// 性別。
    pub sex: Sex,
}

impl AiProfile {
    /// 基礎代謝。目標カロリーの下限の判定に使う。
    pub fn basal_metabolism_kcal(self) -> f64 {
        10.0 * self.weight_kg + 6.25 * self.height_cm - 5.0 * f64::from(self.age)
            + self.sex.adjustment()
    }
}

/// コンテキストのセット。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextSet {
    /// セット種別。
    pub kind: SetKind,
    /// 重量 (kg)。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 時間 (秒)。
    pub duration_seconds: Option<u32>,
    /// RPE。
    pub rpe: Option<f64>,
    /// 反動の回数。
    pub momentum_reps: u32,
    /// 補助の回数。
    pub assisted_reps: u32,
}

/// コンテキストの種目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextExercise {
    /// 種目 ID。
    pub exercise_id: String,
    /// 種目名。
    pub name: String,
    /// グリップ。
    pub grip: Option<String>,
    /// アタッチメント。
    pub attachment: Option<String>,
    /// 記録方式。
    pub mode: RecordMode,
    /// 種目のメモ。
    pub memo: Option<String>,
    /// セット。
    pub sets: Vec<ContextSet>,
}

/// コンテキストのトレーニング。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContextTraining {
    /// トレーニングの日付 (日本時間)。
    pub date: JstDate,
    /// トレーニングのメモ。
    pub memo: Option<String>,
    /// 種目。
    pub exercises: Vec<ContextExercise>,
}

/// 部位別の分析。画面に出すセット数を持つ。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PartAnalysis {
    /// 部位区分。
    pub part: PartCategory,
    /// 画面のセット数。ウォームアップを除き、ドロップセットを 1 と数える。
    pub sets: f64,
    /// 総重量 (kg)。
    pub total_weight_kg: f64,
}

/// 補助筋を含めたセット数の参考値。AI にだけ渡す。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PartSetReference {
    /// 部位区分。
    pub part: PartCategory,
    /// 補助筋のセットを 0.5 として加えた値。
    pub sets_with_assisted: f64,
}

/// 回復状況。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPhase {
    /// 回復中。
    Recovering,
    /// ほぼ回復。
    AlmostRecovered,
    /// 回復済み。
    Recovered,
}

/// 部位の回復状況。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RecoveryStatus {
    /// 部位区分。
    pub part: PartCategory,
    /// 回復状況。
    pub phase: RecoveryPhase,
}

/// 現在のルーティンの 1 項目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutineItem {
    /// 種目 ID。
    pub exercise_id: String,
    /// セット数。
    pub sets: u32,
    /// 目標回数。
    pub reps: u32,
    /// 目標重量 (kg)。
    pub weight_kg: f64,
    /// レスト時間 (秒)。
    pub rest_seconds: u32,
}

/// 提案に使える種目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExerciseSummary {
    /// 種目 ID。
    pub exercise_id: String,
    /// 種目名。
    pub name: String,
    /// 主働筋の部位。
    pub primary_part: PartCategory,
    /// 補助筋の部位。
    pub secondary_parts: Vec<PartCategory>,
}

/// AI に渡すコンテキスト。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AiContext {
    /// 目的。
    pub goal: Goal,
    /// 身体情報。
    pub profile: AiProfile,
    /// トレーニング経験。
    pub experience: TrainingExperience,
    /// 週あたりのトレーニング可能日数。
    pub training_days_per_week: u32,
    /// 対象期間のトレーニング記録。
    pub records: Vec<ContextTraining>,
    /// 種目ごとの目標重量。
    pub goals: Vec<ExerciseGoal>,
    /// 部位別の分析。
    pub part_analysis: Vec<PartAnalysis>,
    /// 補助筋を含めた参考値。AI にだけ渡す。
    pub assisted_set_reference: Vec<PartSetReference>,
    /// 回復状況。
    pub recovery: Vec<RecoveryStatus>,
    /// 現在のルーティン。
    pub routine: Vec<RoutineItem>,
    /// 提案に使える種目の一覧。
    pub exercises: Vec<ExerciseSummary>,
}

impl AiContext {
    /// 画面のセット数と補助筋を含む参考値が食い違うか。
    pub fn reference_mismatch(&self) -> bool {
        self.assisted_set_reference.iter().any(|reference| {
            let screen = self
                .part_analysis
                .iter()
                .find(|analysis| analysis.part == reference.part)
                .map_or(0.0, |analysis| analysis.sets);
            (screen - reference.sets_with_assisted).abs() > 1e-9
        })
    }

    /// 痛みやけがの記述がある種目。トレーニングのメモはその日の全種目に及ぶ。
    pub fn safety_exercise_ids(&self) -> HashSet<String> {
        let mut affected = HashSet::new();
        for training in &self.records {
            let training_affects_all = training
                .memo
                .as_deref()
                .is_some_and(super::safety::memo_has_safety_term);
            for exercise in &training.exercises {
                let exercise_affected = exercise
                    .memo
                    .as_deref()
                    .is_some_and(super::safety::memo_has_safety_term);
                if training_affects_all || exercise_affected {
                    affected.insert(exercise.exercise_id.clone());
                }
            }
        }
        affected
    }

    /// コンテキストに現れる数値。根拠の数値の照合に使う。
    pub fn known_numbers(&self) -> HashSet<String> {
        let value = serde_json::to_value(self).unwrap_or(serde_json::Value::Null);
        let mut numbers = HashSet::new();
        collect_numbers(&value, &mut numbers);
        numbers
    }

    /// メモを無害化したコンテキスト。
    pub fn sanitized(&self) -> Self {
        let mut sanitized = self.clone();
        for training in &mut sanitized.records {
            training.memo = training.memo.as_deref().map(super::prompt::sanitize_memo);
            for exercise in &mut training.exercises {
                exercise.memo = exercise.memo.as_deref().map(super::prompt::sanitize_memo);
            }
        }
        sanitized
    }
}

fn collect_numbers(value: &serde_json::Value, numbers: &mut HashSet<String>) {
    match value {
        serde_json::Value::Number(number) => {
            numbers.insert(normalize_number(number.as_f64().unwrap_or_default()));
        }
        serde_json::Value::String(text) => {
            for token in numeric_tokens(text) {
                numbers.insert(token);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_numbers(value, numbers);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                collect_numbers(value, numbers);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) => {}
    }
}

/// 文字列に含まれる数値を正規化した形で返す。
pub fn numeric_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() || character == '.' {
            current.push(character);
        } else if !current.is_empty() {
            if let Ok(value) = current.parse::<f64>() {
                tokens.push(normalize_number(value));
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Ok(value) = current.parse::<f64>() {
            tokens.push(normalize_number(value));
        }
    }
    tokens
}

fn normalize_number(value: f64) -> String {
    format!("{value:.6}")
}
