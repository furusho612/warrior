//! セットの計算規則 (PRD の「計算ルールと用語定義」「セット種別ごとの扱い」)。
//!
//! 推定 1RM、総重量、ストリクト回数、部位別のセット数、自己ベストの判定をここに集める。
//! 記録、分析、AI、CSV はこのモジュールの関数だけを使い、計算を再実装しない。

use serde::{Deserialize, Serialize};

use crate::jst::JstDate;

/// セットの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    /// 通常のセット。
    Normal,
    /// ウォームアップセット。総重量と自己ベストの対象外である。
    Warmup,
    /// ドロップセット。総重量は全段、最大重量と推定 1RM は最初の段だけを対象にする。
    Drop,
}

impl SetKind {
    /// DB と API の文字列。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warmup => "warmup",
            Self::Drop => "drop",
        }
    }

    /// DB の文字列からセット種別に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "normal" => Some(Self::Normal),
            "warmup" => Some(Self::Warmup),
            "drop" => Some(Self::Drop),
            _ => None,
        }
    }
}

/// 左右別の種目で使う側。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// 左。
    Left,
    /// 右。
    Right,
}

/// セットの記録方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordMode {
    /// 重量と回数で記録する。
    WeightAndReps,
    /// 自重と回数で記録する。重量は記録時点の体重に加重を足した値とする。
    BodyweightAndReps,
    /// 時間で記録する。総重量と推定 1RM の対象外である。
    Time,
}

/// 左右別の値。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SideValue {
    /// 重量 (kg)。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
}

impl SideValue {
    /// ストリクト回数。
    pub fn strict_reps(&self, momentum_reps: u32, assisted_reps: u32) -> u32 {
        strict_reps(self.reps, momentum_reps, assisted_reps)
    }

    /// 総重量。補助の回数を除き、反動の回数を含める。
    pub fn total_weight_kg(&self, assisted_reps: u32) -> f64 {
        self.weight_kg * f64::from(self.reps.saturating_sub(assisted_reps))
    }

    /// ストリクト回数だけで計算した総重量。
    pub fn strict_total_weight_kg(&self, momentum_reps: u32, assisted_reps: u32) -> f64 {
        self.weight_kg * f64::from(self.strict_reps(momentum_reps, assisted_reps))
    }

    /// 時間 (秒)。
    pub fn duration_seconds(&self) -> u32 {
        self.duration_seconds.unwrap_or(0)
    }
}

/// 推定 1RM と、10 回を超えるセットの参考値の印。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OneRepMax {
    /// 推定 1RM (kg)。
    pub value_kg: f64,
    /// 10 回を超えるセットの参考値か。
    pub is_reference: bool,
}

/// セットの 1 つの段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSegment {
    /// 重量 (kg)。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 反動を使った回数。
    pub momentum_reps: u32,
    /// 補助を受けた回数。
    pub assisted_reps: u32,
    /// 左の値。左右別の種目だけが持つ。
    pub left: Option<SideValue>,
    /// 右の値。左右別の種目だけが持つ。
    pub right: Option<SideValue>,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
}

impl SetSegment {
    /// ストリクト回数。0 を下回らない。
    pub fn strict_reps(&self) -> u32 {
        strict_reps(self.reps, self.momentum_reps, self.assisted_reps)
    }

    /// 左右別の値を引く。
    pub fn side(&self, side: Side) -> Option<&SideValue> {
        match side {
            Side::Left => self.left.as_ref(),
            Side::Right => self.right.as_ref(),
        }
    }

    /// 総重量。左右別なら左右を合算する。
    pub fn total_weight_kg(&self) -> f64 {
        self.side_total_weight_kg(self.assisted_reps)
    }

    /// ストリクト回数だけで計算した総重量。
    pub fn strict_total_weight_kg(&self) -> f64 {
        if self.has_sides() {
            self.left.as_ref().map_or(0.0, |side| {
                side.strict_total_weight_kg(self.momentum_reps, self.assisted_reps)
            }) + self.right.as_ref().map_or(0.0, |side| {
                side.strict_total_weight_kg(self.momentum_reps, self.assisted_reps)
            })
        } else {
            self.weight_kg * f64::from(self.strict_reps())
        }
    }

    /// 最大重量。左右別なら左右の大きい方。
    pub fn max_weight_kg(&self) -> Option<f64> {
        if self.has_sides() {
            [self.left.as_ref(), self.right.as_ref()]
                .into_iter()
                .flatten()
                .map(|side| side.weight_kg)
                .filter(|weight| *weight > 0.0)
                .reduce(f64::max)
        } else if self.weight_kg > 0.0 {
            Some(self.weight_kg)
        } else {
            None
        }
    }

    /// 推定 1RM と参考値の印。左右別なら左右の推定 1RM の大きい方。
    pub fn one_rep_max(&self) -> Option<OneRepMax> {
        if self.has_sides() {
            [self.left.as_ref(), self.right.as_ref()]
                .into_iter()
                .flatten()
                .filter_map(|side| {
                    let strict = side.strict_reps(self.momentum_reps, self.assisted_reps);
                    estimated_one_rep_max(side.weight_kg, strict).map(|value_kg| OneRepMax {
                        value_kg,
                        is_reference: is_reference_one_rep_max(strict),
                    })
                })
                .max_by(|first, second| first.value_kg.total_cmp(&second.value_kg))
        } else {
            let strict = self.strict_reps();
            estimated_one_rep_max(self.weight_kg, strict).map(|value_kg| OneRepMax {
                value_kg,
                is_reference: is_reference_one_rep_max(strict),
            })
        }
    }

    /// 推定 1RM。左右別なら左右の推定 1RM の大きい方。
    pub fn estimated_one_rep_max(&self) -> Option<f64> {
        self.one_rep_max().map(|one_rep_max| one_rep_max.value_kg)
    }

    /// 時間で記録する段の最長時間。左右別なら左右の大きい方。
    pub fn longest_duration_seconds(&self) -> u32 {
        if self.has_sides() {
            [self.left.as_ref(), self.right.as_ref()]
                .into_iter()
                .flatten()
                .map(SideValue::duration_seconds)
                .max()
                .unwrap_or(0)
        } else {
            self.duration_seconds.unwrap_or(0)
        }
    }

    /// 時間で記録する段の合計時間。左右別なら左右の和。
    pub fn total_duration_seconds(&self) -> u32 {
        if self.has_sides() {
            [self.left.as_ref(), self.right.as_ref()]
                .into_iter()
                .flatten()
                .map(SideValue::duration_seconds)
                .sum()
        } else {
            self.duration_seconds.unwrap_or(0)
        }
    }

    fn side_total_weight_kg(&self, assisted_reps: u32) -> f64 {
        if self.has_sides() {
            self.left
                .as_ref()
                .map_or(0.0, |side| side.total_weight_kg(assisted_reps))
                + self
                    .right
                    .as_ref()
                    .map_or(0.0, |side| side.total_weight_kg(assisted_reps))
        } else {
            self.weight_kg * f64::from(self.reps.saturating_sub(assisted_reps))
        }
    }

    fn has_sides(&self) -> bool {
        self.left.is_some() || self.right.is_some()
    }
}

/// 自己ベストの比較の単位。種目とグリップとアタッチメントの組で分ける。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BestKey {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。選ばない種目は None。
    pub grip: Option<String>,
    /// アタッチメント。選ばない種目は None。
    pub attachment: Option<String>,
}

/// セットの記録。
#[derive(Debug, Clone, PartialEq)]
pub struct SetRecord {
    key: BestKey,
    kind: SetKind,
    segments: Vec<SetSegment>,
    rpe: Option<f64>,
    duration_seconds: Option<u32>,
    mode: RecordMode,
}

impl SetRecord {
    /// 検証してセットの記録を作る。
    pub fn new(
        key: BestKey,
        kind: SetKind,
        segments: Vec<SetSegment>,
        rpe: Option<f64>,
        duration_seconds: Option<u32>,
        mode: RecordMode,
    ) -> Result<Self, SetValidationError> {
        validate_rpe(rpe)?;
        match mode {
            RecordMode::Time => validate_time(&segments, duration_seconds)?,
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                if segments.is_empty() {
                    return Err(SetValidationError::EmptySegments);
                }
                for segment in &segments {
                    validate_weight_segment(segment)?;
                }
            }
        }
        Ok(Self {
            key,
            kind,
            segments,
            rpe,
            duration_seconds,
            mode,
        })
    }

    /// 自己ベストの比較の単位。
    pub fn key(&self) -> &BestKey {
        &self.key
    }

    /// セット種別。
    pub fn kind(&self) -> SetKind {
        self.kind
    }

    /// 段の列。
    pub fn segments(&self) -> &[SetSegment] {
        &self.segments
    }

    /// RPE。
    pub fn rpe(&self) -> Option<f64> {
        self.rpe
    }

    /// 時間で記録するセットの秒数。
    pub fn duration_seconds(&self) -> Option<u32> {
        self.duration_seconds
    }

    /// 記録方式。
    pub fn mode(&self) -> RecordMode {
        self.mode
    }

    /// 総重量。ウォームアップと時間で記録する種目は 0。
    pub fn total_weight_kg(&self) -> f64 {
        if self.kind == SetKind::Warmup || self.mode == RecordMode::Time {
            return 0.0;
        }
        self.segments.iter().map(SetSegment::total_weight_kg).sum()
    }

    /// ストリクト回数だけで計算した総重量。自己ベストの種目別の総重量に使う。
    pub fn strict_total_weight_kg(&self) -> f64 {
        if self.kind == SetKind::Warmup || self.mode == RecordMode::Time {
            return 0.0;
        }
        self.segments
            .iter()
            .map(SetSegment::strict_total_weight_kg)
            .sum()
    }

    /// 最大重量。ウォームアップと時間で記録する種目は None。ドロップセットは最初の段だけ。
    pub fn max_weight_kg(&self) -> Option<f64> {
        if self.kind == SetKind::Warmup || self.mode == RecordMode::Time {
            return None;
        }
        self.segments.first().and_then(SetSegment::max_weight_kg)
    }

    /// 推定 1RM と参考値の印。ウォームアップと時間で記録する種目は None。ドロップセットは最初の段だけ。
    pub fn one_rep_max(&self) -> Option<OneRepMax> {
        if self.kind == SetKind::Warmup || self.mode == RecordMode::Time {
            return None;
        }
        self.segments.first().and_then(SetSegment::one_rep_max)
    }

    /// 推定 1RM。ウォームアップと時間で記録する種目は None。ドロップセットは最初の段だけ。
    pub fn estimated_one_rep_max(&self) -> Option<f64> {
        self.one_rep_max().map(|one_rep_max| one_rep_max.value_kg)
    }

    /// 時間で記録するセットの最長時間。左右別なら左右の大きい方。
    pub fn longest_duration_seconds(&self) -> Option<u32> {
        if self.mode != RecordMode::Time {
            return None;
        }
        let segments = self
            .segments
            .iter()
            .map(SetSegment::longest_duration_seconds)
            .max()
            .unwrap_or(0);
        let longest = self.duration_seconds.unwrap_or(0).max(segments);
        (longest > 0).then_some(longest)
    }

    /// 時間で記録するセットの合計時間。左右別なら左右の和。
    pub fn total_duration_seconds(&self) -> u32 {
        if self.mode != RecordMode::Time {
            return 0;
        }
        let segments = self
            .segments
            .iter()
            .map(SetSegment::total_duration_seconds)
            .sum::<u32>();
        self.duration_seconds.unwrap_or(0) + segments
    }

    /// 部位別のセット数。ウォームアップは 0、通常とドロップ、時間は 1。
    pub fn counted_sets(&self) -> f64 {
        if self.kind == SetKind::Warmup {
            0.0
        } else {
            1.0
        }
    }

    /// 部位別のセット数。補助筋は AI の参考値に限り 0.5 と数える。
    pub fn part_set_count(&self, role: PartRole) -> f64 {
        match role {
            PartRole::Primary => self.counted_sets(),
            PartRole::Assisted => self.counted_sets() * 0.5,
            PartRole::Unrelated => 0.0,
        }
    }
}

/// 種目にとっての部位の役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartRole {
    /// 主働筋の部位。
    Primary,
    /// 補助筋の部位。
    Assisted,
    /// 関係しない部位。
    Unrelated,
}

/// セットの入力の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetValidationError {
    /// 重量と回数のセットに段が無い。
    EmptySegments,
    /// 反動と補助の回数の合計が回数を超えている。
    TooManyAssistedReps,
    /// 重量が負、または有限でない。
    InvalidWeight,
    /// 回数が 0 である。
    InvalidReps,
    /// RPE が 6 から 10 の 0.5 刻みでない。
    InvalidRpe,
    /// 時間が 1 秒から 86400 秒の範囲外である。
    InvalidDuration,
    /// 時間のセットに時間が無い。
    MissingDuration,
    /// 時間を記録するセットの親と段の両方に時間がある。
    ConflictingDuration,
    /// 左右別の段で、左右の片方だけに値がある。
    MissingSide,
}

/// ストリクト回数。回数から反動と補助を引き、0 を下回らない。
pub fn strict_reps(reps: u32, momentum_reps: u32, assisted_reps: u32) -> u32 {
    reps.saturating_sub(momentum_reps)
        .saturating_sub(assisted_reps)
}

/// 推定 1RM。重量が 0 以下、またはストリクト回数が 0 のときは None。
pub fn estimated_one_rep_max(weight_kg: f64, strict_reps: u32) -> Option<f64> {
    if weight_kg <= 0.0 || strict_reps == 0 || !weight_kg.is_finite() {
        return None;
    }
    if strict_reps == 1 {
        return Some(weight_kg);
    }
    let denominator = 48.8 + 53.8 * (-0.075 * f64::from(strict_reps)).exp();
    let value = 100.0 * weight_kg / denominator;
    value.is_finite().then_some(value)
}

/// 推定 1RM が参考値になる回数か。10 回を超えるセットは参考値とする。
pub fn is_reference_one_rep_max(strict_reps: u32) -> bool {
    strict_reps > 10
}

/// 自己ベストの 1 つの値と達成日。
#[derive(Debug, Clone, PartialEq)]
pub struct BestValue<T> {
    /// 値。
    pub value: T,
    /// 達成日 (日本時間)。
    pub achieved_on: JstDate,
}

/// 自己ベストの更新の有無。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BestUpdates {
    /// 最大重量を更新した。
    pub max_weight: bool,
    /// 推定 1RM を更新した。
    pub estimated_one_rep_max: bool,
    /// 種目別の総重量を更新した。
    pub total_weight: bool,
    /// 時間で記録する種目の最長時間を更新した。
    pub longest_duration: bool,
}

/// 1 つの比較の単位の自己ベスト。
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalBest {
    key: BestKey,
    max_weight: Option<BestValue<f64>>,
    estimated_one_rep_max: Option<BestValue<f64>>,
    total_weight: Option<BestValue<f64>>,
    longest_duration: Option<BestValue<u32>>,
}

impl PersonalBest {
    /// 比較の単位の自己ベストを作る。
    pub fn new(key: BestKey) -> Self {
        Self {
            key,
            max_weight: None,
            estimated_one_rep_max: None,
            total_weight: None,
            longest_duration: None,
        }
    }

    /// 比較の単位。
    pub fn key(&self) -> &BestKey {
        &self.key
    }

    /// 最大重量の自己ベスト。
    pub fn max_weight(&self) -> Option<&BestValue<f64>> {
        self.max_weight.as_ref()
    }

    /// 推定 1RM の自己ベスト。
    pub fn estimated_one_rep_max(&self) -> Option<&BestValue<f64>> {
        self.estimated_one_rep_max.as_ref()
    }

    /// 種目別の総重量の自己ベスト。
    pub fn total_weight(&self) -> Option<&BestValue<f64>> {
        self.total_weight.as_ref()
    }

    /// 時間で記録する種目の最長時間の自己ベスト。
    pub fn longest_duration(&self) -> Option<&BestValue<u32>> {
        self.longest_duration.as_ref()
    }

    /// セットを完了した時点の自己ベストを判定し、更新した項目を返す。
    ///
    /// ウォームアップセットは、時間で記録する種目でも自己ベストの対象にしない。
    pub fn update(&mut self, record: &SetRecord, achieved_on: JstDate) -> BestUpdates {
        if record.key() != &self.key {
            return BestUpdates::default();
        }
        if record.kind() == SetKind::Warmup {
            return BestUpdates::default();
        }

        let mut updates = BestUpdates::default();
        if let Some(value) = record.max_weight_kg() {
            if self
                .max_weight
                .as_ref()
                .is_none_or(|best| value > best.value)
            {
                self.max_weight = Some(BestValue { value, achieved_on });
                updates.max_weight = true;
            }
        }
        if let Some(value) = record.estimated_one_rep_max() {
            if self
                .estimated_one_rep_max
                .as_ref()
                .is_none_or(|best| value > best.value)
            {
                self.estimated_one_rep_max = Some(BestValue { value, achieved_on });
                updates.estimated_one_rep_max = true;
            }
        }
        if let Some(value) =
            (record.strict_total_weight_kg() > 0.0).then(|| record.strict_total_weight_kg())
        {
            if self
                .total_weight
                .as_ref()
                .is_none_or(|best| value > best.value)
            {
                self.total_weight = Some(BestValue { value, achieved_on });
                updates.total_weight = true;
            }
        }
        if let Some(value) = record.longest_duration_seconds() {
            if self
                .longest_duration
                .as_ref()
                .is_none_or(|best| value > best.value)
            {
                self.longest_duration = Some(BestValue { value, achieved_on });
                updates.longest_duration = true;
            }
        }
        updates
    }
}

pub(crate) fn validate_rpe(rpe: Option<f64>) -> Result<(), SetValidationError> {
    if rpe.is_some_and(|rpe| !crate::rpe::is_valid(rpe)) {
        return Err(SetValidationError::InvalidRpe);
    }
    Ok(())
}

fn validate_weight_segment(segment: &SetSegment) -> Result<(), SetValidationError> {
    if segment.left.is_some() != segment.right.is_some() {
        return Err(SetValidationError::MissingSide);
    }
    let has_sides = segment.left.is_some();
    if !has_sides {
        if !segment.weight_kg.is_finite() || segment.weight_kg < 0.0 {
            return Err(SetValidationError::InvalidWeight);
        }
        if segment.reps == 0 {
            return Err(SetValidationError::InvalidReps);
        }
        if u64::from(segment.momentum_reps) + u64::from(segment.assisted_reps)
            > u64::from(segment.reps)
        {
            return Err(SetValidationError::TooManyAssistedReps);
        }
    }
    for side in [segment.left.as_ref(), segment.right.as_ref()]
        .into_iter()
        .flatten()
    {
        if !side.weight_kg.is_finite() || side.weight_kg < 0.0 {
            return Err(SetValidationError::InvalidWeight);
        }
        if side.reps == 0 {
            return Err(SetValidationError::InvalidReps);
        }
        if u64::from(segment.momentum_reps) + u64::from(segment.assisted_reps)
            > u64::from(side.reps)
        {
            return Err(SetValidationError::TooManyAssistedReps);
        }
    }
    Ok(())
}

fn validate_time(
    segments: &[SetSegment],
    duration_seconds: Option<u32>,
) -> Result<(), SetValidationError> {
    if duration_seconds.is_some() && !segments.is_empty() {
        return Err(SetValidationError::ConflictingDuration);
    }
    if let Some(duration) = duration_seconds {
        validate_duration(duration)?;
    }
    if segments.is_empty() && duration_seconds.is_none() {
        return Err(SetValidationError::MissingDuration);
    }
    for segment in segments {
        let mut has_duration = false;
        if let Some(duration) = segment.duration_seconds {
            validate_duration(duration)?;
            has_duration = true;
        }
        for side in [segment.left.as_ref(), segment.right.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(duration) = side.duration_seconds {
                validate_duration(duration)?;
                has_duration = true;
            }
        }
        if !has_duration {
            return Err(SetValidationError::MissingDuration);
        }
    }
    Ok(())
}

fn validate_duration(duration: u32) -> Result<(), SetValidationError> {
    if !(1..=86_400).contains(&duration) {
        return Err(SetValidationError::InvalidDuration);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
