//! 記録画面のセットの表と、セットの行。
//!
//! セットの行は入力欄と操作のボタンを持ち、種目の記録方式と左右の指定で入力欄を
//! 切り替える。入力の状態と保存は `frontend/src/screens/record/sets.rs` と
//! `frontend/src/screens/record/exercise_view.rs` が持つ。

use dioxus::prelude::*;
use warrior_core::exercise::{Laterality, RecordMode};
use warrior_core::set_calc::SetKind;
use warrior_core::set_rules;

use super::one_rep_max::OneRepMaxBadge;
use super::set_inputs::{SegmentInputs, SegmentInputsState};
use super::sets::{
    detail_open_label, is_lateral, kind_change_allowed, move_candidate_valid,
    needs_segment_confirmation, set_numbers, SegmentView, SelectedSet, SetAction, SetView,
};
use crate::text::{text, Key};

/// セットの表の表示に必要な状態。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SetTableState {
    /// 種目の行の ID。
    pub(super) view_id: String,
    /// セットの行。
    pub(super) rows: Vec<SetView>,
    /// 記録方式。
    pub(super) mode: RecordMode,
    /// 左右別に行うか。
    pub(super) laterality: Laterality,
    /// 編集できないか。
    pub(super) disabled: bool,
}

/// セットの表。
#[component]
pub(super) fn SetTable(state: SetTableState, on_action: EventHandler<SetAction>) -> Element {
    let add = on_action;
    let add_view_id = state.view_id.clone();
    let kinds = state.rows.iter().map(SetView::kind).collect::<Vec<_>>();
    let numbers = set_numbers(&state.rows);
    let display_rows = state
        .rows
        .iter()
        .map(|row| row.segments.len())
        .sum::<usize>();
    let lateral = is_lateral(state.laterality, state.mode);
    let has_completed = state.rows.iter().any(SetView::completed);
    let rows = state.rows;
    let disabled = state.disabled;
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
                        key: "{row.record.id}",
                        row: SetRowState {
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
                            mode: state.mode,
                            lateral,
                            disabled,
                        },
                        on_action,
                    }
                }
            }
            if disabled {
                p { class: "text-[11px] text-text-sub", "{text(Key::RecordViewOnly)}" }
            }
            if has_completed {
                p {
                    class: "text-[11px] text-text-sub",
                    "data-set-completed-locked": "true",
                    "{text(Key::RecordSetCompletedLocked)}"
                }
            }
        }
    }
}

/// セットの表の 1 行の状態。
#[derive(Debug, Clone, PartialEq)]
struct SetRowState {
    /// 表の中の並び。
    index: usize,
    /// セットの行。
    set: SetView,
    /// セットの番号。ドロップセットは None。
    number: Option<usize>,
    /// ドロップセットにできるか。
    drop_allowed: bool,
    /// ウォームアップにできるか。
    warmup_allowed: bool,
    /// 前へ移動できるか。
    can_move_up: bool,
    /// 後ろへ移動できるか。
    can_move_down: bool,
    /// セットを削除できるか。
    can_delete: bool,
    /// 記録方式。
    mode: RecordMode,
    /// 左右の列を使うか。
    lateral: bool,
    /// 編集できないか。
    disabled: bool,
}

/// セットの表の 1 行。
#[component]
fn SetRow(row: SetRowState, on_action: EventHandler<SetAction>) -> Element {
    let set_id = row.set.record.id.clone();
    let add_segment_id = row.set.record.id.clone();
    let mut selected_set = use_context::<SelectedSet>();
    let open_id = set_id.clone();
    let completed = row.set.completed();
    let ready = row.set.can_complete(row.mode, row.lateral);
    let kind = row.set.kind();
    let is_drop = kind == SetKind::Drop;
    let segment_count = row.set.segments.len();
    let can_delete = row.can_delete;
    let can_delete_first_segment = segment_count > 1 || can_delete;
    let number_text = row
        .number
        .map_or_else(|| "D".to_owned(), |number| number.to_string());
    let open_label = detail_open_label(&number_text, is_drop);
    let first_segment = row.set.segments.first().cloned();
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
    let buttons = RowButtonState {
        set_id: row.set.record.id.clone(),
        first_segment_id: row
            .set
            .segments
            .first()
            .map(|segment| segment.record.id.clone()),
        is_drop,
        can_move_up: row.can_move_up,
        can_move_down: row.can_move_down,
        can_delete,
        can_delete_first_segment,
        ready,
        completed,
    };
    let disabled = row.disabled;
    let locked = disabled || completed;
    let confirm_set_id = set_id.clone();
    rsx! {
        div {
            class: "flex flex-col",
            "data-set-row": "true",
            "data-set-row-index": "{row.index}",
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
                    kind,
                    disabled: locked,
                    drop_allowed: row.drop_allowed,
                    warmup_allowed: row.warmup_allowed,
                    on_kind,
                }
                if let Some(segment) = first_segment {
                    div {
                        class: "flex items-center gap-1",
                        "data-segment-id": "{segment.record.id}",
                        "data-segment-index": "0",
                        SegmentInputs {
                            state: SegmentInputsState {
                                segment: segment.clone(),
                                set_id: set_id.clone(),
                                mode: row.mode,
                                lateral: row.lateral,
                                disabled: locked,
                            },
                            on_action,
                        }
                    }
                }
                RowButtons { buttons, disabled, on_action }
                OneRepMaxBadge { value: row.set.one_rep_max(row.mode, row.lateral) }
            }
            if !can_delete {
                p {
                    class: "border-b border-line px-2 py-1 text-[10px] text-primary",
                    "data-set-delete-blocked": "true",
                    "{text(Key::RecordSetDeleteBlockedByDrop)}"
                }
            }
            if let Some(pending) = (pending_kind)() {
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
                            onclick: move |_| {
                                on_action.call(SetAction::Kind {
                                    set_id: confirm_set_id.clone(),
                                    kind: pending,
                                });
                                pending_kind.set(None);
                            },
                            "{text(Key::RecordSetKindChangeAccept)}"
                        }
                        button {
                            class: "min-h-8 border border-line px-2 text-[10px]",
                            "data-kind-change-cancel": "true",
                            onclick: move |_| pending_kind.set(None),
                            "{text(Key::RecordSetKindChangeCancel)}"
                        }
                    }
                }
            }
            for (segment_index, segment) in row.set.segments.iter().enumerate().skip(1) {
                SegmentRow {
                    key: "{segment.record.id}",
                    row: SegmentRowState {
                        set_id: set_id.clone(),
                        segment: segment.clone(),
                        segment_index,
                        mode: row.mode,
                        lateral: row.lateral,
                        disabled: locked,
                    },
                    on_action,
                }
            }
            if is_drop && row.mode != RecordMode::Time {
                button {
                    class: "min-h-9 border-t border-line px-2 text-left text-[10px] font-bold text-primary",
                    "data-add-segment": "true",
                    disabled: locked,
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

/// セットの行の操作のボタンの状態。
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
            disabled: disabled || completed || !ready,
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
                    disabled: disabled || completed || !can_delete_first_segment,
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
            disabled: disabled || completed || !can_delete,
            onclick: move |_| on_action.call(SetAction::Delete { set_id: delete_id.clone() }),
            "{text(Key::RecordSetDelete)}"
        }
    }
}

/// セットの表の、2 つ目以降の段の行の状態。
#[derive(Debug, Clone, PartialEq)]
struct SegmentRowState {
    /// セットの ID。
    set_id: String,
    /// 段の状態。
    segment: SegmentView,
    /// セットの中の段の並び。
    segment_index: usize,
    /// 記録方式。
    mode: RecordMode,
    /// 左右の列を使うか。
    lateral: bool,
    /// 編集できないか。
    disabled: bool,
}

/// セットの表の、2 つ目以降の段の行。
#[component]
fn SegmentRow(row: SegmentRowState, on_action: EventHandler<SetAction>) -> Element {
    let delete_set_id = row.set_id.clone();
    let delete_segment_id = row.segment.record.id.clone();
    let segment_id = row.segment.record.id.clone();
    rsx! {
        div {
            class: "flex h-11 items-center gap-1 overflow-x-auto border-b border-line px-1 pl-4",
            "data-set-segment-row": "true",
            "data-segment-id": "{segment_id}",
            "data-segment-index": "{row.segment_index}",
            SegmentInputs {
                state: SegmentInputsState {
                    segment: row.segment,
                    set_id: row.set_id,
                    mode: row.mode,
                    lateral: row.lateral,
                    disabled: row.disabled,
                },
                on_action,
            }
            button {
                class: "min-h-8 shrink-0 border border-line px-1 text-[10px] text-primary",
                "data-delete-segment": "true",
                disabled: row.disabled,
                onclick: move |_| on_action.call(SetAction::DeleteSegment {
                    set_id: delete_set_id.clone(),
                    segment_id: delete_segment_id.clone(),
                }),
                "{text(Key::RecordSetSegmentDelete)}"
            }
        }
    }
}
