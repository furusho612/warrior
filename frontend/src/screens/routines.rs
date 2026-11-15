//! ルーティンの一覧、編集、複製、削除と、ルーティンからの開始。
//!
//! 保存、編集、複製、削除は端末内の行と送信待ちの列へ同じトランザクションで書き、
//! サーバーへの送信を操作の完了条件にしない (ADR-002)。開始は送信待ちの列を送ってから
//! サーバーにトレーニングを作らせ、返った記録を端末へ保存する。

use dioxus::prelude::*;
use warrior_core::duration;
use warrior_core::exercise::{Exercise, ExerciseSnapshot, GripMode, GripSelection, RecordMode};
use warrior_core::rest_timer;
use warrior_core::routine::{self as routine_model, Routine, RoutineExercise, RoutineTarget};
use warrior_core::sync::OutboxState;

use crate::grip_text::{attachment_text, grip_style_text, grip_width_text};
use crate::ids::{new_uuid, now_iso};
use crate::library as library_api;
use crate::routes::navigate;
use crate::routine as routine_api;
use crate::storage::{self, StorageError};
use crate::sync::{self, SyncStatus};
use crate::text::{text, Key};

/// ルーティンとプログラムの画面。
#[component]
pub fn RoutinesScreen() -> Element {
    let routines = use_signal(Vec::<Routine>::new);
    let mut editing = use_signal(|| Option::<Routine>::None);
    let exercises = use_signal(Vec::<Exercise>::new);
    let selected_exercise = use_signal(String::new);
    let status = use_signal(SyncStatus::default);
    let mut error = use_signal(|| Option::<String>::None);
    let busy = use_signal(|| false);
    let mut confirm_delete = use_signal(|| Option::<String>::None);
    let mut reload = use_signal(|| 0u32);
    let reload_effect = reload;

    use_effect(move || {
        let _ = (reload_effect)();
        spawn(load(routines, status, error));
    });

    let current = (routines)();
    let editing_value = (editing)();
    let error_value = (error)();
    let confirm = (confirm_delete)();
    let busy_value = (busy)();
    let status_value = (status)();
    let exercises_value = (exercises)();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-routines": "true",
            h1 { class: "text-lg font-bold", "{text(Key::RoutinesTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::RoutinesDescription)}" }
            RoutineSyncStatus {
                status: status_value,
                on_retry: move |_| {
                    spawn(async move {
                        retry_failed().await;
                        reload.set((reload)() + 1);
                    });
                },
            }
            if let Some(message) = error_value {
                p { class: "text-[13px] text-primary", "data-routine-error": "true", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        reload.set((reload)() + 1);
                    },
                    "{text(Key::RoutinesRetry)}"
                }
            }
            if let Some(draft) = editing_value {
                RoutineEditor {
                    draft,
                    editing,
                    exercises: exercises_value,
                    selected_exercise,
                    error,
                    busy: busy_value,
                    reload,
                }
            } else {
                button {
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-routine-new": "true",
                    disabled: busy_value,
                    onclick: move |_| {
                        spawn(open_new_routine(routines, editing, exercises, error));
                    },
                    "{text(Key::RoutinesNew)}"
                }
                if current.is_empty() {
                    p { class: "text-[13px] text-text-sub", "data-routines-empty": "true", "{text(Key::RoutinesEmpty)}" }
                }
                div { class: "flex flex-col gap-2", "data-routine-list": "true",
                    for routine in current {
                        RoutineListItem {
                            routine: routine.clone(),
                            confirm: confirm.clone(),
                            busy: busy_value,
                            on_edit: move |routine: Routine| {
                                editing.set(Some(routine));
                                error.set(None);
                            },
                            on_copy: move |routine_id: String| {
                                spawn(copy_routine(routine_id, routines, error, busy, reload));
                            },
                            on_delete: move |routine_id: String| {
                                confirm_delete.set(Some(routine_id));
                            },
                            on_start: move |routine_id: String| {
                                spawn(start_routine(routine_id, error, busy));
                            },
                            on_delete_confirm: move |routine_id: String| {
                                confirm_delete.set(None);
                                spawn(delete_routine(routine_id, error, busy, reload));
                            },
                            on_delete_cancel: move |_| confirm_delete.set(None),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RoutineSyncStatus(status: SyncStatus, on_retry: EventHandler<()>) -> Element {
    let value = if status.failed > 0 {
        "failed"
    } else if status.unsaved > 0 {
        "unsaved"
    } else {
        "saved"
    };
    let mut parts = Vec::new();
    if status.unsaved > 0 {
        parts.push(format!("{}: {}", text(Key::SyncUnsaved), status.unsaved));
    }
    if status.failed > 0 {
        parts.push(format!("{}: {}", text(Key::SyncFailed), status.failed));
    }
    if parts.is_empty() {
        parts.push(text(Key::SyncSaved).to_owned());
    }
    rsx! {
        p {
            class: "text-[13px] text-text-sub",
            "data-routine-sync-status": value,
            "{parts.join(\" \")}"
        }
        if status.failed > 0 {
            button {
                class: "min-h-11 border border-line px-3 text-[13px]",
                "data-routine-retry": "true",
                onclick: move |_| on_retry.call(()),
                "{text(Key::RoutinesRetry)}"
            }
        }
    }
}

#[component]
fn RoutineListItem(
    routine: Routine,
    confirm: Option<String>,
    busy: bool,
    on_edit: EventHandler<Routine>,
    on_copy: EventHandler<String>,
    on_delete: EventHandler<String>,
    on_start: EventHandler<String>,
    on_delete_confirm: EventHandler<String>,
    on_delete_cancel: EventHandler<()>,
) -> Element {
    let confirming = confirm.as_deref() == Some(routine.id.as_str());
    let edit_routine = routine.clone();
    let start_id = routine.id.clone();
    let copy_id = routine.id.clone();
    let delete_id = routine.id.clone();
    let confirm_id = routine.id.clone();
    let summary = routine_summary(&routine);
    rsx! {
        article {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-routine": "true",
            "data-routine-id": "{routine.id}",
            h2 { class: "text-sm font-bold", "data-routine-name": "true", "{routine.name}" }
            p { class: "text-[11px] leading-4 text-text-sub", "data-routine-summary": "true", "{summary}" }
            div { class: "flex flex-wrap gap-2",
                button {
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-routine-start": "true",
                    disabled: busy,
                    onclick: move |_| on_start.call(start_id.clone()),
                    "{text(Key::RoutinesStart)}"
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-routine-edit": "true",
                    disabled: busy,
                    onclick: move |_| on_edit.call(edit_routine.clone()),
                    "{text(Key::RoutinesEdit)}"
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-routine-copy": "true",
                    disabled: busy,
                    onclick: move |_| on_copy.call(copy_id.clone()),
                    "{text(Key::RoutinesCopy)}"
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-routine-delete": "true",
                    disabled: busy,
                    onclick: move |_| on_delete.call(delete_id.clone()),
                    "{text(Key::RoutinesDelete)}"
                }
            }
            if confirming {
                p { class: "text-[13px] text-primary", "{text(Key::RoutinesDeleteConfirm)}" }
                div { class: "flex flex-wrap gap-2",
                    button {
                        class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                        "data-routine-delete-confirm": "true",
                        onclick: move |_| on_delete_confirm.call(confirm_id.clone()),
                        "{text(Key::RoutinesDeleteAccept)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        "data-routine-delete-cancel": "true",
                        onclick: move |_| on_delete_cancel.call(()),
                        "{text(Key::RoutinesDeleteCancel)}"
                    }
                }
            }
        }
    }
}

#[component]
fn RoutineEditor(
    draft: Routine,
    mut editing: Signal<Option<Routine>>,
    exercises: Vec<Exercise>,
    selected_exercise: Signal<String>,
    mut error: Signal<Option<String>>,
    busy: bool,
    reload: Signal<u32>,
) -> Element {
    let draft_signal = editing;
    let mut name_draft = draft_signal;
    let mut add_draft = draft_signal;
    let selected = (selected_exercise)();
    let name = draft.name.clone();
    let routine_id = draft.id.clone();
    let rows = draft.exercises.clone();
    let library = exercises.clone();
    let option_library = library.clone();
    let mut selected_signal = selected_exercise;
    rsx! {
        div {
            class: "flex flex-col gap-3 border border-line bg-surface p-3",
            "data-routine-editor": "true",
            "data-routine-id": "{routine_id}",
            h2 { class: "text-sm font-bold", "{text(Key::RoutinesEditTitle)}" }
            label { class: "text-[13px] font-bold", r#for: "routine-name", "{text(Key::RoutinesNameLabel)}" }
            input {
                id: "routine-name",
                class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                "data-routine-name-input": "true",
                value: "{name}",
                maxlength: "{routine_model::NAME_MAX_CHARS}",
                oninput: move |event| {
                    if let Some(draft) = name_draft.write().as_mut() {
                        draft.name = event.value();
                    }
                },
            }
            if rows.is_empty() {
                p { class: "text-[13px] text-text-sub", "data-routine-exercises-empty": "true", "{text(Key::RoutinesExerciseEmpty)}" }
            }
            div { class: "flex flex-col gap-2", "data-routine-exercises": "true",
                for (index, exercise) in rows.iter().enumerate() {
                    RoutineExerciseRow {
                        index,
                        exercise: exercise.clone(),
                        draft: draft_signal,
                        on_move: move |(index, delta): (usize, i32)| {
                            move_exercise(draft_signal, index, delta);
                        },
                        on_remove: move |index: usize| {
                            if let Some(draft) = add_draft.write().as_mut() {
                                draft.exercises.remove(index);
                                renumber(&mut draft.exercises);
                            }
                        },
                    }
                }
            }
            div { class: "flex flex-col gap-2 border border-line p-2",
                label { class: "text-[13px] font-bold", r#for: "routine-exercise-choice", "{text(Key::RoutinesExerciseAdd)}" }
                select {
                    id: "routine-exercise-choice",
                    class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                    "data-routine-exercise-choice": "true",
                    value: "{selected}",
                    onchange: move |event| selected_signal.set(event.value()),
                    option { value: "", "{text(Key::RoutinesExerciseSelect)}" }
                    for exercise in option_library {
                        option { value: "{exercise.id}", "{exercise.name}" }
                    }
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-routine-add-exercise": "true",
                    disabled: busy,
                    onclick: move |_| {
                        let chosen = (selected_exercise)();
                        if chosen.is_empty() {
                            error.set(Some(text(Key::RoutinesExerciseSelect).to_owned()));
                            return;
                        }
                        let Some(exercise) = library.iter().find(|exercise| exercise.id == chosen) else {
                            error.set(Some(text(Key::RoutinesExerciseSelect).to_owned()));
                            return;
                        };
                        match add_exercise(add_draft, exercise) {
                            Ok(()) => error.set(None),
                            Err(message) => error.set(Some(message)),
                        }
                    },
                    "{text(Key::RoutinesExerciseAdd)}"
                }
            }
            div { class: "flex flex-wrap gap-2",
                button {
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-routine-save": "true",
                    disabled: busy,
                    onclick: move |_| {
                        spawn(save_routine(draft_signal, error, reload));
                    },
                    "{text(Key::RoutinesSave)}"
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-routine-cancel": "true",
                    disabled: busy,
                    onclick: move |_| {
                        editing.set(None);
                    },
                    "{text(Key::RoutinesCancel)}"
                }
            }
        }
    }
}

#[component]
fn RoutineExerciseRow(
    index: usize,
    exercise: RoutineExercise,
    draft: Signal<Option<Routine>>,
    on_move: EventHandler<(usize, i32)>,
    on_remove: EventHandler<usize>,
) -> Element {
    let snapshot = exercise.snapshot.clone();
    let target = exercise.target.clone();
    let time = snapshot.record_mode == RecordMode::Time;
    let grip_value = target
        .grip
        .as_ref()
        .and_then(|grip| serde_json::to_string(grip).ok())
        .unwrap_or_default();
    let duration_value = target
        .duration_seconds
        .map(duration::format)
        .unwrap_or_default();
    let rest_value = target
        .rest_seconds
        .map(duration::format)
        .unwrap_or_default();
    let weight_value = target.weight_kg.map(format_number).unwrap_or_default();
    let reps_value = target.reps.map(|reps| reps.to_string()).unwrap_or_default();
    let set_count_value = target.set_count.to_string();
    let grip_draft = draft;
    let duration_draft = draft;
    let weight_draft = draft;
    let reps_draft = draft;
    let set_count_draft = draft;
    let rest_draft = draft;
    let grip_options = snapshot
        .grip_options
        .iter()
        .map(|option| {
            (
                serde_json::to_string(option).unwrap_or_default(),
                grip_option_label(*option),
            )
        })
        .collect::<Vec<_>>();
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line p-2",
            "data-routine-exercise": "true",
            "data-routine-exercise-id": "{snapshot.exercise_id}",
            "data-routine-exercise-index": "{index}",
            div { class: "flex items-center justify-between gap-2",
                span { class: "text-[13px] font-bold", "{snapshot.name}" }
                div { class: "flex gap-1",
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-routine-exercise-up": "true",
                        onclick: move |_| on_move.call((index, -1)),
                        "{text(Key::RoutinesMoveUp)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-routine-exercise-down": "true",
                        onclick: move |_| on_move.call((index, 1)),
                        "{text(Key::RoutinesMoveDown)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-routine-exercise-remove": "true",
                        onclick: move |_| on_remove.call(index),
                        "{text(Key::RoutinesExerciseRemove)}"
                    }
                }
            }
            div { class: "grid grid-cols-2 gap-2",
                RoutineField {
                    label: text(Key::RoutinesSetCount).to_owned(),
                    input: rsx! {
                        input {
                            class: "min-h-11 w-full border border-line bg-surface px-2 text-[13px]",
                            "data-routine-set-count": "true",
                            r#type: "number",
                            min: "1",
                            value: "{set_count_value}",
                            oninput: move |event| {
                                update_target(set_count_draft, index, |target| {
                                    target.set_count = event.value().parse().unwrap_or(0);
                                });
                            },
                        }
                    },
                }
                RoutineField {
                    label: text(Key::RoutinesRest).to_owned(),
                    input: rsx! {
                        input {
                            class: "min-h-11 w-full border border-line bg-surface px-2 text-[13px]",
                            "data-routine-rest": "true",
                            value: "{rest_value}",
                            placeholder: "mm:ss",
                            oninput: move |event| {
                                let value = parse_optional_seconds(&event.value(), rest_timer::MAX_SECONDS);
                                update_target(rest_draft, index, |target| {
                                    target.rest_seconds = value;
                                });
                            },
                        }
                    },
                }
                if time {
                    RoutineField {
                        label: text(Key::RoutinesTargetDuration).to_owned(),
                        input: rsx! {
                            input {
                                class: "min-h-11 w-full border border-line bg-surface px-2 text-[13px]",
                                "data-routine-target-duration": "true",
                                value: "{duration_value}",
                                placeholder: "mm:ss",
                                oninput: move |event| {
                                    let value = parse_optional_seconds(
                                        &event.value(),
                                        routine_model::DURATION_MAX_SECONDS,
                                    );
                                    update_target(duration_draft, index, |target| {
                                        target.duration_seconds = value;
                                    });
                                },
                            }
                        },
                    }
                } else {
                    RoutineField {
                        label: text(Key::RoutinesTargetWeight).to_owned(),
                        input: rsx! {
                            input {
                                class: "min-h-11 w-full border border-line bg-surface px-2 text-[13px]",
                                "data-routine-target-weight": "true",
                                value: "{weight_value}",
                                oninput: move |event| {
                                    let value = parse_optional_weight(&event.value());
                                    update_target(weight_draft, index, |target| {
                                        target.weight_kg = value;
                                    });
                                },
                            }
                        },
                    }
                    RoutineField {
                        label: text(Key::RoutinesTargetReps).to_owned(),
                        input: rsx! {
                            input {
                                class: "min-h-11 w-full border border-line bg-surface px-2 text-[13px]",
                                "data-routine-target-reps": "true",
                                r#type: "number",
                                min: "1",
                                value: "{reps_value}",
                                oninput: move |event| {
                                    let value = event.value().parse::<u32>().ok();
                                    update_target(reps_draft, index, |target| {
                                        target.reps = value;
                                    });
                                },
                            }
                        },
                    }
                }
            }
            if snapshot.grip_mode != GripMode::None {
                label { class: "text-[11px] text-text-sub", "{text(Key::RoutinesGrip)}" }
                select {
                    class: "min-h-11 border border-line bg-surface px-2 text-[13px]",
                    "data-routine-grip": "true",
                    value: "{grip_value}",
                    onchange: move |event| {
                        let value = serde_json::from_str::<GripSelection>(&event.value()).ok();
                        update_target(grip_draft, index, |target| {
                            target.grip = value;
                        });
                    },
                    option { value: "", "{text(Key::RecordSetGripNone)}" }
                    for (value, label) in grip_options {
                        option { value: "{value}", "{label}" }
                    }
                }
            }
        }
    }
}

#[component]
fn RoutineField(label: String, input: Element) -> Element {
    rsx! {
        label { class: "flex flex-col gap-1 text-[11px] text-text-sub",
            "{label}"
            {input}
        }
    }
}

async fn load(
    mut routines: Signal<Vec<Routine>>,
    status: Signal<SyncStatus>,
    mut error: Signal<Option<String>>,
) {
    let Ok(storage) = storage::current().await else {
        error.set(Some(text(Key::RoutinesLoadFailed).to_owned()));
        return;
    };
    let local = storage.routines().await.unwrap_or_default();
    routines.set(local);
    update_status(&storage, status).await;
    let entries = storage.outbox_entries().await.unwrap_or_default();
    if entries
        .iter()
        .any(|entry| entry.operation.kind.is_routine())
    {
        return;
    }
    match routine_api::list().await {
        Ok(list) => {
            if storage.replace_routines(&list.routines).await.is_ok() {
                routines.set(storage.routines().await.unwrap_or_default());
            }
            error.set(None);
        }
        Err(_) => {
            // 通信できないときは端末に保存した内容を表示する。
        }
    }
    update_status(&storage, status).await;
}

async fn update_status(storage: &storage::Storage, mut status: Signal<SyncStatus>) {
    let entries = storage.outbox_entries().await.unwrap_or_default();
    let routines = entries
        .iter()
        .filter(|entry| entry.operation.kind.is_routine());
    let mut unsaved = 0;
    let mut failed = 0;
    for entry in routines {
        match entry.state {
            OutboxState::Pending | OutboxState::Sending | OutboxState::Blocked => unsaved += 1,
            OutboxState::Failed => failed += 1,
        }
    }
    status.set(SyncStatus { unsaved, failed });
}

async fn retry_failed() {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let entries = storage.outbox_entries().await.unwrap_or_default();
    for entry in entries
        .iter()
        .filter(|entry| entry.operation.kind.is_routine() && entry.state == OutboxState::Failed)
    {
        sync::retry_operation(&entry.operation.operation_id).await;
    }
    sync::flush().await;
}

async fn open_new_routine(
    routines: Signal<Vec<Routine>>,
    mut editing: Signal<Option<Routine>>,
    mut exercises: Signal<Vec<Exercise>>,
    mut error: Signal<Option<String>>,
) {
    let id = match new_uuid() {
        Ok(id) => id,
        Err(_) => {
            error.set(Some(text(Key::RoutinesSaveFailed).to_owned()));
            return;
        }
    };
    let now = now_iso();
    let position = (routines)()
        .iter()
        .map(|routine| routine.position)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    if (exercises)().is_empty() {
        match library_api::all().await {
            Ok(list) => exercises.set(
                list.exercises
                    .into_iter()
                    .map(|item| item.exercise)
                    .collect(),
            ),
            Err(_) => error.set(Some(text(Key::RoutinesLoadFailed).to_owned())),
        }
    }
    editing.set(Some(Routine {
        id,
        name: String::new(),
        position,
        exercises: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    }));
    error.set(None);
}

fn add_exercise(mut draft: Signal<Option<Routine>>, exercise: &Exercise) -> Result<(), String> {
    let mut guard = draft.write();
    let Some(routine) = guard.as_mut() else {
        return Err(text(Key::RoutinesSaveFailed).to_owned());
    };
    let now = now_iso();
    let position = routine
        .exercises
        .iter()
        .map(|exercise| exercise.position)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let id = new_uuid().map_err(|_| text(Key::RoutinesSaveFailed).to_owned())?;
    let snapshot = ExerciseSnapshot::from(exercise);
    let target = default_target(&snapshot);
    routine.exercises.push(RoutineExercise {
        id,
        routine_id: routine.id.clone(),
        exercise_id: exercise.id.clone(),
        snapshot,
        position,
        target,
        created_at: now.clone(),
        updated_at: now,
    });
    Ok(())
}

fn default_target(snapshot: &ExerciseSnapshot) -> RoutineTarget {
    let time = snapshot.record_mode == RecordMode::Time;
    RoutineTarget {
        set_count: 3,
        weight_kg: if time { None } else { Some(0.0) },
        reps: if time { None } else { Some(10) },
        duration_seconds: if time { Some(60) } else { None },
        grip: snapshot.grip_options.first().copied(),
        rest_seconds: None,
    }
}

fn move_exercise(mut draft: Signal<Option<Routine>>, index: usize, delta: i32) {
    let mut guard = draft.write();
    let Some(routine) = guard.as_mut() else {
        return;
    };
    let target = index as i32 + delta;
    if target < 0 || target as usize >= routine.exercises.len() {
        return;
    }
    routine.exercises.swap(index, target as usize);
    renumber(&mut routine.exercises);
}

fn renumber(exercises: &mut [RoutineExercise]) {
    for (index, exercise) in exercises.iter_mut().enumerate() {
        exercise.position = index as i64 + 1;
    }
}

fn update_target(
    mut draft: Signal<Option<Routine>>,
    index: usize,
    apply: impl FnOnce(&mut RoutineTarget),
) {
    let mut guard = draft.write();
    let Some(routine) = guard.as_mut() else {
        return;
    };
    let Some(exercise) = routine.exercises.get_mut(index) else {
        return;
    };
    apply(&mut exercise.target);
    exercise.updated_at = now_iso();
}

async fn save_routine(
    mut editing: Signal<Option<Routine>>,
    mut error: Signal<Option<String>>,
    mut reload: Signal<u32>,
) {
    let Some(mut draft) = editing() else {
        return;
    };
    if routine_model::validate(&draft.name, &draft.inputs()).is_err() {
        error.set(Some(text(Key::RoutinesInvalid).to_owned()));
        return;
    }
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            error.set(Some(text(Key::RoutinesSaveFailed).to_owned()));
            return;
        }
    };
    let existing = storage.routine(&draft.id).await.ok().flatten().is_some();
    let now = now_iso();
    draft.updated_at = now;
    let result = if existing {
        match routine_api::update(&draft) {
            Ok(operation) => storage.save_routine_update(&draft, &operation).await,
            Err(_) => Err(StorageError::InvalidOperation(
                "the routine update is invalid".to_owned(),
            )),
        }
    } else {
        match routine_api::create(&draft) {
            Ok(operation) => storage.save_routine_create(&draft, &operation).await,
            Err(_) => Err(StorageError::InvalidOperation(
                "the routine creation is invalid".to_owned(),
            )),
        }
    };
    match result {
        Ok(()) => {
            error.set(None);
            editing.set(None);
            sync::flush_in_background();
            reload.set((reload)() + 1);
        }
        Err(_) => error.set(Some(text(Key::RoutinesSaveFailed).to_owned())),
    }
}

async fn copy_routine(
    routine_id: String,
    routines: Signal<Vec<Routine>>,
    mut error: Signal<Option<String>>,
    mut busy: Signal<bool>,
    mut reload: Signal<u32>,
) {
    if busy() {
        return;
    }
    busy.set(true);
    let result = copy_routine_inner(&routine_id, routines).await;
    busy.set(false);
    match result {
        Ok(()) => {
            error.set(None);
            sync::flush_in_background();
            reload.set((reload)() + 1);
        }
        Err(_) => error.set(Some(text(Key::RoutinesSaveFailed).to_owned())),
    }
}

async fn copy_routine_inner(
    routine_id: &str,
    routines: Signal<Vec<Routine>>,
) -> Result<(), StorageError> {
    let storage = storage::current().await?;
    let source = storage
        .routine(routine_id)
        .await?
        .ok_or_else(|| StorageError::InvalidOperation("the routine is missing".to_owned()))?;
    let new_id = new_uuid()?;
    let position = (routines)()
        .iter()
        .map(|routine| routine.position)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let (duplicate, operation) = routine_api::copy(&source, &new_id, position, &now_iso())?;
    storage.save_routine_copy(&duplicate, &operation).await
}

async fn delete_routine(
    routine_id: String,
    mut error: Signal<Option<String>>,
    mut busy: Signal<bool>,
    mut reload: Signal<u32>,
) {
    if busy() {
        return;
    }
    busy.set(true);
    let result = delete_routine_inner(&routine_id).await;
    busy.set(false);
    match result {
        Ok(()) => {
            error.set(None);
            sync::flush_in_background();
            reload.set((reload)() + 1);
        }
        Err(_) => error.set(Some(text(Key::RoutinesSaveFailed).to_owned())),
    }
}

async fn delete_routine_inner(routine_id: &str) -> Result<(), StorageError> {
    let storage = storage::current().await?;
    let operation = routine_api::delete(routine_id)?;
    storage.delete_routine(routine_id, &operation).await
}

async fn start_routine(
    routine_id: String,
    mut error: Signal<Option<String>>,
    mut busy: Signal<bool>,
) {
    if busy() {
        return;
    }
    busy.set(true);
    let result = start_routine_inner(&routine_id).await;
    busy.set(false);
    match result {
        Ok(training_id) => {
            error.set(None);
            navigate(&format!("/record/{training_id}"));
        }
        Err(_) => error.set(Some(text(Key::RoutinesStartFailed).to_owned())),
    }
}

async fn start_routine_inner(routine_id: &str) -> Result<String, StorageError> {
    routine_api::start_and_store(routine_id).await
}

fn routine_summary(routine: &Routine) -> String {
    if routine.exercises.is_empty() {
        return text(Key::RoutinesExerciseEmpty).to_owned();
    }
    routine
        .exercises
        .iter()
        .map(|exercise| {
            let target = &exercise.target;
            let value = match exercise.snapshot.record_mode {
                RecordMode::Time => target
                    .duration_seconds
                    .map(duration::format)
                    .unwrap_or_default(),
                RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => target
                    .reps
                    .map(|reps| format!("{reps}{}", text(Key::CalendarReps)))
                    .unwrap_or_default(),
            };
            format!("{} {}×{}", exercise.snapshot.name, target.set_count, value)
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

fn grip_option_label(option: GripSelection) -> String {
    match option {
        GripSelection::None => text(Key::RecordSetGripNone).to_owned(),
        GripSelection::Grip { style, width } => {
            format!("{} {}", grip_style_text(style), grip_width_text(width))
        }
        GripSelection::Attachment { attachment, style } => match style {
            Some(style) => format!("{} {}", grip_style_text(style), attachment_text(attachment)),
            None => attachment_text(attachment).to_owned(),
        },
    }
}

fn parse_optional_seconds(text: &str, max: u32) -> Option<u32> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    duration::parse(text).filter(|seconds| *seconds >= 1 && *seconds <= max)
}

fn parse_optional_weight(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>()
        .ok()
        .filter(|weight| weight.is_finite() && *weight >= 0.0)
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests;
