//! セット詳細の drawer。セット番号を押すと開き、グリップ、回数の内訳と RPE を編集する。
//! 時間で記録するセットは RPE だけを編集する。
//!
//! 対応する要求: `REQ-SET-006`、`REQ-SET-007`、`REQ-SET-010`、`REQ-SET-011`、`REQ-LIB-011`。
//! ドロップセットは段ごとに反動と補助の回数を持ち、RPE は親セットに 1 つ持つ。
//! グリップは種目スナップショットの選択肢から選び、セットごとに持つ。

use dioxus::prelude::*;
use warrior_core::duration;
use warrior_core::exercise::{
    Attachment, GripMode, GripSelection, GripStyle, GripWidth, RecordMode,
};

use super::set_inputs::Field;
use super::sets::{
    attachment_label, attachment_styles, attachment_with, attachment_with_style, default_grip,
    grip_attachments, grip_style_label, grip_styles, grip_width_label, grip_widths,
    grip_with_style, grip_with_width, set_kind_text, SelectedSet, SetAction, SetView,
};
use crate::text::{text, Key};

/// セット詳細の drawer。
#[component]
pub(super) fn SetDrawer(
    rows: Vec<SetView>,
    mode: RecordMode,
    grip_mode: GripMode,
    grip_options: Vec<GripSelection>,
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
            if grip_mode != GripMode::None {
                GripInput {
                    set_id: set.record.id.clone(),
                    grip_mode,
                    grip_options: grip_options.clone(),
                    selection: set.grip(),
                    disabled: locked,
                    on_action,
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

/// セットのグリップの選択。
#[component]
fn GripInput(
    set_id: String,
    grip_mode: GripMode,
    grip_options: Vec<GripSelection>,
    selection: Option<GripSelection>,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let Some(selection) = default_grip(grip_mode, &grip_options, selection) else {
        return rsx! {};
    };
    match grip_mode {
        GripMode::None => rsx! {},
        GripMode::Grip => {
            let GripSelection::Grip { style, width } = selection else {
                return rsx! {};
            };
            let styles = grip_styles(&grip_options);
            let widths = grip_widths(&grip_options, style);
            let style_set_id = set_id.clone();
            let width_set_id = set_id;
            let style_options = grip_options.clone();
            let width_options = grip_options;
            rsx! {
                div {
                    class: "flex flex-wrap items-center gap-2",
                    "data-set-grip-input": "true",
                    label {
                        class: "flex items-center gap-1 text-[11px] font-bold",
                        "{text(Key::RecordSetGripStyle)}"
                        select {
                            class: "min-h-11 border border-line bg-bg px-1 text-[12px]",
                            "data-set-grip-style": "true",
                            "aria-label": "{text(Key::RecordSetGripStyle)}",
                            disabled,
                            value: "{style.as_str()}",
                            onchange: move |event| {
                                if let Some(style) = grip_style_from_value(&event.value()) {
                                    on_action.call(SetAction::Grip {
                                        set_id: style_set_id.clone(),
                                        grip: grip_with_style(selection, style, &style_options),
                                    });
                                }
                            },
                            for option in styles {
                                option {
                                    value: "{option.as_str()}",
                                    selected: option == style,
                                    "{grip_style_label(option)}"
                                }
                            }
                        }
                    }
                    label {
                        class: "flex items-center gap-1 text-[11px] font-bold",
                        "{text(Key::RecordSetGripWidth)}"
                        select {
                            class: "min-h-11 border border-line bg-bg px-1 text-[12px]",
                            "data-set-grip-width": "true",
                            "aria-label": "{text(Key::RecordSetGripWidth)}",
                            disabled,
                            value: "{width.as_str()}",
                            onchange: move |event| {
                                if let Some(width) = grip_width_from_value(&event.value()) {
                                    on_action.call(SetAction::Grip {
                                        set_id: width_set_id.clone(),
                                        grip: grip_with_width(selection, width, &width_options),
                                    });
                                }
                            },
                            for option in widths {
                                option {
                                    value: "{option.as_str()}",
                                    selected: option == width,
                                    "{grip_width_label(option)}"
                                }
                            }
                        }
                    }
                }
            }
        }
        GripMode::Attachment => {
            let GripSelection::Attachment { attachment, style } = selection else {
                return rsx! {};
            };
            let attachments = grip_attachments(&grip_options);
            let styles = attachment_styles(&grip_options, attachment);
            let current_style = style.or_else(|| styles.first().copied());
            let attachment_set_id = set_id.clone();
            let style_set_id = set_id;
            let attachment_options = grip_options.clone();
            let style_options = grip_options;
            rsx! {
                div {
                    class: "flex flex-wrap items-center gap-2",
                    "data-set-grip-input": "true",
                    label {
                        class: "flex items-center gap-1 text-[11px] font-bold",
                        "{text(Key::RecordSetGripAttachment)}"
                        select {
                            class: "min-h-11 border border-line bg-bg px-1 text-[12px]",
                            "data-set-grip-attachment": "true",
                            "aria-label": "{text(Key::RecordSetGripAttachment)}",
                            disabled,
                            value: "{attachment.as_str()}",
                            onchange: move |event| {
                                if let Some(attachment) = attachment_from_value(&event.value()) {
                                    on_action.call(SetAction::Grip {
                                        set_id: attachment_set_id.clone(),
                                        grip: attachment_with(selection, attachment, &attachment_options),
                                    });
                                }
                            },
                            for option in attachments {
                                option {
                                    value: "{option.as_str()}",
                                    selected: option == attachment,
                                    "{attachment_label(option)}"
                                }
                            }
                        }
                    }
                    if let Some(current_style) = current_style {
                        label {
                            class: "flex items-center gap-1 text-[11px] font-bold",
                            "{text(Key::RecordSetGripStyle)}"
                            select {
                                class: "min-h-11 border border-line bg-bg px-1 text-[12px]",
                                "data-set-grip-style": "true",
                                "aria-label": "{text(Key::RecordSetGripStyle)}",
                                disabled,
                                value: "{current_style.as_str()}",
                                onchange: move |event| {
                                    if let Some(style) = grip_style_from_value(&event.value()) {
                                        on_action.call(SetAction::Grip {
                                            set_id: style_set_id.clone(),
                                            grip: attachment_with_style(selection, style, &style_options),
                                        });
                                    }
                                },
                                for option in styles {
                                    option {
                                        value: "{option.as_str()}",
                                        selected: option == current_style,
                                        "{grip_style_label(option)}"
                                    }
                                }
                            }
                        }
                    }
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

fn grip_style_from_value(value: &str) -> Option<GripStyle> {
    GripStyle::ALL
        .iter()
        .copied()
        .find(|style| style.as_str() == value)
}

fn grip_width_from_value(value: &str) -> Option<GripWidth> {
    GripWidth::ALL
        .iter()
        .copied()
        .find(|width| width.as_str() == value)
}

fn attachment_from_value(value: &str) -> Option<Attachment> {
    Attachment::ALL
        .iter()
        .copied()
        .find(|attachment| attachment.as_str() == value)
}
