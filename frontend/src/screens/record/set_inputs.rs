//! セットの行の入力欄。
//!
//! 時間で記録する種目は時間、左右別の種目は左右の値、それ以外は重量と回数を
//! 表示する。時間の入力にはストップウォッチを添える。入力は画面の状態だけを変え、
//! 保存は `frontend/src/screens/record/exercise_view/operations.rs` が担う。

use dioxus::prelude::*;
use warrior_core::exercise::RecordMode;
use warrior_core::set_calc::Side;

use super::sets::{SegmentView, SetAction};
use super::stopwatch::StopwatchControl;
use crate::storage::StopwatchSide;
use crate::text::{text, Key};

/// セットの入力の欄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    /// 重量または加重。
    Weight,
    /// 回数。
    Reps,
    /// 反動を使った回数。
    Momentum,
    /// 補助を受けた回数。
    Assisted,
    /// 時間。
    Duration,
    /// 左の重量または加重。
    LeftWeight,
    /// 左の回数。
    LeftReps,
    /// 右の重量または加重。
    RightWeight,
    /// 右の回数。
    RightReps,
    /// 左の時間。
    LeftDuration,
    /// 右の時間。
    RightDuration,
}

/// セットの段の入力に必要な状態。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SegmentInputsState {
    /// 段の状態。
    pub(super) segment: SegmentView,
    /// セットの ID。
    pub(super) set_id: String,
    /// 記録方式。
    pub(super) mode: RecordMode,
    /// 左右の列を使うか。
    pub(super) lateral: bool,
    /// 編集できないか。
    pub(super) disabled: bool,
}

/// 1 つの段の入力の欄。
#[component]
pub(super) fn SegmentInputs(
    state: SegmentInputsState,
    on_action: EventHandler<SetAction>,
) -> Element {
    let SegmentInputsState {
        segment,
        set_id,
        mode,
        lateral,
        disabled,
    } = state;
    if mode == RecordMode::Time {
        if lateral {
            let right_segment = segment.clone();
            let right_set_id = set_id.clone();
            return rsx! {
                TimeInput {
                    segment,
                    set_id,
                    side: StopwatchSide::Left,
                    field: Field::LeftDuration,
                    disabled,
                    on_action,
                }
                TimeInput {
                    segment: right_segment,
                    set_id: right_set_id,
                    side: StopwatchSide::Right,
                    field: Field::RightDuration,
                    disabled,
                    on_action,
                }
            };
        }
        return rsx! {
            TimeInput {
                segment,
                set_id,
                side: StopwatchSide::Whole,
                field: Field::Duration,
                disabled,
                on_action,
            }
        };
    }
    if lateral {
        let right_segment = segment.clone();
        let right_set_id = set_id.clone();
        return rsx! {
            SideFields {
                segment,
                set_id,
                side: Side::Left,
                mode,
                disabled,
                on_action,
            }
            SideFields {
                segment: right_segment,
                set_id: right_set_id,
                side: Side::Right,
                mode,
                disabled,
                on_action,
            }
        };
    }
    let weight_set_id = set_id.clone();
    let reps_set_id = set_id;
    let weight_id = segment.record.id.clone();
    let reps_id = segment.record.id.clone();
    let is_bodyweight = mode == RecordMode::BodyweightAndReps;
    rsx! {
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

/// 左右別の種目の、片側の重量と回数の入力。
#[component]
fn SideFields(
    segment: SegmentView,
    set_id: String,
    side: Side,
    mode: RecordMode,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let left = side == Side::Left;
    let (weight, reps) = if left {
        (segment.left_weight.clone(), segment.left_reps.clone())
    } else {
        (segment.right_weight.clone(), segment.right_reps.clone())
    };
    let (weight_field, reps_field) = if left {
        (Field::LeftWeight, Field::LeftReps)
    } else {
        (Field::RightWeight, Field::RightReps)
    };
    let (label, weight_key, reps_key, side_name) = match side {
        Side::Left => (
            Key::RecordSetLeft,
            Key::RecordSetLeftWeight,
            Key::RecordSetLeftReps,
            "left",
        ),
        Side::Right => (
            Key::RecordSetRight,
            Key::RecordSetRightWeight,
            Key::RecordSetRightReps,
            "right",
        ),
    };
    let is_bodyweight = mode == RecordMode::BodyweightAndReps;
    let weight_set_id = set_id.clone();
    let reps_set_id = set_id;
    let weight_id = segment.record.id.clone();
    let reps_id = segment.record.id.clone();
    rsx! {
        span {
            class: "shrink-0 text-[10px] text-text-sub",
            "data-set-side-label": "{side_name}",
            "{text(label)}"
        }
        input {
            class: "min-h-8 w-14 shrink-0 border border-line bg-bg px-1 text-[11px]",
            r#type: "number",
            inputmode: "decimal",
            min: "0",
            step: "0.5",
            placeholder: if is_bodyweight { text(Key::RecordSetAddedWeight) } else { text(Key::RecordExerciseWeight) },
            "aria-label": "{text(weight_key)}",
            "data-set-side-weight": "true",
            "data-side": "{side_name}",
            value: "{weight}",
            disabled,
            oninput: move |event| on_action.call(SetAction::Input {
                set_id: weight_set_id.clone(),
                segment_id: weight_id.clone(),
                field: weight_field,
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
            "aria-label": "{text(reps_key)}",
            "data-set-side-reps": "true",
            "data-side": "{side_name}",
            value: "{reps}",
            disabled,
            oninput: move |event| on_action.call(SetAction::Input {
                set_id: reps_set_id.clone(),
                segment_id: reps_id.clone(),
                field: reps_field,
                value: event.value(),
            }),
        }
    }
}

/// 時間で記録する種目の入力。
#[component]
fn TimeInput(
    segment: SegmentView,
    set_id: String,
    side: StopwatchSide,
    field: Field,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let value = match side {
        StopwatchSide::Whole => segment.duration.clone(),
        StopwatchSide::Left => segment.left_duration.clone(),
        StopwatchSide::Right => segment.right_duration.clone(),
    };
    let side_name = match side {
        StopwatchSide::Whole => "whole",
        StopwatchSide::Left => "left",
        StopwatchSide::Right => "right",
    };
    let label = match side {
        StopwatchSide::Whole => text(Key::RecordExerciseDuration).to_owned(),
        StopwatchSide::Left => format!(
            "{} {}",
            text(Key::RecordSetLeft),
            text(Key::RecordExerciseDuration)
        ),
        StopwatchSide::Right => format!(
            "{} {}",
            text(Key::RecordSetRight),
            text(Key::RecordExerciseDuration)
        ),
    };
    let duration_id = segment.record.id.clone();
    let input_set_id = set_id.clone();
    let stopwatch_segment_id = segment.record.id.clone();
    rsx! {
        input {
            class: "min-h-8 w-20 shrink-0 border border-line bg-bg px-1 text-[11px]",
            r#type: "text",
            inputmode: "numeric",
            placeholder: "{text(Key::RecordExerciseDurationPlaceholder)}",
            "data-set-duration": if side == StopwatchSide::Whole { "true" } else { "false" },
            "data-set-side-duration": if side == StopwatchSide::Whole { "false" } else { "true" },
            "data-side": "{side_name}",
            "aria-label": "{label}",
            value: "{value}",
            disabled,
            oninput: move |event| on_action.call(SetAction::Input {
                set_id: input_set_id.clone(),
                segment_id: duration_id.clone(),
                field,
                value: event.value(),
            }),
        }
        StopwatchControl {
            set_id,
            segment_id: stopwatch_segment_id,
            side,
            field,
            disabled,
            on_action,
        }
    }
}
