//! 記録のセットの表と、セットの行の状態。
//!
//! セットの行は端末内の保存の行と入力を一緒に持つ。入力は画面へすぐ反映し、
//! 保存は `frontend/src/screens/record/exercises.rs` が端末内の保存へ送る。
//! ドロップセットは段を 1 行ずつ表示し、段の行を親セットの直後に置く。
//! セットの集計は `warrior_core::set_calc` の `SetRecord` に変換して行う。

use dioxus::prelude::*;
use warrior_core::exercise::{GripSelection, Laterality, RecordMode};
use warrior_core::previous::PreviousSet;
use warrior_core::profile::bodyweight_set_weight;
use warrior_core::rpe;
use warrior_core::set_calc::{BestKey, SetKind, SetRecord, SetSegment, SideValue};
use warrior_core::set_rules;
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

pub(super) use super::set_inputs::Field;
use super::set_inputs::SegmentInputs;
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
    /// RPE を変える。
    Rpe {
        /// セットの ID。
        set_id: String,
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
            momentum: count_text(record.momentum_reps),
            assisted: count_text(record.assisted_reps),
            duration: duration_text(record.duration_seconds),
            left_weight: number_text(record.left_weight_kg.unwrap_or(0.0)),
            left_reps: count_text(record.left_reps.unwrap_or(0)),
            right_weight: number_text(record.right_weight_kg.unwrap_or(0.0)),
            right_reps: count_text(record.right_reps.unwrap_or(0)),
            record,
        }
    }

    fn values(&self, lateral: bool) -> SetSegmentValues {
        let side = |weight: &str, reps: &str| SegmentSideValues {
            weight_kg: parse_number(weight).unwrap_or(0.0),
            reps: parse_count(reps).unwrap_or(0),
        };
        SetSegmentValues {
            weight_kg: parse_number(&self.weight).unwrap_or(0.0),
            reps: parse_count(&self.reps).unwrap_or(0),
            momentum_reps: parse_count_or_zero(&self.momentum).unwrap_or(0),
            assisted_reps: parse_count_or_zero(&self.assisted).unwrap_or(0),
            duration_seconds: parse_duration(&self.duration),
            left: lateral.then(|| side(&self.left_weight, &self.left_reps)),
            right: lateral.then(|| side(&self.right_weight, &self.right_reps)),
        }
    }

    fn can_complete(&self, mode: RecordMode, lateral: bool) -> bool {
        match mode {
            RecordMode::Time => parse_duration(&self.duration).is_some(),
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                if self.breakdown_invalid(lateral) {
                    return false;
                }
                if lateral {
                    parse_count(&self.left_reps).is_some()
                        && parse_number(&self.left_weight).is_some()
                        && parse_count(&self.right_reps).is_some()
                        && parse_number(&self.right_weight).is_some()
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
        let weight = match mode {
            RecordMode::BodyweightAndReps => {
                bodyweight_set_weight(body_weight_kg, weight).unwrap_or(0.0)
            }
            RecordMode::WeightAndReps | RecordMode::Time => weight,
        };
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
        let weight = match mode {
            RecordMode::BodyweightAndReps => {
                bodyweight_set_weight(body_weight_kg, weight).unwrap_or(0.0)
            }
            RecordMode::WeightAndReps | RecordMode::Time => weight,
        };
        Some(SideValue {
            weight_kg: weight,
            reps,
            duration_seconds: None,
        })
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

    /// 入力から保存する値を作る。空の回数は 0 とする。
    pub(super) fn values(&self, lateral: bool) -> SetValues {
        SetValues {
            kind: self.kind(),
            segments: self
                .segments
                .iter()
                .map(|segment| segment.values(lateral))
                .collect::<Vec<_>>(),
            completed: self.completed(),
            body_weight_kg: self.record.body_weight_kg,
            rpe: rpe::parse(&self.rpe).ok().flatten(),
            grip: grip_selection(&self.record),
        }
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
        match mode {
            RecordMode::Time => {
                let duration = parse_duration(&self.segments.first()?.duration)?;
                let rpe = rpe::parse(&self.rpe).ok().flatten();
                SetRecord::new(key, kind, Vec::new(), rpe, Some(duration), mode).ok()
            }
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                let rpe = rpe::parse(&self.rpe).ok().flatten();
                let segments = self
                    .segments
                    .iter()
                    .map(|segment| {
                        segment.to_set_segment(mode, self.record.body_weight_kg, lateral)
                    })
                    .collect::<Option<Vec<_>>>()?;
                SetRecord::new(key, kind, segments, rpe, None, mode).ok()
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
    let lateral = laterality == Laterality::Unilateral && mode != RecordMode::Time;
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
                weight_kg: match mode {
                    RecordMode::BodyweightAndReps => previous.body_weight_kg.map_or(0.0, |body| {
                        (value.map_or(0.0, |side| side.weight_kg) - body).max(0.0)
                    }),
                    RecordMode::WeightAndReps | RecordMode::Time => {
                        value.map_or(0.0, |side| side.weight_kg)
                    }
                },
                reps: value.map_or(0, |side| side.reps),
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
        segments,
        body_weight_kg,
        rpe: previous.rpe,
        grip: previous.grip,
        completed: false,
    })
}

/// セットの表。
#[component]
pub(super) fn SetTable(
    view_id: String,
    rows: Vec<SetView>,
    is_time: bool,
    is_bodyweight: bool,
    is_lateral: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let add = on_action;
    let add_view_id = view_id.clone();
    let kinds = rows.iter().map(SetView::kind).collect::<Vec<_>>();
    let numbers = set_numbers(&rows);
    let display_rows = rows.iter().map(|row| row.segments.len()).sum::<usize>();
    rsx! {
        div { class: "flex flex-col gap-1",
            div { class: "flex items-center justify-between gap-2",
                h4 { class: "text-[11px] font-bold", "{text(Key::RecordSets)}" }
                button {
                    class: "min-h-9 border border-primary px-2 text-[11px] font-bold text-primary",
                    "data-add-set": "true",
                    disabled,
                    onclick: move |_| add.call(SetAction::Add { view_id: add_view_id.clone() }),
                    "{text(Key::RecordSetAdd)}"
                }
            }
            div {
                class: "h-[222px] overflow-y-auto border border-line bg-surface",
                "data-set-table": "true",
                "data-set-rows": "{rows.len()}",
                "data-set-display-rows": "{display_rows}",
                "data-set-row-height": "44",
                if rows.is_empty() {
                    p {
                        class: "p-2 text-[11px] text-text-sub",
                        "data-set-empty": "true",
                        "{text(Key::RecordExerciseEmpty)}"
                    }
                }
                for (index, row) in rows.iter().enumerate() {
                    SetRow {
                        index,
                        set: row.clone(),
                        number: numbers[index],
                        drop_allowed: kind_change_allowed(&kinds, index, SetKind::Drop),
                        warmup_allowed: kind_change_allowed(&kinds, index, SetKind::Warmup),
                        can_move_up: move_candidate_valid(&kinds, index, -1),
                        can_move_down: move_candidate_valid(&kinds, index, 1),
                        is_time,
                        is_bodyweight,
                        is_lateral,
                        disabled,
                        on_action,
                    }
                }
            }
            if disabled {
                p { class: "text-[11px] text-text-sub", "{text(Key::RecordViewOnly)}" }
            }
        }
    }
}

/// セットの表の 1 行。
#[component]
fn SetRow(
    index: usize,
    set: SetView,
    number: Option<usize>,
    drop_allowed: bool,
    warmup_allowed: bool,
    can_move_up: bool,
    can_move_down: bool,
    is_time: bool,
    is_bodyweight: bool,
    is_lateral: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let set_id = set.record.id.clone();
    let add_segment_id = set.record.id.clone();
    let mut selected_set = use_context::<SelectedSet>();
    let open_id = set.record.id.clone();
    let completed = set.completed();
    let mode = if is_time {
        RecordMode::Time
    } else if is_bodyweight {
        RecordMode::BodyweightAndReps
    } else {
        RecordMode::WeightAndReps
    };
    let ready = set.can_complete(mode, is_lateral);
    let kind = set.kind();
    let is_drop = kind == SetKind::Drop;
    let number_text = number.map_or_else(|| "D".to_owned(), |number| number.to_string());
    let open_label = detail_open_label(&number_text, is_drop);
    let first_segment = set.segments.first().cloned();
    let buttons = RowButtonState {
        set_id: set.record.id.clone(),
        first_segment_id: set
            .segments
            .first()
            .map(|segment| segment.record.id.clone()),
        is_drop,
        can_move_up,
        can_move_down,
        ready,
        completed,
    };
    rsx! {
        div {
            class: "flex flex-col",
            "data-set-row": "true",
            "data-set-row-index": "{index}",
            "data-set-id": "{set_id}",
            "data-set-kind": "{kind.as_str()}",
            "data-set-number": "{number_text}",
            "data-set-drop": if is_drop { "true" } else { "false" },
            "data-set-completed": if completed { "true" } else { "false" },
            div {
                class: if is_drop { "flex h-11 items-center gap-1 overflow-x-auto border-b border-line px-1 pl-4" } else { "flex h-11 items-center gap-1 overflow-x-auto border-b border-line px-1" },
                button {
                    class: "min-h-11 min-w-11 shrink-0 text-left text-[11px] text-text-sub",
                    "data-set-open": "true",
                    "aria-label": "{open_label}",
                    onclick: move |_| selected_set.0.set(Some(open_id.clone())),
                    "{number_text}"
                }
                KindSelect {
                    set_id: set.record.id.clone(),
                    kind,
                    disabled,
                    drop_allowed,
                    warmup_allowed,
                    on_action,
                }
                if let Some(segment) = first_segment {
                    div {
                        class: "flex items-center gap-1",
                        "data-segment-id": "{segment.record.id}",
                        "data-segment-index": "0",
                        SegmentInputs {
                            segment: segment.clone(),
                            set_id: set_id.clone(),
                            mode,
                            is_bodyweight,
                            is_lateral,
                            disabled,
                            on_action,
                        }
                    }
                }
                RowButtons { buttons, disabled, on_action }
            }
            for (segment_index, segment) in set.segments.iter().enumerate().skip(1) {
                SegmentRow {
                    set_id: set_id.clone(),
                    segment: segment.clone(),
                    segment_index,
                    is_time,
                    is_bodyweight,
                    is_lateral,
                    disabled,
                    on_action,
                }
            }
            if is_drop && !is_time {
                button {
                    class: "min-h-9 border-t border-line px-2 text-left text-[10px] font-bold text-primary",
                    "data-add-segment": "true",
                    disabled,
                    onclick: move |_| on_action.call(SetAction::AddSegment { set_id: add_segment_id.clone() }),
                    "{text(Key::RecordSetSegmentAdd)}"
                }
            }
        }
    }
}

/// セット種別の選択。
#[component]
fn KindSelect(
    set_id: String,
    kind: SetKind,
    disabled: bool,
    drop_allowed: bool,
    warmup_allowed: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    rsx! {
        select {
            class: "min-h-8 w-16 shrink-0 border border-line bg-bg px-1 text-[10px]",
            "data-set-kind": "true",
            "aria-label": "{text(Key::RecordSetKindLabel)}",
            disabled,
            value: "{kind.as_str()}",
            onchange: move |event| {
                if let Some(kind) = SetKind::parse(&event.value()) {
                    on_action.call(SetAction::Kind { set_id: set_id.clone(), kind });
                }
            },
            option { value: "normal", selected: kind == SetKind::Normal, "{text(Key::RecordSetKindNormal)}" }
            option { value: "warmup", selected: kind == SetKind::Warmup, disabled: !warmup_allowed, "{text(Key::RecordSetKindWarmup)}" }
            option { value: "drop", selected: kind == SetKind::Drop, disabled: !drop_allowed, "{text(Key::RecordSetKindDrop)}" }
        }
    }
}

/// セットの行の操作のボタン。
#[derive(Debug, Clone, PartialEq)]
struct RowButtonState {
    /// セットの ID。
    set_id: String,
    /// 最初の段の ID。
    first_segment_id: Option<String>,
    /// ドロップセットか。
    is_drop: bool,
    /// 前へ移動できるか。
    can_move_up: bool,
    /// 後ろへ移動できるか。
    can_move_down: bool,
    /// 完了できるか。
    ready: bool,
    /// 完了しているか。
    completed: bool,
}

/// セットの行の移動、完了、削除のボタン。
#[component]
fn RowButtons(
    buttons: RowButtonState,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let up_id = buttons.set_id.clone();
    let down_id = buttons.set_id.clone();
    let complete_id = buttons.set_id.clone();
    let delete_id = buttons.set_id.clone();
    let delete_segment_set_id = buttons.set_id.clone();
    let first_segment_id = buttons.first_segment_id.clone();
    let can_move_up = buttons.can_move_up;
    let can_move_down = buttons.can_move_down;
    let ready = buttons.ready;
    let completed = buttons.completed;
    rsx! {
        button {
            class: "min-h-8 shrink-0 border border-line px-1 text-[10px]",
            "data-set-move-up": "true",
            disabled: disabled || !can_move_up,
            onclick: move |_| on_action.call(SetAction::Move { set_id: up_id.clone(), delta: -1 }),
            "{text(Key::RecordExerciseMoveUp)}"
        }
        button {
            class: "min-h-8 shrink-0 border border-line px-1 text-[10px]",
            "data-set-move-down": "true",
            disabled: disabled || !can_move_down,
            onclick: move |_| on_action.call(SetAction::Move { set_id: down_id.clone(), delta: 1 }),
            "{text(Key::RecordExerciseMoveDown)}"
        }
        button {
            class: "min-h-8 shrink-0 border border-primary px-1 text-[10px] font-bold text-primary",
            "data-set-complete": "true",
            "data-set-complete-ready": if ready { "true" } else { "false" },
            disabled: disabled || (!completed && !ready),
            onclick: move |_| on_action.call(SetAction::Complete { set_id: complete_id.clone() }),
            if completed {
                "{text(Key::RecordSetCompleted)}"
            } else {
                "{text(Key::RecordSetComplete)}"
            }
        }
        if buttons.is_drop {
            if let Some(segment_id) = first_segment_id {
                button {
                    class: "min-h-8 shrink-0 border border-line px-1 text-[10px] text-primary",
                    "data-delete-segment": "true",
                    disabled,
                    onclick: move |_| on_action.call(SetAction::DeleteSegment {
                        set_id: delete_segment_set_id.clone(),
                        segment_id: segment_id.clone(),
                    }),
                    "{text(Key::RecordSetSegmentDelete)}"
                }
            }
        }
        button {
            class: "min-h-8 shrink-0 border border-line px-1 text-[10px] text-primary",
            "data-delete-set": "true",
            disabled,
            onclick: move |_| on_action.call(SetAction::Delete { set_id: delete_id.clone() }),
            "{text(Key::RecordSetDelete)}"
        }
    }
}

/// セットの表の、2 つ目以降の段の行。
#[component]
fn SegmentRow(
    set_id: String,
    segment: SegmentView,
    segment_index: usize,
    is_time: bool,
    is_bodyweight: bool,
    is_lateral: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let mode = if is_time {
        RecordMode::Time
    } else if is_bodyweight {
        RecordMode::BodyweightAndReps
    } else {
        RecordMode::WeightAndReps
    };
    let delete_set_id = set_id.clone();
    let delete_segment_id = segment.record.id.clone();
    let segment_id = segment.record.id.clone();
    rsx! {
        div {
            class: "flex h-11 items-center gap-1 overflow-x-auto border-b border-line px-1 pl-4",
            "data-set-segment-row": "true",
            "data-segment-id": "{segment_id}",
            "data-segment-index": "{segment_index}",
            SegmentInputs {
                segment,
                set_id,
                mode,
                is_bodyweight,
                is_lateral,
                disabled,
                on_action,
            }
            button {
                class: "min-h-8 shrink-0 border border-line px-1 text-[10px] text-primary",
                "data-delete-segment": "true",
                disabled,
                onclick: move |_| on_action.call(SetAction::DeleteSegment {
                    set_id: delete_set_id.clone(),
                    segment_id: delete_segment_id.clone(),
                }),
                "{text(Key::RecordSetSegmentDelete)}"
            }
        }
    }
}

fn set_numbers(rows: &[SetView]) -> Vec<Option<usize>> {
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

fn kind_change_allowed(kinds: &[SetKind], index: usize, kind: SetKind) -> bool {
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

fn move_candidate_valid(kinds: &[SetKind], index: usize, delta: i32) -> bool {
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

fn detail_open_label(number_text: &str, is_drop: bool) -> String {
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
