//! 種目ライブラリの一覧と、記録に追加する種目の選択。
//!
//! 対応する要求: `REQ-LIB-002`、`REQ-LIB-006`、`REQ-LIB-007`、`REQ-REC-002`。
//! ライブラリの一覧は `GET /api/exercises` を 1 ページずつ取り、部位区分、使用器具、
//! お気に入り、名称の検索で絞り込む。記録への種目の選択は全件を取り、端末で
//! スナップショットを作って記録に保存する。

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::exercise::{BodyPart, Equipment, Exercise};
use warrior_core::library::LibraryExercise;

use crate::calendar_view::part_label;
use crate::library::{self as library_api, equipment_label, ExercisesQuery};
use crate::routes::{navigate, Route};
use crate::screens::FavoriteIcon;
use crate::storage;
use crate::sync;
use crate::text::{text, Key};
use crate::training_exercise;

/// 種目ライブラリの一覧、検索、絞り込み、お気に入り。
#[component]
pub fn LibraryScreen() -> Element {
    let state = use_library_state();
    let list = (state.items)();
    let list_count = list.len();
    let list_empty = list.is_empty();
    let loaded = (state.loaded)();
    let loading = (state.loading)();
    let error = (state.error)();
    let has_error = error.is_some();
    let action_error = (state.action_error)();
    let next_cursor = (state.next_cursor)();
    let body_part = (state.body_part)();
    let equipment = (state.equipment)();
    let favorite_only = (state.favorite_only)();
    let query = (state.query)();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-library": "true",
            "data-library-loaded": if loaded { "true" } else { "false" },
            "data-library-count": "{list_count}",
            "data-library-selected-part": body_part.map(BodyPart::as_str).unwrap_or("all"),
            "data-library-selected-equipment": equipment.map(Equipment::as_str).unwrap_or("all"),
            "data-library-selected-favorite": if favorite_only { "true" } else { "false" },
            h1 { class: "text-lg font-bold", "{text(Key::LibraryTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::LibraryDescription)}" }
            input {
                class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                r#type: "search",
                "data-library-query": "true",
                "aria-label": text(Key::LibrarySearchLabel),
                placeholder: text(Key::LibrarySearchPlaceholder),
                value: "{query}",
                oninput: move |event| {
                    let mut state = state;
                    state.query.set(event.value());
                },
            }
            div { class: "flex flex-col gap-1",
                span { class: "text-[11px] font-bold text-text-sub", "{text(Key::LibraryBodyPartFilter)}" }
                div { class: "flex flex-wrap gap-1",
                    button {
                        class: filter_class(body_part.is_none()),
                        "data-library-part": "all",
                        "aria-pressed": if body_part.is_none() { "true" } else { "false" },
                        onclick: move |_| {
                            let mut state = state;
                            state.body_part.set(None);
                        },
                        "{text(Key::LibraryFilterAll)}"
                    }
                    for part in BodyPart::ALL {
                        button {
                            class: filter_class(body_part == Some(part)),
                            "data-library-part": part.as_str(),
                            "aria-pressed": if body_part == Some(part) { "true" } else { "false" },
                            onclick: move |_| {
                                let mut state = state;
                                state.body_part.set(Some(part));
                            },
                            "{part_label(part)}"
                        }
                    }
                }
            }
            div { class: "flex flex-col gap-1",
                span { class: "text-[11px] font-bold text-text-sub", "{text(Key::LibraryEquipmentFilter)}" }
                div { class: "flex flex-wrap gap-1",
                    button {
                        class: filter_class(equipment.is_none()),
                        "data-library-equipment": "all",
                        "aria-pressed": if equipment.is_none() { "true" } else { "false" },
                        onclick: move |_| {
                            let mut state = state;
                            state.equipment.set(None);
                        },
                        "{text(Key::LibraryFilterAll)}"
                    }
                    for item in Equipment::ALL {
                        button {
                            class: filter_class(equipment == Some(item)),
                            "data-library-equipment": item.as_str(),
                            "aria-pressed": if equipment == Some(item) { "true" } else { "false" },
                            onclick: move |_| {
                                let mut state = state;
                                state.equipment.set(Some(item));
                            },
                            "{equipment_label(item)}"
                        }
                    }
                }
            }
            button {
                class: favorite_filter_class(favorite_only),
                "data-library-favorite-only": "true",
                "aria-pressed": if favorite_only { "true" } else { "false" },
                onclick: move |_| {
                    let mut state = state;
                    state.favorite_only.set(!favorite_only);
                },
                "{text(Key::LibraryFavoriteFilter)}"
            }
            if let Some(message) = action_error {
                p { class: "text-[13px] text-primary", "{message}" }
            }
            if let Some(message) = error.as_deref() {
                p { class: "text-[13px] text-primary", "data-library-error": "true", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        let mut state = state;
                        state.reload += 1;
                    },
                    "{text(Key::LibraryRetry)}"
                }
            } else if !loaded {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordLoading)}" }
            } else {
                if list_empty {
                    div {
                        class: "flex flex-col gap-1 border border-line bg-surface p-3",
                        "data-library-empty": "true",
                        p { class: "text-[13px] leading-5", "{text(Key::LibraryEmpty)}" }
                        p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::LibraryEmptyGuide)}" }
                    }
                }
                div { class: "flex flex-col gap-1", "data-library-list": "true",
                    for item in list {
                        {
                            let exercise_id = item.exercise.id.clone();
                            let favorite = item.favorite;
                            let name = item.exercise.name.clone();
                            let part = item.exercise.body_part;
                            let equipment = item.exercise.equipment;
                            rsx! {
                                article {
                                    class: "flex items-stretch gap-1 border border-line bg-surface",
                                    "data-library-exercise": "true",
                                    "data-exercise-id": "{exercise_id}",
                                    Link {
                                        to: Route::Exercise { exercise_id: exercise_id.clone() },
                                        class: "flex min-h-11 min-w-0 flex-1 flex-col justify-center gap-0.5 px-3 text-left",
                                        "data-library-open": "true",
                                        span { class: "truncate text-[13px] font-bold", "{name}" }
                                        span { class: "text-[11px] text-text-sub",
                                            "{part_label(part)} / {equipment_label(equipment)}"
                                        }
                                    }
                                    button {
                                        class: "min-w-11 border-l border-line px-3 text-[13px] text-primary",
                                        "data-library-favorite": "true",
                                        "aria-pressed": if favorite { "true" } else { "false" },
                                        "aria-label": if favorite { text(Key::LibraryFavoriteRemove) } else { text(Key::LibraryFavoriteAdd) },
                                        onclick: move |_| {
                                            toggle_favorite(state, exercise_id.clone(), !favorite);
                                        },
                                        if favorite {
                                            FavoriteIcon { filled: true }
                                        } else {
                                            FavoriteIcon { filled: false }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if loading && loaded {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordLoading)}" }
            }
            if !has_error {
                if let Some(cursor) = next_cursor {
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        "data-library-more": "true",
                        disabled: loading,
                        onclick: move |_| {
                            let request = PageLoad {
                                query: current_query(state),
                                cursor: Some(cursor.clone()),
                                generation: (state.generation)()
                            };
                            spawn(load_page(request, state));
                        },
                        "{text(Key::LibraryMore)}"
                    }
                }
            }
        }
    }
}

/// 一覧の状態。読み込みの結果もこの状態へ書き込む。
#[derive(Clone, Copy)]
struct LibraryState {
    body_part: Signal<Option<BodyPart>>,
    equipment: Signal<Option<Equipment>>,
    favorite_only: Signal<bool>,
    query: Signal<String>,
    items: Signal<Vec<LibraryExercise>>,
    next_cursor: Signal<Option<String>>,
    loaded: Signal<bool>,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
    action_error: Signal<Option<String>>,
    reload: Signal<u32>,
    generation: Signal<u32>,
}

/// 一覧の状態を作り、絞り込みの条件が変わるたびに最初のページを読む効果を登録する。
fn use_library_state() -> LibraryState {
    let state = LibraryState {
        body_part: use_signal(|| Option::<BodyPart>::None),
        equipment: use_signal(|| Option::<Equipment>::None),
        favorite_only: use_signal(|| false),
        query: use_signal(String::new),
        items: use_signal(Vec::<LibraryExercise>::new),
        next_cursor: use_signal(|| Option::<String>::None),
        loaded: use_signal(|| false),
        loading: use_signal(|| false),
        error: use_signal(|| Option::<String>::None),
        action_error: use_signal(|| Option::<String>::None),
        reload: use_signal(|| 0u32),
        generation: use_signal(|| 0u32),
    };
    use_effect(move || {
        let mut state = state;
        let _ = (state.reload)();
        let generation = state.generation.peek().wrapping_add(1);
        state.generation.set(generation);
        state.items.set(Vec::new());
        state.next_cursor.set(None);
        state.loaded.set(false);
        state.error.set(None);
        state.action_error.set(None);
        let request = PageLoad {
            query: current_query(state),
            cursor: None,
            generation,
        };
        spawn(async move {
            load_page(request, state).await;
        });
    });
    state
}

fn current_query(state: LibraryState) -> ExercisesQuery {
    ExercisesQuery {
        body_part: (state.body_part)(),
        equipment: (state.equipment)(),
        favorite_only: (state.favorite_only)(),
        query: (state.query)(),
    }
}

struct PageLoad {
    query: ExercisesQuery,
    cursor: Option<String>,
    generation: u32,
}

async fn load_page(request: PageLoad, mut state: LibraryState) {
    state.loading.set(true);
    let result = library_api::page(&request.query, request.cursor.as_deref()).await;
    if (state.generation)() != request.generation {
        return;
    }
    match result {
        Ok(page) => {
            if request.cursor.is_some() {
                state.items.write().extend(page.exercises);
            } else {
                state.items.set(page.exercises);
            }
            state.next_cursor.set(page.next_cursor);
            state.loaded.set(true);
            state.error.set(None);
        }
        Err(_) => state
            .error
            .set(Some(text(Key::LibraryLoadFailed).to_owned())),
    }
    state.loading.set(false);
}

fn toggle_favorite(mut state: LibraryState, exercise_id: String, favorite: bool) {
    spawn(async move {
        match library_api::set_favorite(&exercise_id, favorite).await {
            Ok(result) => {
                state.action_error.set(None);
                if (state.favorite_only)() {
                    state.reload += 1;
                    return;
                }
                let mut items = state.items.write();
                if let Some(item) = items
                    .iter_mut()
                    .find(|item| item.exercise.id == exercise_id)
                {
                    item.favorite = result.favorite;
                }
            }
            Err(_) => state
                .action_error
                .set(Some(text(Key::ExerciseFavoriteFailed).to_owned())),
        }
    });
}

fn filter_class(active: bool) -> &'static str {
    if active {
        "min-h-11 border border-primary bg-primary px-2.5 text-[12px] font-bold text-on-primary"
    } else {
        "min-h-11 border border-line px-2.5 text-[12px]"
    }
}

fn favorite_filter_class(active: bool) -> &'static str {
    if active {
        "min-h-11 self-start border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary"
    } else {
        "min-h-11 self-start border border-line px-3 text-[13px]"
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
                    p { class: "text-[13px] text-text-sub", "{text(Key::LibraryEmpty)}" }
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
    match library_api::all().await {
        Ok(response) => exercises.set(Some(
            response
                .exercises
                .into_iter()
                .map(|item| item.exercise)
                .collect(),
        )),
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
