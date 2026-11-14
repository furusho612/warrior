//! セットの行の入力欄。
//!
//! 時間で記録する種目は時間、左右別の種目は左右の値、それ以外は重量と回数を
//! 表示する。時間の入力にはストップウォッチを添える。入力は画面の状態だけを変え、
//! 保存は `frontend/src/screens/record/exercise_view.rs` が担う。

use dioxus::prelude::*;
use warrior_core::exercise::RecordMode;

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

/// 1 つの段の入力の欄。
#[component]
pub(super) fn SegmentInputs(
    segment: SegmentView,
    set_id: String,
    mode: RecordMode,
    is_bodyweight: bool,
    is_lateral: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    if mode == RecordMode::Time {
        if is_lateral {
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
    if is_lateral {
        let right_segment = segment.clone();
        let right_set_id = set_id.clone();
        return rsx! {
            SideFields {
                segment,
                set_id,
                side: "left",
                is_bodyweight,
                disabled,
                on_action,
            }
            SideFields {
                segment: right_segment,
                set_id: right_set_id,
                side: "right",
                is_bodyweight,
                disabled,
                on_action,
            }
        };
    }
    let weight_set_id = set_id.clone();
    let reps_set_id = set_id.clone();
    let weight_id = segment.record.id.clone();
    let reps_id = segment.record.id.clone();
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
    side: &'static str,
    is_bodyweight: bool,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let left = side == "left";
    let (weight, reps) = if left {
        (segment.left_weight.clone(), segment.left_reps.clone())
    } else {
        (segment.right_weight.clone(), segment.right_reps.clone())
    };
    let weight_field = if left {
        Field::LeftWeight
    } else {
        Field::RightWeight
    };
    let reps_field = if left {
        Field::LeftReps
    } else {
        Field::RightReps
    };
    let weight_set_id = set_id.clone();
    let reps_set_id = set_id;
    let weight_id = segment.record.id.clone();
    let reps_id = segment.record.id.clone();
    rsx! {
        input {
            class: "min-h-8 w-14 shrink-0 border border-line bg-bg px-1 text-[11px]",
            r#type: "number",
            inputmode: "decimal",
            min: "0",
            step: "0.5",
            placeholder: if is_bodyweight { text(Key::RecordSetAddedWeight) } else { text(Key::RecordExerciseWeight) },
            "data-set-side-weight": "true",
            "data-side": "{side}",
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
            "data-set-side-reps": "true",
            "data-side": "{side}",
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
