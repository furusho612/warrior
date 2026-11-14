//! 種目ライブラリの選択画面。
//!
//! 記録中のトレーニングに追加する種目を選ぶ。種目の一覧は `GET /api/exercises` から取り、
//! 選んだ種目のスナップショットを端末で作って記録に保存する。

use dioxus::prelude::*;
use warrior_core::exercise::Exercise;

use crate::calendar_view::part_label;
use crate::library as library_api;
use crate::routes::navigate;
use crate::screens::ScreenFrame;
use crate::storage;
use crate::sync;
use crate::text::{text, Key};
use crate::training_exercise;

#[component]
pub fn LibraryScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::LibraryTitle).to_owned(),
            description: text(Key::LibraryDescription).to_owned()
        }
    }
}

/// 記録に追加する種目を選ぶ画面。
#[component]
pub fn ExerciseSelectScreen(training_id: String) -> Element {
    let exercises = use_signal(|| Option::<Vec<Exercise>>::None);
    let error = use_signal(|| Option::<String>::None);
    let view_only = use_signal(|| false);
    let adding = use_signal(|| Option::<String>::None);
    let route_id = training_id.clone();
    let retry_id = training_id.clone();
    use_effect(move || {
        spawn(load(route_id.clone(), exercises, error, view_only));
    });

    let list = (exercises)();
    let load_error = (error)();
    let busy = (adding)().is_some();
    let read_only = (view_only)();
    let back_id = training_id.clone();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-exercise-select": "true",
            div { class: "flex items-center justify-between gap-2",
                h1 { class: "text-lg font-bold", "{text(Key::RecordExerciseSelectTitle)}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-exercise-select-back": "true",
                    onclick: move |_| navigate(&format!("/record/{back_id}")),
                    "{text(Key::RecordExerciseBack)}"
                }
            }
            p { class: "text-[13px] text-text-sub", "{text(Key::RecordExerciseSelectDescription)}" }
            if read_only {
                p { class: "text-[13px] text-primary", "{text(Key::RecordViewOnly)}" }
            }
            if let Some(message) = load_error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        let id = retry_id.clone();
                        spawn(load(id, exercises, error, view_only));
                    },
                    "{text(Key::RecordRetry)}"
                }
            } else if let Some(list) = list {
                if list.is_empty() {
                    p { class: "text-[13px] text-text-sub", "{text(Key::RecordExerciseEmpty)}" }
                }
                div { class: "flex flex-col gap-1", "data-exercise-choices": "true",
                    for exercise in list {
                        button {
                            class: "flex min-h-11 items-center justify-between gap-2 border border-line bg-surface px-3 text-left",
                            "data-exercise-choose": "true",
                            "data-exercise-id": "{exercise.id}",
                            disabled: busy || read_only,
                            onclick: {
                                let training_id = training_id.clone();
                                let exercise = exercise.clone();
                                move |_| {
                                    let training_id = training_id.clone();
                                    let exercise = exercise.clone();
                                    spawn(add(
                                        training_id,
                                        exercise,
                                        adding,
                                        error,
                                    ));
                                }
                            },
                            span { class: "text-[13px] font-bold", "{exercise.name}" }
                            span { class: "text-[11px] text-text-sub", "{part_label(exercise.body_part)}" }
                        }
                    }
                }
            } else {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordLoading)}" }
            }
        }
    }
}

async fn load(
    training_id: String,
    mut exercises: Signal<Option<Vec<Exercise>>>,
    mut error: Signal<Option<String>>,
    mut view_only: Signal<bool>,
) {
    error.set(None);
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            error.set(Some(text(Key::RecordExerciseSelectFailed).to_owned()));
            return;
        }
    };
    let forbidden = storage.forbidden_trainings().await.unwrap_or_default();
    view_only.set(forbidden.iter().any(|id| id == &training_id));
    match library_api::exercises().await {
        Ok(response) => exercises.set(Some(response.exercises)),
        Err(_) => error.set(Some(text(Key::RecordExerciseSelectFailed).to_owned())),
    }
}

async fn add(
    training_id: String,
    exercise: Exercise,
    mut adding: Signal<Option<String>>,
    mut error: Signal<Option<String>>,
) {
    adding.set(Some(exercise.id.clone()));
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            adding.set(None);
            error.set(Some(text(Key::RecordExerciseSaveFailed).to_owned()));
            return;
        }
    };
    let existing = storage
        .training_exercises(&training_id)
        .await
        .unwrap_or_default();
    let position = existing
        .iter()
        .map(|exercise| exercise.position)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let created = training_exercise::create(&training_id, &exercise, position);
    let result = match created {
        Ok((record, operation)) => {
            storage
                .save_training_exercise_create(&record, &operation)
                .await
        }
        Err(error) => Err(error),
    };
    adding.set(None);
    if result.is_err() {
        error.set(Some(text(Key::RecordExerciseSaveFailed).to_owned()));
        return;
    }
    sync::flush_in_background();
    navigate(&format!("/record/{training_id}"));
}
