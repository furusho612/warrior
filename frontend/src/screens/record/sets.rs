//! 記録のセットの表と、セットの行の状態。
//!
//! セットの行は端末内の保存の行と入力を一緒に持つ。入力は画面へすぐ反映し、
//! 保存は `frontend/src/screens/record/exercise_view/operations.rs` が端末内の保存へ送る。
//! セットの表は `frontend/src/screens/record/set_table.rs` が描く。
//! セットの集計は `warrior_core::set_calc` の `SetRecord` に変換して行う。

use dioxus::prelude::*;
use warrior_core::ai::output::SetProposal;
use warrior_core::duration;
use warrior_core::exercise::{GripMode, GripSelection, Laterality, RecordMode};
use warrior_core::previous::PreviousSet;
use warrior_core::profile::bodyweight_set_weight;
use warrior_core::rpe;
use warrior_core::set_calc::{BestKey, OneRepMax, SetKind, SetRecord, SetSegment, SideValue};
use warrior_core::set_rules;
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

pub(super) use super::set_inputs::Field;
use crate::grip_text::{attachment_text, grip_style_text, grip_width_text};
use crate::storage::StoredSet;
use crate::text::{text, Key};
use crate::training_set::grip_json;
use crate::training_set::{SegmentSideValues, SetSegmentValues, SetValues};

/// セットの表から送る操作。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SetAction {
    /// セットを追加する。
    Add {
        /// 種目の行の ID。
        view_id: String,
    },
    /// AI の次回セットの提案を入力欄へ反映し、提案のセット数を追加する。
    ApplyProposal {
        /// 種目の行の ID。
        view_id: String,
        /// 提案。
        proposal: SetProposal,
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
    /// RPE を変える。
    Rpe {
        /// セットの ID。
        set_id: String,
        /// 入力の値。
        value: String,
    },
    /// グリップを変える。
    Grip {
        /// セットの ID。
        set_id: String,
        /// 選んだグリップ。
        grip: GripSelection,
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

/// セット詳細の drawer が開いているセット。
#[derive(Clone, Copy, PartialEq)]
pub(super) struct SelectedSet(pub(super) Signal<Option<String>>);

/// 画面が読む 1 つの段。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SegmentView {
    /// 端末内の保存の段。
    pub(super) record: SetSegmentRecord,
    /// 重量または加重の入力。
    pub(super) weight: String,
    /// 回数の入力。
    pub(super) reps: String,
    /// 反動を使った回数の入力。
    pub(super) momentum: String,
    /// 補助を受けた回数の入力。
    pub(super) assisted: String,
    /// 時間の入力。
    pub(super) duration: String,
    /// 左の重量または加重の入力。
    pub(super) left_weight: String,
    /// 左の回数の入力。
    pub(super) left_reps: String,
    /// 左の時間の入力。
    pub(super) left_duration: String,
    /// 右の重量または加重の入力。
    pub(super) right_weight: String,
    /// 右の回数の入力。
    pub(super) right_reps: String,
    /// 右の時間の入力。
    pub(super) right_duration: String,
}

impl SegmentView {
    fn from_record(record: SetSegmentRecord) -> Self {
        Self {
            weight: number_text(record.weight_kg.unwrap_or(0.0)),
            reps: count_text(record.reps.unwrap_or(0)),
            momentum: count_text(record.momentum_reps),
            assisted: count_text(record.assisted_reps),
            duration: duration_text(record.duration_seconds),
            left_weight: side_weight_text(record.left_weight_kg),
            left_reps: count_text(record.left_reps.unwrap_or(0)),
            left_duration: duration_text(record.left_duration_seconds),
            right_weight: side_weight_text(record.right_weight_kg),
            right_reps: count_text(record.right_reps.unwrap_or(0)),
            right_duration: duration_text(record.right_duration_seconds),
            record,
        }
    }

    fn values(&self, lateral: bool, mode: RecordMode) -> SetSegmentValues {
        let side = |weight: &str, reps: &str, duration: &str| SegmentSideValues {
            weight_kg: side_weight_value(weight, mode),
            reps: parse_count(reps).unwrap_or(0),
            duration_seconds: parse_duration(duration),
        };
        SetSegmentValues {
            weight_kg: parse_number(&self.weight).unwrap_or(0.0),
            reps: parse_count(&self.reps).unwrap_or(0),
            momentum_reps: parse_count_or_zero(&self.momentum).unwrap_or(0),
            assisted_reps: parse_count_or_zero(&self.assisted).unwrap_or(0),
            duration_seconds: parse_duration(&self.duration),
            left: lateral.then(|| side(&self.left_weight, &self.left_reps, &self.left_duration)),
            right: lateral
                .then(|| side(&self.right_weight, &self.right_reps, &self.right_duration)),
        }
    }

    fn can_complete(&self, mode: RecordMode, lateral: bool) -> bool {
        if mode != RecordMode::Time && self.breakdown_invalid(lateral) {
            return false;
        }
        match mode {
            RecordMode::Time => {
                if lateral {
                    parse_duration(&self.left_duration).is_some()
                        && parse_duration(&self.right_duration).is_some()
                } else {
                    parse_duration(&self.duration).is_some()
                }
            }
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
        if mode == RecordMode::Time {
            return self.to_time_segment(lateral);
        }
        let momentum_reps = parse_count_or_zero(&self.momentum)?;
        let assisted_reps = parse_count_or_zero(&self.assisted)?;
        if lateral {
            return Some(SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps,
                assisted_reps,
                left: Some(self.to_side(
                    mode,
                    body_weight_kg,
                    &self.left_weight,
                    &self.left_reps,
                )?),
                right: Some(self.to_side(
                    mode,
                    body_weight_kg,
                    &self.right_weight,
                    &self.right_reps,
                )?),
                duration_seconds: None,
            });
        }
        let weight = parse_number(&self.weight)?;
        let reps = parse_count(&self.reps)?;
        let weight = bodyweight_weight(mode, body_weight_kg, weight);
        Some(SetSegment {
            weight_kg: weight,
            reps,
            momentum_reps,
            assisted_reps,
            left: None,
            right: None,
            duration_seconds: None,
        })
    }

    fn to_time_segment(&self, lateral: bool) -> Option<SetSegment> {
        if lateral {
            let left = parse_duration(&self.left_duration)?;
            let right = parse_duration(&self.right_duration)?;
            return Some(SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps: 0,
                left: Some(SideValue {
                    weight_kg: 0.0,
                    reps: 0,
                    duration_seconds: Some(left),
                }),
                right: Some(SideValue {
                    weight_kg: 0.0,
                    reps: 0,
                    duration_seconds: Some(right),
                }),
                duration_seconds: None,
            });
        }
        Some(SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: Some(parse_duration(&self.duration)?),
        })
    }

    fn breakdown_invalid(&self, lateral: bool) -> bool {
        let Some(momentum_reps) = parse_count_or_zero(&self.momentum) else {
            return true;
        };
        let Some(assisted_reps) = parse_count_or_zero(&self.assisted) else {
            return true;
        };
        let used = u64::from(momentum_reps) + u64::from(assisted_reps);
        if lateral {
            let left = parse_count(&self.left_reps).unwrap_or(0);
            let right = parse_count(&self.right_reps).unwrap_or(0);
            used > u64::from(left) || used > u64::from(right)
        } else {
            used > u64::from(parse_count(&self.reps).unwrap_or(0))
        }
    }

    fn to_side(
        &self,
        mode: RecordMode,
        body_weight_kg: Option<f64>,
        weight: &str,
        reps: &str,
    ) -> Option<SideValue> {
        let weight = parse_number(weight)?;
        let reps = parse_count(reps)?;
        let weight = bodyweight_weight(mode, body_weight_kg, weight);
        Some(SideValue {
            weight_kg: weight,
            reps,
            duration_seconds: None,
        })
    }
}

/// 種目の左右の指定から、左右の列を使うか決める。
///
/// 時間で記録する種目も左右別の入力を持つ。
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
    /// RPE の入力。空でもよい。
    pub(super) rpe: String,
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
            rpe: rpe_text(stored.set.rpe),
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
            rpe: rpe_text(record.rpe),
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

    /// このセットで選んだグリップ。
    pub(super) fn grip(&self) -> Option<GripSelection> {
        grip_selection(&self.record)
    }

    /// このセットのグリップを変える。
    pub(super) fn set_grip(&mut self, grip: GripSelection) {
        self.record.grip = grip_json(Some(grip));
        self.bump();
    }

    /// 入力から保存する値を作る。空の回数は 0 とする。
    pub(super) fn values(&self, lateral: bool, mode: RecordMode) -> SetValues {
        SetValues {
            kind: self.kind(),
            mode,
            segments: self
                .segments
                .iter()
                .map(|segment| segment.values(lateral, mode))
                .collect::<Vec<_>>(),
            completed: self.completed(),
            body_weight_kg: self.record.body_weight_kg,
            rpe: rpe::parse(&self.rpe).ok().flatten(),
            grip: grip_selection(&self.record),
        }
    }

    /// このセットの推定 1RM。時間で記録する種目、ウォームアップ、重量か回数が空のときは None。
    pub(super) fn one_rep_max(&self, mode: RecordMode, lateral: bool) -> Option<OneRepMax> {
        if mode == RecordMode::Time || self.kind() == SetKind::Warmup {
            return None;
        }
        self.segments
            .first()?
            .to_set_segment(mode, self.record.body_weight_kg, lateral)?
            .one_rep_max()
    }

    /// 入力の誤りを返す。RPE と回数の内訳を確かめる。
    pub(super) fn validation_error(&self, lateral: bool) -> Option<Key> {
        if rpe::parse(&self.rpe).is_err() {
            return Some(Key::RecordSetRpeInvalid);
        }
        if self
            .segments
            .iter()
            .any(|segment| segment.breakdown_invalid(lateral))
        {
            return Some(Key::RecordSetBreakdownInvalid);
        }
        None
    }

    /// 完了できるか。時間の種目は時間、それ以外は回数が要る。
    pub(super) fn can_complete(&self, mode: RecordMode, lateral: bool) -> bool {
        if self.completed() {
            return true;
        }
        if self.segments.is_empty() {
            return false;
        }
        if self.validation_error(lateral).is_some() {
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
        let rpe = rpe::parse(&self.rpe).ok().flatten();
        let segments = self
            .segments
            .iter()
            .map(|segment| segment.to_set_segment(mode, self.record.body_weight_kg, lateral))
            .collect::<Option<Vec<_>>>()?;
        SetRecord::new(key, kind, segments, rpe, None, mode).ok()
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
                RecordMode::Time if !lateral => {
                    let duration = segment.longest_duration_seconds();
                    (duration > 0).then_some(duration)
                }
                RecordMode::Time | RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                    None
                }
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
                duration_seconds: match mode {
                    RecordMode::Time => value.and_then(|side| side.duration_seconds),
                    RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => None,
                },
            };
            SetSegmentValues {
                weight_kg: weight,
                reps: segment.reps,
                momentum_reps: segment.momentum_reps,
                assisted_reps: segment.assisted_reps,
                duration_seconds: duration,
                left: lateral.then(|| side(segment.left.as_ref())),
                right: lateral.then(|| side(segment.right.as_ref())),
            }
        })
        .collect();
    Some(SetValues {
        kind: previous.kind,
        mode,
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

/// セット番号のボタンの読み上げの文言。
pub(super) fn detail_open_label(number_text: &str, is_drop: bool) -> String {
    let prefix = if is_drop {
        format!("{}{}", text(Key::RecordSetKindDrop), text(Key::RecordSets))
    } else {
        text(Key::RecordSets).to_owned()
    };
    format!(
        "{} {} {}",
        prefix,
        number_text,
        text(Key::RecordSetDetailOpen)
    )
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
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| duration::is_valid(*value))
        .map_or_else(String::new, duration::format)
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

fn parse_count_or_zero(text: &str) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0);
    }
    text.parse::<u32>().ok()
}

fn rpe_text(value: Option<f64>) -> String {
    value.map_or_else(String::new, rpe::format)
}

fn parse_duration(text: &str) -> Option<u32> {
    duration::parse(text)
}

/// 新しいセットに使うグリップを返す。
///
/// その種目の直近のセットの選択を既定にし、直近が無いときと選択肢に無いときは
/// 選択肢の先頭にする。グリップを選ばない種目は None を返す。
pub(super) fn default_grip(
    mode: GripMode,
    options: &[GripSelection],
    latest: Option<GripSelection>,
) -> Option<GripSelection> {
    if mode == GripMode::None {
        return None;
    }
    latest
        .filter(|selection| options.contains(selection))
        .or_else(|| options.first().copied())
}

/// グリップが無いセットに、直近の選択または先頭の選択を入れる。
pub(super) fn fill_missing_grips(sets: &mut [SetView], mode: GripMode, options: &[GripSelection]) {
    let mut latest = None;
    for set in sets {
        if set.grip().is_none() {
            if let Some(grip) = default_grip(mode, options, latest) {
                set.set_grip(grip);
            }
        }
        latest = set.grip().or(latest);
    }
}

/// 前回の応答を画面へ反映してよいか。
pub(super) fn previous_response_is_current(
    current: Option<GripSelection>,
    expected: GripSelection,
) -> bool {
    current == Some(expected)
}

/// グリップの選択を画面に出す文言にする。
pub(super) fn grip_selection_label(selection: GripSelection) -> String {
    match selection {
        GripSelection::None => text(Key::RecordSetGripNone).to_owned(),
        GripSelection::Grip { style, width } => {
            format!("{} {}", grip_style_text(style), grip_width_text(width))
        }
        GripSelection::Attachment { attachment, style } => match style {
            Some(style) => format!(
                "{} / {}",
                grip_style_text(style),
                attachment_text(attachment)
            ),
            None => attachment_text(attachment).to_owned(),
        },
    }
}

fn grip_selection(record: &SetRecordRow) -> Option<GripSelection> {
    record
        .grip
        .as_deref()
        .and_then(|text| serde_json::from_str::<GripSelection>(text).ok())
}

#[cfg(test)]
mod tests;
