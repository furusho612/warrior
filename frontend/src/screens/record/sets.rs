//! 記録のセットの表と、セットの行の状態。
//!
//! セットの行は端末内の保存の行と入力を一緒に持つ。入力は画面へすぐ反映し、
//! 保存は `frontend/src/screens/record/exercises.rs` が端末内の保存へ送る。
//! セットの集計は `warrior_core::set_calc` の `SetRecord` に変換して行う。

use dioxus::prelude::*;
use warrior_core::exercise::{GripSelection, RecordMode};
use warrior_core::previous::PreviousSet;
use warrior_core::profile::bodyweight_set_weight;
use warrior_core::set_calc::{BestKey, SetKind, SetRecord, SetSegment};
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

use crate::storage::StoredSet;
use crate::text::{text, Key};
use crate::training_set::SetValues;

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
    /// セットの順序を変える。
    Move {
        /// セットの ID。
        set_id: String,
        /// 移動する向き。前が -1、後ろが 1。
        delta: i32,
    },
}

/// 画面が読む 1 つのセット。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SetView {
    /// 端末内の保存の行。
    pub(super) record: SetRecordRow,
    /// 端末内の保存の段。
    pub(super) segment: SetSegmentRecord,
    /// 重量または加重の入力。
    pub(super) weight: String,
    /// 回数の入力。
    pub(super) reps: String,
    /// 時間の入力。
    pub(super) duration: String,
    /// 入力の版。保存の完了を判定する。
    pub(super) revision: u64,
}

impl SetView {
    /// 端末内の保存の行から画面の行を作る。
    pub(super) fn from_stored(stored: StoredSet) -> Option<Self> {
        let segment = stored.segments.first()?.clone();
        Some(Self {
            weight: number_text(segment.weight_kg.unwrap_or(0.0)),
            reps: count_text(segment.reps.unwrap_or(0)),
            duration: duration_text(segment.duration_seconds),
            record: stored.set,
            segment,
            revision: 0,
        })
    }

    /// 保存した行から画面の行を作る。
    pub(super) fn from_rows(record: SetRecordRow, segment: SetSegmentRecord) -> Self {
        Self {
            weight: number_text(segment.weight_kg.unwrap_or(0.0)),
            reps: count_text(segment.reps.unwrap_or(0)),
            duration: duration_text(segment.duration_seconds),
            record,
            segment,
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
            weight_kg: parse_number(&self.weight).unwrap_or(0.0),
            reps: parse_count(&self.reps).unwrap_or(0),
            duration_seconds: parse_duration(&self.duration),
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
        match mode {
            RecordMode::Time => parse_duration(&self.duration).is_some(),
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                parse_count(&self.reps).is_some() && parse_number(&self.weight).is_some()
            }
        }
    }

    /// 0012 のセットの記録に変換する。入力が足りないときは None。
    pub(super) fn to_record(&self, exercise_id: &str, mode: RecordMode) -> Option<SetRecord> {
        let kind = self.kind();
        let key = BestKey {
            exercise_id: exercise_id.to_owned(),
            grip: grip_selection(&self.record)
                .map(|grip| grip.comparison_key().0)
                .unwrap_or(None),
            attachment: grip_selection(&self.record)
                .map(|grip| grip.comparison_key().1)
                .unwrap_or(None),
        };
        match mode {
            RecordMode::Time => {
                let duration = parse_duration(&self.duration)?;
                SetRecord::new(key, kind, Vec::new(), self.record.rpe, Some(duration), mode).ok()
            }
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                let weight = parse_number(&self.weight)?;
                let reps = parse_count(&self.reps)?;
                let weight = match mode {
                    RecordMode::BodyweightAndReps => {
                        bodyweight_set_weight(self.record.body_weight_kg, weight).unwrap_or(0.0)
                    }
                    RecordMode::WeightAndReps | RecordMode::Time => weight,
                };
                SetRecord::new(
                    key,
                    kind,
                    vec![SetSegment {
                        weight_kg: weight,
                        reps,
                        momentum_reps: 0,
                        assisted_reps: 0,
                        left: None,
                        right: None,
                        duration_seconds: None,
                    }],
                    self.record.rpe,
                    None,
                    mode,
                )
                .ok()
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
/// 自重種目の前回の重量は記録した時点の体重を含むため、いまの体重を引いて加重にする。
pub(super) fn previous_values(
    previous: &PreviousSet,
    mode: RecordMode,
    body_weight_kg: Option<f64>,
) -> Option<SetValues> {
    let segment = previous.segments.first()?;
    let weight = match mode {
        RecordMode::BodyweightAndReps => {
            let body = body_weight_kg.unwrap_or(0.0);
            (segment.weight_kg - body).max(0.0)
        }
        RecordMode::WeightAndReps => segment.weight_kg,
        RecordMode::Time => 0.0,
    };
    Some(SetValues {
        kind: previous.kind,
        weight_kg: weight,
        reps: segment.reps,
        duration_seconds: match mode {
            RecordMode::Time => {
                let duration = segment.longest_duration_seconds();
                (duration > 0).then_some(duration)
            }
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => None,
        },
        body_weight_kg,
        rpe: previous.rpe,
        grip: previous.grip,
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
                class: "max-h-[444px] overflow-y-auto border border-line bg-surface",
                "data-set-table": "true",
                "data-set-rows": "{rows.len()}",
                "data-set-row-height": "44",
                if rows.is_empty() {
                    p {
                        class: "p-2 text-[11px] text-text-sub",
                        "data-set-empty": "true",
                        "{text(Key::RecordExerciseEmpty)}"
                    }
                }
                for (index, row) in rows.iter().enumerate() {
                    {
                        let set_id = row.record.id.clone();
                        let kind_id = row.record.id.clone();
                        let complete_id = row.record.id.clone();
                        let delete_id = row.record.id.clone();
                        let up_id = row.record.id.clone();
                        let down_id = row.record.id.clone();
                        let weight_id = row.record.id.clone();
                        let reps_id = row.record.id.clone();
                        let duration_id = row.record.id.clone();
                        let first = index == 0;
                        let last = index + 1 == rows.len();
                        let completed = row.completed();
                        let ready = row.can_complete(if is_time { RecordMode::Time } else if is_bodyweight { RecordMode::BodyweightAndReps } else { RecordMode::WeightAndReps });
                        let kind = row.kind();
                        rsx! {
                            div {
                                class: "flex flex-col gap-1 border-b border-line p-2",
                                "data-set-row": "true",
                                "data-set-row-index": "{index}",
                                "data-set-id": "{set_id}",
                                "data-set-kind": "{kind.as_str()}",
                                "data-set-completed": if completed { "true" } else { "false" },
                                div { class: "flex flex-wrap items-center gap-1",
                                    span { class: "w-5 shrink-0 text-[11px] text-text-sub", "{index + 1}" }
                                    select {
                                        class: "min-h-8 border border-line bg-bg px-1 text-[11px]",
                                        "data-set-kind": "true",
                                        "aria-label": "{text(Key::RecordSetKindLabel)}",
                                        disabled,
                                        value: "{kind.as_str()}",
                                        onchange: move |event| {
                                            if let Some(kind) = SetKind::parse(&event.value()) {
                                                on_action.call(SetAction::Kind { set_id: kind_id.clone(), kind });
                                            }
                                        },
                                        option { value: "normal", selected: kind == SetKind::Normal, "{text(Key::CalendarSetKindNormal)}" }
                                        option { value: "warmup", selected: kind == SetKind::Warmup, "{text(Key::CalendarSetKindWarmup)}" }
                                        option { value: "drop", selected: kind == SetKind::Drop, "{text(Key::CalendarSetKindDrop)}" }
                                    }
                                    button {
                                        class: "min-h-8 border border-line px-1 text-[11px]",
                                        "data-set-move-up": "true",
                                        disabled: disabled || first,
                                        onclick: move |_| on_action.call(SetAction::Move { set_id: up_id.clone(), delta: -1 }),
                                        "{text(Key::RecordExerciseMoveUp)}"
                                    }
                                    button {
                                        class: "min-h-8 border border-line px-1 text-[11px]",
                                        "data-set-move-down": "true",
                                        disabled: disabled || last,
                                        onclick: move |_| on_action.call(SetAction::Move { set_id: down_id.clone(), delta: 1 }),
                                        "{text(Key::RecordExerciseMoveDown)}"
                                    }
                                    button {
                                        class: "min-h-8 border border-line px-1 text-[11px] text-primary",
                                        "data-delete-set": "true",
                                        disabled,
                                        onclick: move |_| on_action.call(SetAction::Delete { set_id: delete_id.clone() }),
                                        "{text(Key::RecordSetDelete)}"
                                    }
                                    button {
                                        class: "min-h-8 border border-primary px-1 text-[11px] font-bold text-primary",
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
                                }
                                if is_time {
                                    input {
                                        class: "min-h-9 w-full border border-line bg-bg px-2 text-[13px]",
                                        r#type: "number",
                                        inputmode: "numeric",
                                        min: "1",
                                        step: "1",
                                        placeholder: "{text(Key::RecordExerciseDuration)}",
                                        "data-set-duration": "true",
                                        value: "{row.duration}",
                                        disabled,
                                        oninput: move |event| on_action.call(SetAction::Input {
                                            set_id: duration_id.clone(),
                                            field: Field::Duration,
                                            value: event.value(),
                                        }),
                                    }
                                } else {
                                    input {
                                        class: "min-h-9 w-1/2 border border-line bg-bg px-2 text-[13px]",
                                        r#type: "number",
                                        inputmode: "decimal",
                                        min: "0",
                                        step: "0.5",
                                        placeholder: if is_bodyweight { text(Key::RecordSetAddedWeight) } else { text(Key::RecordExerciseWeight) },
                                        "data-set-weight": "true",
                                        value: "{row.weight}",
                                        disabled,
                                        oninput: move |event| on_action.call(SetAction::Input {
                                            set_id: weight_id.clone(),
                                            field: Field::Weight,
                                            value: event.value(),
                                        }),
                                    }
                                    input {
                                        class: "min-h-9 w-1/2 border border-line bg-bg px-2 text-[13px]",
                                        r#type: "number",
                                        inputmode: "numeric",
                                        min: "1",
                                        step: "1",
                                        placeholder: "{text(Key::RecordExerciseReps)}",
                                        "data-set-reps": "true",
                                        value: "{row.reps}",
                                        disabled,
                                        oninput: move |event| on_action.call(SetAction::Input {
                                            set_id: reps_id.clone(),
                                            field: Field::Reps,
                                            value: event.value(),
                                        }),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if disabled {
                p { class: "text-[11px] text-text-sub", "{text(Key::RecordViewOnly)}" }
            }
        }
    }
}

/// セット種別の文言。
pub(super) fn set_kind_text(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Normal => text(Key::CalendarSetKindNormal),
        SetKind::Warmup => text(Key::CalendarSetKindWarmup),
        SetKind::Drop => text(Key::CalendarSetKindDrop),
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
