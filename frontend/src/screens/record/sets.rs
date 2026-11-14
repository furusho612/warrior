//! 記録のセットの表と、セットの行の状態。
//!
//! セットの行は端末内の保存の行と入力を一緒に持つ。入力は画面へすぐ反映し、
//! 保存は `frontend/src/screens/record/exercise_view.rs` が端末内の保存へ送る。
//! セットの表は `frontend/src/screens/record/set_table.rs` が描く。
//! セットの集計は `warrior_core::set_calc` の `SetRecord` に変換して行う。

use warrior_core::exercise::{GripSelection, Laterality, RecordMode};
use warrior_core::previous::PreviousSet;
use warrior_core::profile::bodyweight_set_weight;
use warrior_core::set_calc::{BestKey, SetKind, SetRecord, SetSegment, SideValue};
use warrior_core::set_rules;
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

pub(super) use super::set_inputs::Field;
use crate::storage::StoredSet;
use crate::text::{text, Key};
use crate::training_set::{SegmentSideValues, SetSegmentValues, SetValues};

/// セットの表から送る操作。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SetAction {
    /// セットを追加する。
    Add {
        /// 種目の行の ID。
        view_id: String,
    },
    /// 前回の値をコピーする。
    Copy {
        /// 種目の行の ID。
        view_id: String,
    },
    /// 入力の値を変える。
    Input {
        /// セットの ID。
        set_id: String,
        /// 段の ID。
        segment_id: String,
        /// 入力の欄。
        field: Field,
        /// 入力の値。
        value: String,
    },
    /// セット種別を変える。
    Kind {
        /// セットの ID。
        set_id: String,
        /// セット種別。
        kind: SetKind,
    },
    /// 完了の状態を変える。
    Complete {
        /// セットの ID。
        set_id: String,
    },
    /// セットを削除する。
    Delete {
        /// セットの ID。
        set_id: String,
    },
    /// 段を追加する。
    AddSegment {
        /// セットの ID。
        set_id: String,
    },
    /// 段を削除する。
    DeleteSegment {
        /// セットの ID。
        set_id: String,
        /// 段の ID。
        segment_id: String,
    },
    /// セットの順序を変える。
    Move {
        /// セットの ID。
        set_id: String,
        /// 移動する向き。前が -1、後ろが 1。
        delta: i32,
    },
}

/// 画面が読む 1 つの段。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SegmentView {
    /// 端末内の保存の段。
    pub(super) record: SetSegmentRecord,
    /// 重量または加重の入力。
    pub(super) weight: String,
    /// 回数の入力。
    pub(super) reps: String,
    /// 時間の入力。
    pub(super) duration: String,
    /// 左の重量または加重の入力。
    pub(super) left_weight: String,
    /// 左の回数の入力。
    pub(super) left_reps: String,
    /// 右の重量または加重の入力。
    pub(super) right_weight: String,
    /// 右の回数の入力。
    pub(super) right_reps: String,
}

impl SegmentView {
    fn from_record(record: SetSegmentRecord) -> Self {
        Self {
            weight: number_text(record.weight_kg.unwrap_or(0.0)),
            reps: count_text(record.reps.unwrap_or(0)),
            duration: duration_text(record.duration_seconds),
            left_weight: side_weight_text(record.left_weight_kg),
            left_reps: count_text(record.left_reps.unwrap_or(0)),
            right_weight: side_weight_text(record.right_weight_kg),
            right_reps: count_text(record.right_reps.unwrap_or(0)),
            record,
        }
    }

    fn values(&self, lateral: bool, mode: RecordMode) -> SetSegmentValues {
        let side = |weight: &str, reps: &str| SegmentSideValues {
            weight_kg: side_weight_value(weight, mode),
            reps: parse_count(reps).unwrap_or(0),
        };
        SetSegmentValues {
            weight_kg: parse_number(&self.weight).unwrap_or(0.0),
            reps: parse_count(&self.reps).unwrap_or(0),
            duration_seconds: parse_duration(&self.duration),
            left: lateral.then(|| side(&self.left_weight, &self.left_reps)),
            right: lateral.then(|| side(&self.right_weight, &self.right_reps)),
        }
    }

    fn can_complete(&self, mode: RecordMode, lateral: bool) -> bool {
        match mode {
            RecordMode::Time => parse_duration(&self.duration).is_some(),
            RecordMode::WeightAndReps => {
                if lateral {
                    parse_count(&self.left_reps).is_some()
                        && parse_weight(&self.left_weight).is_some()
                        && parse_count(&self.right_reps).is_some()
                        && parse_weight(&self.right_weight).is_some()
                } else {
                    parse_count(&self.reps).is_some() && parse_number(&self.weight).is_some()
                }
            }
            RecordMode::BodyweightAndReps => {
                if lateral {
                    parse_count(&self.left_reps).is_some()
                        && parse_count(&self.right_reps).is_some()
                } else {
                    parse_count(&self.reps).is_some() && parse_number(&self.weight).is_some()
                }
            }
        }
    }

    fn to_set_segment(
        &self,
        mode: RecordMode,
        body_weight_kg: Option<f64>,
        lateral: bool,
    ) -> Option<SetSegment> {
        if lateral {
            return Some(SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps: 0,
                left: Some(self.to_side(mode, body_weight_kg, &self.left_weight, &self.left_reps)),
                right: Some(self.to_side(
                    mode,
                    body_weight_kg,
                    &self.right_weight,
                    &self.right_reps,
                )),
                duration_seconds: None,
            });
        }
        let weight = parse_number(&self.weight)?;
        let reps = parse_count(&self.reps)?;
        let weight = bodyweight_weight(mode, body_weight_kg, weight);
        Some(SetSegment {
            weight_kg: weight,
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        })
    }

    fn to_side(
        &self,
        mode: RecordMode,
        body_weight_kg: Option<f64>,
        weight: &str,
        reps: &str,
    ) -> SideValue {
        let weight = bodyweight_weight(mode, body_weight_kg, parse_number(weight).unwrap_or(0.0));
        SideValue {
            weight_kg: weight,
            reps: parse_count(reps).unwrap_or(0),
            duration_seconds: None,
        }
    }
}

/// 種目の記録方式と左右の指定から、左右の列を使うか決める。
///
/// 時間で記録する種目の左右は、時間の入力とあわせて実装する。
pub(super) use warrior_core::exercise::is_lateral;

/// 重量種目は入力した重量を、自重種目は記録した体重に加重を足した値を返す。
fn bodyweight_weight(mode: RecordMode, body_weight_kg: Option<f64>, weight: f64) -> f64 {
    match mode {
        RecordMode::BodyweightAndReps => {
            bodyweight_set_weight(body_weight_kg, weight).unwrap_or(0.0)
        }
        RecordMode::WeightAndReps | RecordMode::Time => weight,
    }
}

/// 画面が読む 1 つのセット。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SetView {
    /// 端末内の保存の行。
    pub(super) record: SetRecordRow,
    /// 端末内の保存の段。並び順である。
    pub(super) segments: Vec<SegmentView>,
    /// 入力の版。保存の完了を判定する。
    pub(super) revision: u64,
}

impl SetView {
    /// 端末内の保存の行から画面の行を作る。
    pub(super) fn from_stored(stored: StoredSet) -> Option<Self> {
        if stored.segments.is_empty() {
            return None;
        }
        Some(Self {
            record: stored.set,
            segments: stored
                .segments
                .into_iter()
                .map(SegmentView::from_record)
                .collect(),
            revision: 0,
        })
    }

    /// 保存した行から画面の行を作る。
    pub(super) fn from_rows(record: SetRecordRow, segments: Vec<SetSegmentRecord>) -> Self {
        Self {
            record,
            segments: segments.into_iter().map(SegmentView::from_record).collect(),
            revision: 0,
        }
    }

    /// セット種別。
    pub(super) fn kind(&self) -> SetKind {
        SetKind::parse(&self.record.kind).unwrap_or(SetKind::Normal)
    }

    /// 完了しているか。
    pub(super) fn completed(&self) -> bool {
        self.record.completed != 0
    }

    /// 入力から保存する値を作る。空の回数は 0 とする。
    pub(super) fn values(&self, lateral: bool, mode: RecordMode) -> SetValues {
        SetValues {
            kind: self.kind(),
            segments: self
                .segments
                .iter()
                .map(|segment| segment.values(lateral, mode))
                .collect::<Vec<_>>(),
            completed: self.completed(),
            body_weight_kg: self.record.body_weight_kg,
            rpe: self.record.rpe,
            grip: grip_selection(&self.record),
        }
    }

    /// 完了できるか。時間の種目は時間、それ以外は回数が要る。
    pub(super) fn can_complete(&self, mode: RecordMode, lateral: bool) -> bool {
        if self.completed() {
            return true;
        }
        if self.segments.is_empty() {
            return false;
        }
        self.segments
            .iter()
            .all(|segment| segment.can_complete(mode, lateral))
    }

    /// crate::set_calc のセットの記録に変換する。入力が足りないときは None。
    pub(super) fn to_record(
        &self,
        exercise_id: &str,
        mode: RecordMode,
        lateral: bool,
    ) -> Option<SetRecord> {
        let kind = self.kind();
        let (grip, attachment) = grip_selection(&self.record)
            .map(|grip| grip.comparison_key())
            .unwrap_or((None, None));
        let key = BestKey {
            exercise_id: exercise_id.to_owned(),
            grip,
            attachment,
        };
        match mode {
            RecordMode::Time => {
                let duration = parse_duration(&self.segments.first()?.duration)?;
                SetRecord::new(key, kind, Vec::new(), self.record.rpe, Some(duration), mode).ok()
            }
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                let segments = self
                    .segments
                    .iter()
                    .map(|segment| {
                        segment.to_set_segment(mode, self.record.body_weight_kg, lateral)
                    })
                    .collect::<Option<Vec<_>>>()?;
                SetRecord::new(key, kind, segments, self.record.rpe, None, mode).ok()
            }
        }
    }

    /// 入力の版を進める。
    pub(super) fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

/// 前回のセットから、コピーする入力の値を作る。
///
/// 自重種目の前回の重量は記録した時点の体重を含むため、前回の記録時の体重を引いて
/// 加重にする。前回の体重が無いときは、前回の重量を復元できないため加重を 0 とする。
pub(super) fn previous_values(
    previous: &PreviousSet,
    mode: RecordMode,
    body_weight_kg: Option<f64>,
    laterality: Laterality,
) -> Option<SetValues> {
    if previous.segments.is_empty() {
        return None;
    }
    let lateral = is_lateral(laterality, mode);
    let segments = previous
        .segments
        .iter()
        .map(|segment| {
            let weight = match mode {
                RecordMode::BodyweightAndReps => previous
                    .body_weight_kg
                    .map_or(0.0, |body| (segment.weight_kg - body).max(0.0)),
                RecordMode::WeightAndReps => segment.weight_kg,
                RecordMode::Time => 0.0,
            };
            let duration = match mode {
                RecordMode::Time => {
                    let duration = segment.longest_duration_seconds();
                    (duration > 0).then_some(duration)
                }
                RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => None,
            };
            let side = |value: Option<&SideValue>| SegmentSideValues {
                weight_kg: Some(match mode {
                    RecordMode::BodyweightAndReps => previous.body_weight_kg.map_or(0.0, |body| {
                        (value.map_or(0.0, |side| side.weight_kg) - body).max(0.0)
                    }),
                    RecordMode::WeightAndReps | RecordMode::Time => {
                        value.map_or(0.0, |side| side.weight_kg)
                    }
                }),
                reps: value.map_or(0, |side| side.reps),
            };
            SetSegmentValues {
                weight_kg: weight,
                reps: segment.reps,
                duration_seconds: duration,
                left: lateral.then(|| side(segment.left.as_ref())),
                right: lateral.then(|| side(segment.right.as_ref())),
            }
        })
        .collect();
    Some(SetValues {
        kind: previous.kind,
        segments,
        body_weight_kg,
        rpe: previous.rpe,
        grip: previous.grip,
        completed: false,
    })
}

pub(super) fn set_numbers(rows: &[SetView]) -> Vec<Option<usize>> {
    let mut numbers = Vec::with_capacity(rows.len());
    let mut number = 0usize;
    for row in rows {
        if row.kind() == SetKind::Drop {
            numbers.push(None);
        } else {
            number = number.saturating_add(1);
            numbers.push(Some(number));
        }
    }
    numbers
}

pub(super) fn kind_change_allowed(kinds: &[SetKind], index: usize, kind: SetKind) -> bool {
    let mut candidate = kinds.to_vec();
    let Some(slot) = candidate.get_mut(index) else {
        return false;
    };
    *slot = kind;
    set_rules::drop_set_positions_valid(&candidate)
}

/// 並びの index のセット種別を変えたときの規則を満たすか返す。
pub(super) fn kind_allowed(rows: &[SetView], index: usize, kind: SetKind) -> bool {
    let kinds = rows.iter().map(SetView::kind).collect::<Vec<_>>();
    kind_change_allowed(&kinds, index, kind)
}

pub(super) fn move_candidate_valid(kinds: &[SetKind], index: usize, delta: i32) -> bool {
    let target = index as i32 + delta;
    if target < 0 || target as usize >= kinds.len() {
        return false;
    }
    let mut candidate = kinds.to_vec();
    candidate.swap(index, target as usize);
    set_rules::drop_set_positions_valid(&candidate)
}

/// 並びの index のセットを delta だけ動かしたときの規則を満たすか返す。
pub(super) fn move_allowed(rows: &[SetView], index: usize, delta: i32) -> bool {
    let kinds = rows.iter().map(SetView::kind).collect::<Vec<_>>();
    move_candidate_valid(&kinds, index, delta)
}

/// 段を減らす変更の前に確認が要るか。
///
/// ドロップセットから別の種別へ変えると、2 つ目以降の段を捨てることになる。
pub(super) fn needs_segment_confirmation(
    kind: SetKind,
    new_kind: SetKind,
    segment_count: usize,
) -> bool {
    kind == SetKind::Drop && new_kind != SetKind::Drop && segment_count > 1
}

/// セット種別の文言。
pub(super) fn set_kind_text(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Normal => text(Key::RecordSetKindNormal),
        SetKind::Warmup => text(Key::RecordSetKindWarmup),
        SetKind::Drop => text(Key::RecordSetKindDrop),
    }
}

/// 数値の表示。整数は小数点を付けない。
pub(super) fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

fn number_text(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        String::new()
    } else {
        format_number(value)
    }
}

fn side_weight_text(value: Option<f64>) -> String {
    value.map_or_else(String::new, format_number)
}

fn side_weight_value(text: &str, mode: RecordMode) -> Option<f64> {
    match mode {
        RecordMode::WeightAndReps => parse_weight(text),
        RecordMode::BodyweightAndReps => Some(parse_number(text).unwrap_or(0.0)),
        RecordMode::Time => None,
    }
}

fn count_text(value: i64) -> String {
    if value > 0 {
        value.to_string()
    } else {
        String::new()
    }
}

fn duration_text(value: Option<i64>) -> String {
    value
        .filter(|value| *value > 0)
        .map_or_else(String::new, |value| value.to_string())
}

fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0.0);
    }
    let value = text.parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// 重量欄の入力。空の欄と 0 を区別する。
fn parse_weight(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let value = text.parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn parse_count(text: &str) -> Option<u32> {
    let value = text.trim().parse::<u32>().ok()?;
    (value > 0).then_some(value)
}

fn parse_duration(text: &str) -> Option<u32> {
    let value = text.trim().parse::<u32>().ok()?;
    (1..=86_400).contains(&value).then_some(value)
}

fn grip_selection(record: &SetRecordRow) -> Option<GripSelection> {
    record
        .grip
        .as_deref()
        .and_then(|text| serde_json::from_str::<GripSelection>(text).ok())
}

#[cfg(test)]
mod tests;
