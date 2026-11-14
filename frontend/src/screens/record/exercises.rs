//! 記録の種目の一覧、追加、並べ替え、削除と、前回の記録の表示、コピー、差。
//! スーパーセットの組の作成と解除もここから始める。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-REC-003`、`REQ-REC-004`、`REQ-SET-001`、`REQ-SET-002`、
//! `REQ-SET-005`。
//! 種目の行とセットの行は端末内の IndexedDB に保存し、送信待ちの列を通す (ADR-002)。
//! セットの入力は画面へすぐ反映し、端末内の保存は入力の版が変わるたびに行う。
//! セットの完了は `SetCompleted` を 1 回発行する。前回の選択と差の計算は
//! `warrior_core::previous` の純粋な関数を使う。

use dioxus::prelude::*;
use warrior_core::exercise::ExerciseSnapshot;
use warrior_core::previous::{
    DurationDifference, ExerciseDifference, PreviousExercise, PreviousTraining,
    TotalWeightDifference,
};

use super::errors::{clear_load_error, retain_set_errors, set_load_error, RecordErrors};
use super::exercise_view::{
    dispatch, dissolve_superset, exercise_diff, save_superset_group, training_difference,
    ExerciseView, SetSaver, SupersetGroupView,
};
use super::set_drawer::SetDrawer;
use super::set_events::SetCompletedEvents;
use super::set_table::{SetTable, SetTableState};
use super::sets::{format_number, set_kind_text, SelectedSet, SetAction, SetView};
use super::superset_panel::{SupersetCandidate, SupersetPanel};
use super::RecordSignals;
use crate::previous as previous_api;
use crate::record_rail::{RecordRailExercise, RecordRailState};
use crate::routes::navigate;
use crate::storage;
use crate::sync;
use crate::text::{text, Key};
use crate::training_exercise;

/// 記録の種目の一覧と、選んだ種目の詳細。
#[component]
pub(super) fn ExerciseSection(training_id: String, signals: RecordSignals) -> Element {
    let views = use_signal(Vec::<ExerciseView>::new);
    let selected_set = use_signal(|| Option::<String>::None);
    use_context_provider(|| SelectedSet(selected_set));
    let rail = use_context::<Signal<RecordRailState>>();
    let previous_training = use_signal(|| Option::<PreviousTraining>::None);
    let error = use_signal(RecordErrors::default);
    let loading = use_signal(|| true);
    let superset_busy = use_signal(|| false);
    let mut reload = use_signal(|| 0u32);
    let mut last_tree = use_signal(|| 0u64);
    let reload_effect = reload;
    let tree_revision = signals.tree_revision;
    let exercise_revision = signals.exercise_revision;
    let route_id = training_id.clone();
    use_effect(move || {
        let _ = (reload_effect)();
        let tree = (tree_revision)();
        let _ = (exercise_revision)();
        let preserve_inputs = tree == *last_tree.peek();
        last_tree.set(tree);
        let id = route_id.clone();
        spawn(load(
            id,
            views,
            rail,
            previous_training,
            error,
            loading,
            preserve_inputs,
        ));
    });
    let mut rail_for_drop = rail;
    use_drop(move || {
        let mut state = rail_for_drop.write();
        state.training_id = None;
        state.exercises.clear();
        state.selected = None;
    });

    let saver = use_hook(SetSaver::default);
    let completed_events = use_context::<SetCompletedEvents>();
    let action_training_id = training_id.clone();
    let on_action = EventHandler::new(move |action: SetAction| {
        dispatch(
            action,
            action_training_id.clone(),
            views,
            error,
            saver.clone(),
            completed_events,
        );
    });

    let delete_training_id = training_id.clone();
    let on_delete = EventHandler::new(move |view_id: String| {
        let operation = match training_exercise::delete(&delete_training_id, &view_id) {
            Ok(operation) => operation,
            Err(_) => {
                set_load_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
        };
        let training_id = delete_training_id.clone();
        spawn(async move {
            let result = match storage::current().await {
                Ok(storage) => {
                    storage
                        .save_training_exercise_delete(&training_id, &view_id, &operation)
                        .await
                }
                Err(error) => Err(error),
            };
            if result.is_err() {
                set_load_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
            sync::flush_in_background();
            reload.set((reload)() + 1);
        });
    });
    let move_training_id = training_id.clone();
    let on_move = EventHandler::new(move |(view_id, delta): (String, i32)| {
        let mut list = (views)();
        let Some(index) = list.iter().position(|view| view.record.id == view_id) else {
            return;
        };
        let target = index as i32 + delta;
        if target < 0 || target as usize >= list.len() {
            return;
        }
        list.swap(index, target as usize);
        let ordered = list
            .iter()
            .map(|view| view.record.id.clone())
            .collect::<Vec<_>>();
        let operation = match training_exercise::reorder(&move_training_id, &ordered) {
            Ok(operation) => operation,
            Err(_) => {
                set_load_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
        };
        let training_id = move_training_id.clone();
        spawn(async move {
            let result = match storage::current().await {
                Ok(storage) => {
                    storage
                        .save_training_exercise_reorder(&training_id, &ordered, &operation)
                        .await
                }
                Err(error) => Err(error),
            };
            if result.is_err() {
                set_load_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
            sync::flush_in_background();
            reload.set((reload)() + 1);
        });
    });
    let add_training_id = training_id.clone();
    let on_add = EventHandler::new(move |_: ()| {
        navigate(&format!("/record/{add_training_id}/exercises"));
    });
    let superset_training_id = training_id.clone();
    let mut create_busy = superset_busy;
    let on_superset_create =
        EventHandler::new(move |(view_id, candidate_ids): (String, Vec<String>)| {
            if create_busy() {
                return;
            }
            create_busy.set(true);
            let training_id = superset_training_id.clone();
            spawn(async move {
                save_superset_group(training_id, view_id, candidate_ids, views, error).await;
                create_busy.set(false);
            });
        });
    let delete_superset_training_id = training_id.clone();
    let mut delete_busy = superset_busy;
    let on_superset_delete = EventHandler::new(move |superset_id: String| {
        if delete_busy() {
            return;
        }
        delete_busy.set(true);
        let training_id = delete_superset_training_id.clone();
        spawn(async move {
            dissolve_superset(training_id, superset_id, views, error).await;
            delete_busy.set(false);
        });
    });

    let current_views = (views)();
    let rail_state = (rail)();
    let selected_id = rail_state.selected;
    let selected_view = selected_id
        .as_ref()
        .and_then(|id| current_views.iter().find(|view| &view.record.id == id))
        .cloned();
    let selected_index = selected_view.as_ref().and_then(|selected| {
        current_views
            .iter()
            .position(|view| view.record.id == selected.record.id)
    });
    let view_only = (signals.view_only)();
    let training_diff = training_difference(&current_views, (previous_training)().as_ref());
    let errors = (error)();
    let load_error = errors.load;
    let set_errors = errors.sets;
    let is_loading = (loading)();
    let superset_candidates = selected_view.as_ref().map_or_else(Vec::new, |selected| {
        current_views
            .iter()
            .filter(|view| view.record.id != selected.record.id && view.superset.is_none())
            .map(|view| SupersetCandidate {
                training_exercise_id: view.record.id.clone(),
                exercise_id: view.snapshot.exercise_id.clone(),
                name: view.snapshot.name.clone(),
            })
            .collect::<Vec<_>>()
    });
    let superset_members = selected_view
        .as_ref()
        .and_then(|view| view.superset.as_ref())
        .map_or_else(Vec::new, |group| {
            group
                .member_ids
                .iter()
                .filter_map(|id| {
                    current_views
                        .iter()
                        .find(|view| &view.record.id == id)
                        .map(|view| (id.clone(), view.snapshot.name.clone()))
                })
                .collect::<Vec<_>>()
        });

    rsx! {
        section {
            class: "flex flex-col gap-2",
            "data-exercises": "true",
            div { class: "flex items-center justify-end gap-2",
                button {
                    class: "min-h-11 border border-primary px-3 text-[13px] font-bold text-primary",
                    "data-add-exercise": "true",
                    disabled: view_only,
                    onclick: move |_| on_add.call(()),
                    "{text(Key::RecordExerciseAdd)}"
                }
            }
            if let Some(message) = load_error {
                p {
                    class: "text-[13px] text-primary",
                    "data-record-error": "true",
                    "{message}"
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| reload.set((reload)() + 1),
                    "{text(Key::RecordRetry)}"
                }
            }
            for (set_id, message) in set_errors {
                p {
                    class: "text-[13px] text-primary",
                    "data-record-error": "true",
                    "data-record-error-set-id": "{set_id}",
                    "{message}"
                }
            }
            if current_views.is_empty() && !is_loading {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordExerciseEmpty)}" }
            }
            if let Some(view) = selected_view {
                {
                    let first = selected_index == Some(0);
                    let last = selected_index
                        .is_some_and(|index| index + 1 == current_views.len());
                    let up_id = view.record.id.clone();
                    let down_id = view.record.id.clone();
                    let delete_id = view.record.id.clone();
                    let drawer_mode = view.snapshot.record_mode;
                    let drawer_rows = view.sets.clone();
                    rsx! {
                        ExerciseDetail {
                            state: ExerciseDetailState {
                                view,
                                disabled: view_only || is_loading || superset_busy(),
                                first,
                                last,
                                superset_members,
                                superset_candidates,
                            },
                            actions: ExerciseDetailActions {
                                on_action,
                                on_up: EventHandler::new(move |_: ()| {
                                    on_move.call((up_id.clone(), -1));
                                }),
                                on_down: EventHandler::new(move |_: ()| {
                                    on_move.call((down_id.clone(), 1));
                                }),
                                on_delete: EventHandler::new(move |_: ()| {
                                    on_delete.call(delete_id.clone());
                                }),
                                on_superset_create,
                                on_superset_delete,
                            },
                        }
                        SetDrawer {
                            rows: drawer_rows,
                            mode: drawer_mode,
                            disabled: view_only,
                            on_action,
                        }
                    }
                }
            } else {
                EmptySetTable {}
            }
            if let Some(diff) = training_diff {
                TrainingDifference { diff }
            }
        }
    }
}

async fn load(
    training_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    mut rail: Signal<RecordRailState>,
    mut previous_training: Signal<Option<PreviousTraining>>,
    error: Signal<RecordErrors>,
    mut loading: Signal<bool>,
    preserve_inputs: bool,
) {
    loading.set(true);
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            set_load_error(error, Key::RecordExerciseLoadFailed);
            loading.set(false);
            return;
        }
    };
    let records = match storage.training_exercises(&training_id).await {
        Ok(records) => records,
        Err(_) => {
            set_load_error(error, Key::RecordExerciseLoadFailed);
            loading.set(false);
            return;
        }
    };
    let body_weight_kg = crate::profile::body_weight_for_record().await;
    let supersets = match storage.training_supersets(&training_id).await {
        Ok(supersets) => supersets,
        Err(_) => {
            set_load_error(error, Key::RecordSupersetLoadFailed);
            loading.set(false);
            return;
        }
    };
    let old = (views)();
    let mut next = Vec::with_capacity(records.len());
    let mut training_previous = match previous_api::fetch_training(&training_id).await {
        Ok(response) => response.training,
        Err(_) => None,
    };
    for record in records {
        let Ok(snapshot) = serde_json::from_str::<ExerciseSnapshot>(&record.snapshot) else {
            continue;
        };
        let (previous, previous_error) =
            match previous_api::fetch(&training_id, &snapshot.exercise_id, None, None).await {
                Ok(response) => {
                    if training_previous.is_none() {
                        training_previous = response.training.clone();
                    }
                    (response.exercise, false)
                }
                Err(_) => (None, true),
            };
        let stored_sets = match storage.training_sets(&record.id).await {
            Ok(sets) => sets,
            Err(_) => {
                set_load_error(error, Key::RecordSetLoadFailed);
                loading.set(false);
                return;
            }
        };
        let loaded_sets = stored_sets
            .into_iter()
            .filter_map(SetView::from_stored)
            .collect::<Vec<_>>();
        let sets = if preserve_inputs {
            old.iter()
                .find(|view| view.record.id == record.id)
                .map_or(loaded_sets, |view| view.sets.clone())
        } else {
            loaded_sets
        };
        let superset = supersets
            .iter()
            .find(|group| {
                group
                    .members
                    .iter()
                    .any(|member| member.training_exercise_id == record.id)
            })
            .map(|group| SupersetGroupView {
                id: group.id.clone(),
                member_ids: group
                    .members
                    .iter()
                    .map(|member| member.training_exercise_id.clone())
                    .collect(),
            });
        next.push(ExerciseView {
            record,
            snapshot,
            previous,
            previous_error,
            sets,
            body_weight_kg,
            superset,
        });
    }
    let current_selected = if rail.peek().training_id.as_deref() == Some(training_id.as_str()) {
        rail.peek().selected.clone()
    } else {
        None
    };
    let selected = current_selected
        .filter(|id| next.iter().any(|view| &view.record.id == id))
        .or_else(|| next.first().map(|view| view.record.id.clone()));
    rail.set(RecordRailState {
        training_id: Some(training_id.clone()),
        exercises: next
            .iter()
            .map(|view| RecordRailExercise {
                id: view.record.id.clone(),
                exercise_id: view.snapshot.exercise_id.clone(),
                name: view.snapshot.name.clone(),
            })
            .collect(),
        selected,
    });
    let set_ids = next
        .iter()
        .flat_map(|view| view.sets.iter().map(|set| set.record.id.clone()))
        .collect::<Vec<_>>();
    previous_training.set(training_previous);
    views.set(next);
    clear_load_error(error);
    retain_set_errors(error, &set_ids);
    loading.set(false);
}

/// 種目の詳細に渡す状態。
#[derive(Clone, PartialEq)]
struct ExerciseDetailState {
    /// 種目の状態。
    view: ExerciseView,
    /// 編集できないか。
    disabled: bool,
    /// 先頭の種目か。
    first: bool,
    /// 末尾の種目か。
    last: bool,
    /// 組の種目と名前。
    superset_members: Vec<(String, String)>,
    /// 組に加えられる種目。
    superset_candidates: Vec<SupersetCandidate>,
}

/// 種目の詳細の操作。
#[derive(Clone, PartialEq)]
struct ExerciseDetailActions {
    /// セットの操作。
    on_action: EventHandler<SetAction>,
    /// 前へ移動する。
    on_up: EventHandler<()>,
    /// 後ろへ移動する。
    on_down: EventHandler<()>,
    /// 削除する。
    on_delete: EventHandler<()>,
    /// 組を作る。
    on_superset_create: EventHandler<(String, Vec<String>)>,
    /// 組を解除する。
    on_superset_delete: EventHandler<String>,
}

#[component]
fn ExerciseDetail(state: ExerciseDetailState, actions: ExerciseDetailActions) -> Element {
    let ExerciseDetailState {
        view,
        disabled,
        first,
        last,
        superset_members,
        superset_candidates,
    } = state;
    let ExerciseDetailActions {
        on_action,
        on_up,
        on_down,
        on_delete,
        on_superset_create,
        on_superset_delete,
    } = actions;
    let diff = exercise_diff(&view);
    let view_id = view.record.id.clone();
    let copy_id = view.record.id.clone();
    let previous = view.previous.clone();
    let previous_error = view.previous_error;
    let mode = view.snapshot.record_mode;
    let laterality = view.snapshot.laterality;
    let superset_view_id = view.record.id.clone();
    let superset = view.superset.clone();
    let rows = view.sets.clone();

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-2",
            "data-exercise-detail": "true",
            "data-exercise-id": "{view.snapshot.exercise_id}",
            div { class: "flex flex-wrap items-center justify-between gap-1",
                h3 { class: "text-[13px] font-bold", "{view.snapshot.name}" }
                div { class: "flex flex-wrap gap-1",
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-move-up": "true",
                        disabled: disabled || first,
                        onclick: move |_| on_up.call(()),
                        "{text(Key::RecordExerciseMoveUp)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-move-down": "true",
                        disabled: disabled || last,
                        onclick: move |_| on_down.call(()),
                        "{text(Key::RecordExerciseMoveDown)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px] text-primary",
                        "data-delete-exercise": "true",
                        disabled,
                        onclick: move |_| on_delete.call(()),
                        "{text(Key::RecordExerciseDelete)}"
                    }
                    if previous.is_some() {
                        button {
                            class: "min-h-11 border border-primary px-2 text-[11px] font-bold text-primary",
                            "data-copy-previous": "true",
                            disabled,
                            onclick: move |_| on_action.call(SetAction::Copy { view_id: copy_id.clone() }),
                            "{text(Key::RecordExerciseCopy)}"
                        }
                    }
                }
            }
            if let Some(previous) = previous {
                PreviousRecord { previous }
            } else if previous_error {
                p { class: "text-[11px] text-text-sub", "{text(Key::RecordExercisePreviousFailed)}" }
            }
            SupersetPanel {
                view_id: superset_view_id,
                group: superset,
                members: superset_members,
                candidates: superset_candidates,
                disabled,
                on_create: on_superset_create,
                on_delete: on_superset_delete,
            }
            if let Some(diff) = diff {
                ExerciseDifferenceView { diff }
            }
            SetTable {
                state: SetTableState {
                    view_id,
                    rows,
                    mode,
                    laterality,
                    disabled,
                },
                on_action,
            }
        }
    }
}

#[component]
fn PreviousRecord(previous: PreviousExercise) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-line p-2",
            "data-previous": "true",
            "data-previous-total-weight-kg": "{previous.total_weight_kg}",
            "data-previous-total-duration-seconds": "{previous.total_duration_seconds}",
            "data-previous-max-duration-seconds": "{previous.max_duration_seconds}",
            h4 { class: "text-[11px] font-bold", "{text(Key::RecordExercisePrevious)}" }
            for (index, set) in previous.sets.iter().enumerate() {
                div {
                    class: "text-[11px] leading-5",
                    "data-previous-set": "true",
                    "data-set-index": "{index}",
                    "data-set-kind": "{set.kind.as_str()}",
                    "{previous_set_text(index, set)}"
                }
            }
        }
    }
}

#[component]
fn ExerciseDifferenceView(diff: ExerciseDifference) -> Element {
    match diff {
        ExerciseDifference::Weight(diff) => rsx! {
            div {
                class: "text-[13px] font-bold",
                "data-exercise-weight-diff": "{rounded_tenth(diff.difference_kg)}",
                "{text(Key::RecordExerciseWeightDiff)}: {signed_weight(diff.difference_kg)}"
            }
        },
        ExerciseDifference::Time(diff) => rsx! {
            div {
                class: "flex flex-col gap-1 text-[13px] font-bold",
                "data-exercise-total-duration-diff": "{diff.total_difference_seconds}",
                "data-exercise-longest-duration-diff": "{diff.longest_difference_seconds}",
                div { "{text(Key::RecordExerciseTotalDurationDiff)}: {signed_duration(diff)}" }
                div { "{text(Key::RecordExerciseLongestDurationDiff)}: {signed_longest(diff)}" }
            }
        },
    }
}

#[component]
fn TrainingDifference(diff: TotalWeightDifference) -> Element {
    rsx! {
        div {
            class: "border border-line bg-surface p-2 text-[13px] font-bold",
            "data-training-total-diff": "{rounded_tenth(diff.difference_kg)}",
            "{text(Key::RecordTrainingWeightDiff)}: {signed_weight(diff.difference_kg)}"
        }
    }
}

#[component]
fn EmptySetTable() -> Element {
    rsx! {
        div { class: "flex flex-col gap-1",
            h2 { class: "text-sm font-bold", "{text(Key::RecordSets)}" }
            div {
                class: "h-[222px] overflow-y-auto border border-line bg-surface",
                "data-set-table": "true",
                "data-set-rows": "5",
                "data-set-row-height": "44",
                for index in 0..5 {
                    div {
                        class: "h-11 border-b border-line",
                        "data-set-row": "true",
                        "data-set-row-index": "{index}",
                    }
                }
            }
        }
    }
}

fn previous_set_text(index: usize, set: &warrior_core::previous::PreviousSet) -> String {
    let segment = set.segments.first();
    let value = match segment {
        Some(segment) => {
            let side_text = |side: &warrior_core::set_calc::SideValue| {
                format!("{} kg x {}", format_number(side.weight_kg), side.reps)
            };
            match (segment.left.as_ref(), segment.right.as_ref()) {
                (Some(left), Some(right)) => format!(
                    "{} {} / {} {}",
                    text(Key::RecordSetLeft),
                    side_text(left),
                    text(Key::RecordSetRight),
                    side_text(right)
                ),
                _ => {
                    let duration = segment.longest_duration_seconds();
                    if duration > 0 && segment.weight_kg == 0.0 && segment.reps == 0 {
                        format!("{duration}{}", text(Key::RecordSeconds))
                    } else {
                        format!("{} kg x {}", format_number(segment.weight_kg), segment.reps)
                    }
                }
            }
        }
        None => String::new(),
    };
    format!("{} {} ({})", index + 1, value, set_kind_text(set.kind))
}

fn signed_weight(value: f64) -> String {
    let value = rounded_tenth(value);
    if value > 0.0 {
        format!("+{value:.1} kg")
    } else if value < 0.0 {
        format!("{value:.1} kg")
    } else {
        "0 kg".to_owned()
    }
}

fn rounded_tenth(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let rounded = (value * 10.0).round() / 10.0;
    if rounded == 0.0 {
        0.0
    } else {
        rounded
    }
}

fn signed_duration(diff: DurationDifference) -> String {
    signed_seconds(diff.total_difference_seconds)
}

fn signed_longest(diff: DurationDifference) -> String {
    signed_seconds(diff.longest_difference_seconds)
}

fn signed_seconds(value: i64) -> String {
    if value > 0 {
        format!("+{value} {}", text(Key::RecordSeconds))
    } else if value < 0 {
        format!("{value} {}", text(Key::RecordSeconds))
    } else {
        format!("0 {}", text(Key::RecordSeconds))
    }
}
