//! 記録のセットの表と、セットの行の状態。
//!
//! セットの行は端末内の保存の行と入力を一緒に持つ。入力は画面へすぐ反映し、
//! 保存は `frontend/src/screens/record/exercises.rs` が端末内の保存へ送る。
//! ドロップセットは段を 1 行ずつ表示し、段の行を親セットの直後に置く。
//! セットの集計は `warrior_core::set_calc` の `SetRecord` に変換して行う。

use dioxus::prelude::*;
use warrior_core::exercise::{GripSelection, RecordMode};
use warrior_core::previous::PreviousSet;
use warrior_core::profile::bodyweight_set_weight;
use warrior_core::set_calc::{BestKey, SetKind, SetRecord, SetSegment};
use warrior_core::set_rules;
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

use crate::storage::StoredSet;
use crate::text::{text, Key};
use crate::training_set::{SetSegmentValues, SetValues};

/// セットの入力の欄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    /// 重量または加重。
    Weight,
    /// 回数。
    Reps,
    /// 時間。
    Duration,
}

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
}

impl SegmentView {
    fn from_record(record: SetSegmentRecord) -> Self {
        Self {
            weight: number_text(record.weight_kg.unwrap_or(0.0)),
            reps: count_text(record.reps.unwrap_or(0)),
            duration: duration_text(record.duration_seconds),
            record,
        }
    }

    fn values(&self) -> SetSegmentValues {
        SetSegmentValues {
            weight_kg: parse_number(&self.weight).unwrap_or(0.0),
            reps: parse_count(&self.reps).unwrap_or(0),
            duration_seconds: parse_duration(&self.duration),
        }
    }

    fn can_complete(&self, mode: RecordMode) -> bool {
        match mode {
            RecordMode::Time => parse_duration(&self.duration).is_some(),
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                parse_count(&self.reps).is_some() && parse_number(&self.weight).is_some()
            }
        }
    }

    fn to_set_segment(&self, mode: RecordMode, body_weight_kg: Option<f64>) -> Option<SetSegment> {
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
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
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
    pub(super) fn values(&self) -> SetValues {
        SetValues {
            kind: self.kind(),
            segments: self
                .segments
                .iter()
                .map(SegmentView::values)
                .collect::<Vec<_>>(),
            completed: self.completed(),
            body_weight_kg: self.record.body_weight_kg,
            rpe: self.record.rpe,
            grip: grip_selection(&self.record),
        }
    }

    /// 完了できるか。時間の種目は時間、それ以外は回数が要る。
    pub(super) fn can_complete(&self, mode: RecordMode) -> bool {
        if self.completed() {
            return true;
        }
        if self.segments.is_empty() {
            return false;
        }
        self.segments
            .iter()
            .all(|segment| segment.can_complete(mode))
    }

    /// crate::set_calc のセットの記録に変換する。入力が足りないときは None。
    pub(super) fn to_record(&self, exercise_id: &str, mode: RecordMode) -> Option<SetRecord> {
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
                    .map(|segment| segment.to_set_segment(mode, self.record.body_weight_kg))
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
) -> Option<SetValues> {
    if previous.segments.is_empty() {
        return None;
    }
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
            SetSegmentValues {
                weight_kg: weight,
                reps: segment.reps,
                duration_seconds: duration,
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
                        can_delete: set_rules::delete_set_allowed(
                            rows.get(index + 1).map(SetView::kind),
                        ),
                        is_time,
                        is_bodyweight,
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
    can_delete: bool,
    is_time: bool,
    is_bodyweight: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let set_id = set.record.id.clone();
    let completed = set.completed();
    let mode = if is_time {
        RecordMode::Time
    } else if is_bodyweight {
        RecordMode::BodyweightAndReps
    } else {
        RecordMode::WeightAndReps
    };
    let ready = set.can_complete(mode);
    let kind = set.kind();
    let is_drop = kind == SetKind::Drop;
    let number_text = number.map_or_else(|| "D".to_owned(), |number| number.to_string());
    let segment_count = set.segments.len();
    let segments = segment_list_state(&set, is_drop, is_time, is_bodyweight, disabled);
    let buttons = RowButtonState {
        set_id: set.record.id.clone(),
        first_segment_id: set
            .segments
            .first()
            .map(|segment| segment.record.id.clone()),
        is_drop,
        can_move_up,
        can_move_down,
        can_delete,
        can_delete_first_segment: segment_count > 1 || can_delete,
        ready,
        completed,
    };
    let header = SetHeaderState {
        set_id: set_id.clone(),
        number_text: number_text.clone(),
        kind,
        is_drop,
        disabled,
        drop_allowed,
        warmup_allowed,
        first_segment: set.segments.first().cloned(),
        is_time,
        is_bodyweight,
        segment_count,
        buttons,
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
            SetHeader { header, on_action }
            if !can_delete {
                p {
                    class: "border-b border-line px-2 py-1 text-[10px] text-primary",
                    "data-set-delete-blocked": "true",
                    "{text(Key::RecordSetDeleteBlockedByDrop)}"
                }
            }
            SegmentList { list: segments, on_action }
        }
    }
}

/// セットの行の見出し。
#[derive(Debug, Clone, PartialEq)]
struct SetHeaderState {
    set_id: String,
    number_text: String,
    kind: SetKind,
    is_drop: bool,
    disabled: bool,
    drop_allowed: bool,
    warmup_allowed: bool,
    first_segment: Option<SegmentView>,
    is_time: bool,
    is_bodyweight: bool,
    segment_count: usize,
    buttons: RowButtonState,
}

/// セットの行の番号、種別、最初の段、操作のボタン。
#[component]
fn SetHeader(header: SetHeaderState, on_action: EventHandler<SetAction>) -> Element {
    let SetHeaderState {
        set_id,
        number_text,
        kind,
        is_drop,
        disabled,
        drop_allowed,
        warmup_allowed,
        first_segment,
        is_time,
        is_bodyweight,
        segment_count,
        buttons,
    } = header;
    let mut pending_kind = use_signal(|| Option::<SetKind>::None);
    let kind_set_id = set_id.clone();
    let on_kind = EventHandler::new(move |new_kind: SetKind| {
        if needs_segment_confirmation(kind, new_kind, segment_count) {
            pending_kind.set(Some(new_kind));
        } else {
            on_action.call(SetAction::Kind {
                set_id: kind_set_id.clone(),
                kind: new_kind,
            });
        }
    });
    let confirm_set_id = set_id.clone();
    let on_decide = EventHandler::new(move |decision: Option<SetKind>| {
        if let Some(kind) = decision {
            on_action.call(SetAction::Kind {
                set_id: confirm_set_id.clone(),
                kind,
            });
        }
        pending_kind.set(None);
    });
    rsx! {
        div {
            class: if is_drop { "flex h-11 items-center gap-1 overflow-x-auto border-b border-line px-1 pl-4" } else { "flex h-11 items-center gap-1 overflow-x-auto border-b border-line px-1" },
            span { class: "w-4 shrink-0 text-[11px] text-text-sub", "{number_text}" }
            KindSelect {
                kind,
                disabled,
                drop_allowed,
                warmup_allowed,
                on_kind,
            }
            if let Some(segment) = first_segment {
                div {
                    class: "flex items-center gap-1",
                    "data-segment-id": "{segment.record.id}",
                    "data-segment-index": "0",
                    SegmentInputs {
                        segment: segment.clone(),
                        set_id,
                        is_time,
                        is_bodyweight,
                        disabled,
                        on_action,
                    }
                }
            }
            RowButtons { buttons, disabled, on_action }
        }
        if let Some(pending) = (pending_kind)() {
            KindChangeConfirm { pending, on_decide }
        }
    }
}

/// セット種別の変更の確認。
#[component]
fn KindChangeConfirm(pending: SetKind, on_decide: EventHandler<Option<SetKind>>) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-1 border-b border-line px-2 py-1",
            p {
                class: "text-[10px] text-primary",
                "data-kind-change-confirm": "true",
                "{text(Key::RecordSetKindChangeConfirm)}"
            }
            div { class: "flex gap-1",
                button {
                    class: "min-h-8 border border-primary px-2 text-[10px] font-bold text-primary",
                    "data-kind-change-accept": "true",
                    onclick: move |_| on_decide.call(Some(pending)),
                    "{text(Key::RecordSetKindChangeAccept)}"
                }
                button {
                    class: "min-h-8 border border-line px-2 text-[10px]",
                    "data-kind-change-cancel": "true",
                    onclick: move |_| on_decide.call(None),
                    "{text(Key::RecordSetKindChangeCancel)}"
                }
            }
        }
    }
}

/// セットの段の一覧。
#[derive(Debug, Clone, PartialEq)]
struct SegmentListState {
    set_id: String,
    segments: Vec<SegmentView>,
    is_drop: bool,
    is_time: bool,
    is_bodyweight: bool,
    disabled: bool,
}

fn segment_list_state(
    set: &SetView,
    is_drop: bool,
    is_time: bool,
    is_bodyweight: bool,
    disabled: bool,
) -> SegmentListState {
    SegmentListState {
        set_id: set.record.id.clone(),
        segments: set.segments.clone(),
        is_drop,
        is_time,
        is_bodyweight,
        disabled,
    }
}

/// セットの 2 つ目以降の段の行と、段を追加するボタン。
#[component]
fn SegmentList(list: SegmentListState, on_action: EventHandler<SetAction>) -> Element {
    let SegmentListState {
        set_id,
        segments,
        is_drop,
        is_time,
        is_bodyweight,
        disabled,
    } = list;
    let add_segment_id = set_id.clone();
    rsx! {
        for (segment_index, segment) in segments.iter().enumerate().skip(1) {
            SegmentRow {
                set_id: set_id.clone(),
                segment: segment.clone(),
                segment_index,
                is_time,
                is_bodyweight,
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

/// セット種別の選択。
#[component]
fn KindSelect(
    kind: SetKind,
    disabled: bool,
    drop_allowed: bool,
    warmup_allowed: bool,
    on_kind: EventHandler<SetKind>,
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
                    on_kind.call(kind);
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
    /// セットを削除できるか。
    can_delete: bool,
    /// 最初の段を削除できるか。
    can_delete_first_segment: bool,
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
    let can_delete = buttons.can_delete;
    let can_delete_first_segment = buttons.can_delete_first_segment;
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
                    disabled: disabled || !can_delete_first_segment,
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
            disabled: disabled || !can_delete,
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
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
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
                is_time,
                is_bodyweight,
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

/// 1 つの段の入力の欄。
#[component]
fn SegmentInputs(
    segment: SegmentView,
    set_id: String,
    is_time: bool,
    is_bodyweight: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let weight_set_id = set_id.clone();
    let reps_set_id = set_id.clone();
    let duration_set_id = set_id;
    let weight_id = segment.record.id.clone();
    let reps_id = segment.record.id.clone();
    let duration_id = segment.record.id.clone();
    rsx! {
        if is_time {
            input {
                class: "min-h-8 w-16 shrink-0 border border-line bg-bg px-1 text-[11px]",
                r#type: "number",
                inputmode: "numeric",
                min: "1",
                step: "1",
                placeholder: "{text(Key::RecordExerciseDuration)}",
                "data-set-duration": "true",
                value: "{segment.duration}",
                disabled,
                oninput: move |event| on_action.call(SetAction::Input {
                    set_id: duration_set_id.clone(),
                    segment_id: duration_id.clone(),
                    field: Field::Duration,
                    value: event.value(),
                }),
            }
        } else {
            input {
                class: "min-h-8 w-14 shrink-0 border border-line bg-bg px-1 text-[11px]",
                r#type: "number",
                inputmode: "decimal",
                min: "0",
                step: "0.5",
                placeholder: if is_bodyweight { text(Key::RecordSetAddedWeight) } else { text(Key::RecordExerciseWeight) },
                "data-set-weight": "true",
                value: "{segment.weight}",
                disabled,
                oninput: move |event| on_action.call(SetAction::Input {
                    set_id: weight_set_id.clone(),
                    segment_id: weight_id.clone(),
                    field: Field::Weight,
                    value: event.value(),
                }),
            }
            input {
                class: "min-h-8 w-12 shrink-0 border border-line bg-bg px-1 text-[11px]",
                r#type: "number",
                inputmode: "numeric",
                min: "1",
                step: "1",
                placeholder: "{text(Key::RecordExerciseReps)}",
                "data-set-reps": "true",
                value: "{segment.reps}",
                disabled,
                oninput: move |event| on_action.call(SetAction::Input {
                    set_id: reps_set_id.clone(),
                    segment_id: reps_id.clone(),
                    field: Field::Reps,
                    value: event.value(),
                }),
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

fn needs_segment_confirmation(kind: SetKind, new_kind: SetKind, segment_count: usize) -> bool {
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
