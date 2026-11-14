//! セット詳細の drawer。セット番号を押すと開き、回数の内訳と RPE を編集する。
//! 時間で記録するセットは RPE だけを編集する。
//!
//! 対応する要求: `REQ-SET-006`、`REQ-SET-007`、`REQ-SET-010`。
//! ドロップセットは段ごとに反動と補助の回数を持ち、RPE は親セットに 1 つ持つ。

use dioxus::prelude::*;
use warrior_core::duration;
use warrior_core::exercise::RecordMode;

use super::set_inputs::Field;
use super::sets::{set_kind_text, SelectedSet, SetAction, SetView};
use crate::text::{text, Key};

/// セット詳細の drawer。
#[component]
pub(super) fn SetDrawer(
    rows: Vec<SetView>,
    mode: RecordMode,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let selected = use_context::<SelectedSet>();
    let selected_id = (selected.0)();
    let Some(set) = rows
        .iter()
        .find(|set| selected_id.as_deref() == Some(set.record.id.as_str()))
    else {
        return rsx! {};
    };
    let mut selected_set = selected;
    let set_id = set.record.id.clone();
    let rpe_set_id = set.record.id.clone();
    let locked = disabled || set.completed();
    let summary = summary_text(set, mode);
    rsx! {
        aside {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-set-drawer": "true",
            "data-set-id": "{set.record.id}",
            div { class: "flex items-start justify-between gap-2",
                div { class: "flex min-w-0 flex-col",
                    h2 { class: "text-sm font-bold", "{text(Key::RecordSetDetail)}" }
                    p {
                        class: "text-[11px] leading-4 text-text-sub",
                        "data-set-drawer-summary": "true",
                        "{summary}"
                    }
                }
                button {
                    class: "min-h-11 border border-line px-2 text-[11px]",
                    "data-set-drawer-close": "true",
                    onclick: move |_| selected_set.0.set(None),
                    "{text(Key::RecordSetDetailClose)}"
                }
            }
            if set.completed() {
                p {
                    class: "text-[11px] leading-4 text-text-sub",
                    "data-set-drawer-locked": "true",
                    "{text(Key::RecordSetCompletedLocked)}"
                }
            }
            if mode != RecordMode::Time {
                p { class: "text-[11px] font-bold", "{text(Key::RecordSetBreakdownLabel)}" }
                for (index, segment) in set.segments.iter().enumerate() {
                    BreakdownRow {
                        index,
                        set_id: set_id.clone(),
                        segment: segment.clone(),
                        disabled: locked,
                        on_action,
                    }
                }
                p {
                    class: "text-[10px] leading-4 text-text-sub",
                    "{text(Key::RecordSetBreakdownNote)}"
                }
            }
            RpeInput {
                set_id: rpe_set_id,
                value: set.rpe.clone(),
                disabled: locked,
                on_action,
            }
        }
    }
}

/// 1 つの段の反動と補助の回数の入力。
#[component]
fn BreakdownRow(
    index: usize,
    set_id: String,
    segment: super::sets::SegmentView,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let momentum_set_id = set_id.clone();
    let assisted_set_id = set_id;
    let momentum_segment_id = segment.record.id.clone();
    let assisted_segment_id = segment.record.id.clone();
    let momentum = segment.momentum.clone();
    let assisted = segment.assisted.clone();
    rsx! {
        div {
            class: "flex items-center gap-2",
            "data-set-drawer-segment": "{index}",
            span { class: "w-12 shrink-0 text-[11px] text-text-sub", "{index + 1} {text(Key::RecordSetSegment)}" }
            label {
                class: "flex items-center gap-1 text-[11px]",
                "{text(Key::RecordSetMomentum)}"
                input {
                    class: "min-h-11 w-14 border border-line bg-bg px-1 text-[13px]",
                    r#type: "number",
                    inputmode: "numeric",
                    min: "0",
                    step: "1",
                    "data-set-momentum": "true",
                    "aria-label": "{text(Key::RecordSetMomentum)}",
                    value: "{momentum}",
                    disabled,
                    oninput: move |event| on_action.call(SetAction::Input {
                        set_id: momentum_set_id.clone(),
                        segment_id: momentum_segment_id.clone(),
                        field: Field::Momentum,
                        value: event.value(),
                    }),
                }
            }
            label {
                class: "flex items-center gap-1 text-[11px]",
                "{text(Key::RecordSetAssisted)}"
                input {
                    class: "min-h-11 w-14 border border-line bg-bg px-1 text-[13px]",
                    r#type: "number",
                    inputmode: "numeric",
                    min: "0",
                    step: "1",
                    "data-set-assisted": "true",
                    "aria-label": "{text(Key::RecordSetAssisted)}",
                    value: "{assisted}",
                    disabled,
                    oninput: move |event| on_action.call(SetAction::Input {
                        set_id: assisted_set_id.clone(),
                        segment_id: assisted_segment_id.clone(),
                        field: Field::Assisted,
                        value: event.value(),
                    }),
                }
            }
        }
    }
}

/// セットの RPE の入力。
#[component]
fn RpeInput(
    set_id: String,
    value: String,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    rsx! {
        label {
            class: "flex items-center gap-2 text-[11px] font-bold",
            "{text(Key::RecordSetRpeLabel)}"
            input {
                class: "min-h-11 w-16 border border-line bg-bg px-1 text-[13px]",
                r#type: "number",
                inputmode: "decimal",
                min: "6",
                max: "10",
                step: "0.5",
                "data-set-rpe": "true",
                "aria-label": "{text(Key::RecordSetRpe)}",
                value: "{value}",
                disabled,
                oninput: move |event| on_action.call(SetAction::Rpe {
                    set_id: set_id.clone(),
                    value: event.value(),
                }),
            }
        }
    }
}

fn summary_text(set: &SetView, mode: RecordMode) -> String {
    let kind = set_kind_text(set.kind());
    let Some(segment) = set.segments.first() else {
        return kind.to_owned();
    };
    let detail = match mode {
        RecordMode::Time
            if !segment.left_duration.is_empty() || !segment.right_duration.is_empty() =>
        {
            format!(
                "{} {} / {} {}",
                text(Key::RecordSetLeft),
                duration_or_zero(&segment.left_duration),
                text(Key::RecordSetRight),
                duration_or_zero(&segment.right_duration)
            )
        }
        RecordMode::Time => duration_or_zero(&segment.duration),
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps
            if !segment.left_weight.is_empty() && !segment.right_weight.is_empty() =>
        {
            format!(
                "{} {} kg × {} / {} {} kg × {}",
                text(Key::RecordSetLeft),
                segment.left_weight,
                text_or_zero(&segment.left_reps),
                text(Key::RecordSetRight),
                segment.right_weight,
                text_or_zero(&segment.right_reps)
            )
        }
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => format!(
            "{} kg × {}",
            text_or_zero(&segment.weight),
            text_or_zero(&segment.reps)
        ),
    };
    format!("{kind}・{detail}")
}

fn text_or_zero(value: &str) -> String {
    if value.is_empty() {
        "0".to_owned()
    } else {
        value.to_owned()
    }
}

fn duration_or_zero(value: &str) -> String {
    duration::parse(value).map_or_else(|| "00:00".to_owned(), duration::format)
}
