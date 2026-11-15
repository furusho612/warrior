//! 種目の詳細。名称、部位区分、主働筋、補助筋、使用器具、やり方、注意点、コツと、
//! お気に入りの登録、履歴とグラフへの入口を表示する。

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::exercise::{MuscleAssignment, MuscleRole, OtherScope};
use warrior_core::library::LibraryExercise;

use crate::calendar_view::part_label;
use crate::library::{self as library_api, equipment_label};
use crate::routes::Route;
use crate::screens::FavoriteIcon;
use crate::text::{text, Key};

/// 種目の詳細。
#[component]
pub fn ExerciseScreen(exercise_id: String) -> Element {
    let detail = use_signal(|| Option::<LibraryExercise>::None);
    let error = use_signal(|| Option::<String>::None);
    let action_error = use_signal(|| Option::<String>::None);
    let toggling = use_signal(|| false);
    let reload = use_signal(|| 0u32);
    let route_id = exercise_id.clone();
    use_effect(move || {
        let _ = (reload)();
        spawn(load(route_id.clone(), detail, error));
    });

    let current = (detail)();
    let load_error = (error)();
    let action_message = (action_error)();
    let busy = (toggling)();
    let loaded = current.is_some();
    let favorite = current.as_ref().is_some_and(|item| item.favorite);
    let history_id = exercise_id.clone();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-exercise-detail": "true",
            "data-exercise-id": "{exercise_id}",
            "data-exercise-loaded": if loaded { "true" } else { "false" },
            div { class: "flex items-center justify-between gap-2",
                Link {
                    to: Route::Library {},
                    class: "flex min-h-11 items-center text-[13px] text-text-sub",
                    "data-exercise-back": "true",
                    "{text(Key::ExerciseBack)}"
                }
                if let Some(item) = current.as_ref() {
                    button {
                        class: "flex min-h-11 min-w-11 items-center justify-center border border-line text-lg text-primary",
                        "data-exercise-favorite": "true",
                        "aria-pressed": if favorite { "true" } else { "false" },
                        "aria-label": if favorite { text(Key::LibraryFavoriteRemove) } else { text(Key::LibraryFavoriteAdd) },
                        disabled: busy,
                        onclick: {
                            let id = item.exercise.id.clone();
                            move |_| {
                                toggle_favorite(id.clone(), !favorite, detail, toggling, action_error);
                            }
                        },
                        if favorite {
                            FavoriteIcon { filled: true }
                        } else {
                            FavoriteIcon { filled: false }
                        }
                    }
                }
            }
            if let Some(message) = action_message {
                p { class: "text-[13px] text-primary", "{message}" }
            }
            if let Some(message) = load_error {
                p { class: "text-[13px] text-primary", "data-exercise-error": "true", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        let mut reload = reload;
                        reload += 1;
                    },
                    "{text(Key::ExerciseRetry)}"
                }
            } else if let Some(item) = current {
                ExerciseDetailBody { item }
                a {
                    class: "flex min-h-11 items-center justify-center border border-primary px-3 text-[13px] font-bold text-primary",
                    href: "/exercises/{history_id}/history",
                    "data-exercise-history": "true",
                    "{text(Key::ExerciseHistory)}"
                }
            } else {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordLoading)}" }
            }
        }
    }
}

/// 種目の内容。
#[component]
fn ExerciseDetailBody(item: LibraryExercise) -> Element {
    let exercise = item.exercise;
    let primary = muscle_names(&exercise.muscles, MuscleRole::Primary);
    let secondary = muscle_names(&exercise.muscles, MuscleRole::Secondary);
    let secondary = if secondary.is_empty() {
        text(Key::ExerciseNoMuscles).to_owned()
    } else {
        secondary
    };
    let instructions = exercise.instructions.trim();
    let cautions = exercise.cautions.trim();
    let tips = exercise.tips.trim();

    rsx! {
        h1 { class: "text-lg font-bold", "data-exercise-name": "true", "{exercise.name}" }
        dl { class: "flex flex-col gap-2",
            DetailField {
                label: text(Key::ExerciseBodyPart).to_owned(),
                value: part_label(exercise.body_part).to_owned(),
                field: "body-part"
            }
            if exercise.body_part.is_other() {
                DetailField {
                    label: text(Key::ExerciseOtherScope).to_owned(),
                    value: exercise.other_scope.map(scope_label).unwrap_or_default().to_owned(),
                    field: "other-scope"
                }
            } else {
                DetailField {
                    label: text(Key::ExercisePrimaryMuscles).to_owned(),
                    value: primary,
                    field: "primary"
                }
                DetailField {
                    label: text(Key::ExerciseSecondaryMuscles).to_owned(),
                    value: secondary,
                    field: "secondary"
                }
            }
            DetailField {
                label: text(Key::ExerciseEquipment).to_owned(),
                value: equipment_label(exercise.equipment).to_owned(),
                field: "equipment"
            }
            if !instructions.is_empty() {
                DetailField {
                    label: text(Key::ExerciseInstructions).to_owned(),
                    value: instructions.to_owned(),
                    field: "instructions"
                }
            }
            if !cautions.is_empty() {
                DetailField {
                    label: text(Key::ExerciseCautions).to_owned(),
                    value: cautions.to_owned(),
                    field: "cautions"
                }
            }
            if !tips.is_empty() {
                DetailField {
                    label: text(Key::ExerciseTips).to_owned(),
                    value: tips.to_owned(),
                    field: "tips"
                }
            }
        }
    }
}

/// 詳細の 1 項目。
#[component]
fn DetailField(label: String, value: String, field: &'static str) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-0.5 border-b border-line pb-2",
            "data-exercise-field": field,
            dt { class: "text-[11px] font-bold text-text-sub", "{label}" }
            dd { class: "text-[13px] leading-5", "{value}" }
        }
    }
}

fn muscle_names(muscles: &[MuscleAssignment], role: MuscleRole) -> String {
    muscles
        .iter()
        .filter(|assignment| assignment.role == role)
        .map(|assignment| assignment.muscle.name_ja())
        .collect::<Vec<_>>()
        .join("、")
}

fn scope_label(scope: OtherScope) -> &'static str {
    match scope {
        OtherScope::FullBody => text(Key::ExerciseScopeFullBody),
        OtherScope::LowerBody => text(Key::ExerciseScopeLowerBody),
    }
}

async fn load(
    exercise_id: String,
    mut detail: Signal<Option<LibraryExercise>>,
    mut error: Signal<Option<String>>,
) {
    error.set(None);
    match library_api::exercise(&exercise_id).await {
        Ok(item) => detail.set(Some(item)),
        Err(_) => error.set(Some(text(Key::ExerciseLoadFailed).to_owned())),
    }
}

fn toggle_favorite(
    exercise_id: String,
    favorite: bool,
    mut detail: Signal<Option<LibraryExercise>>,
    mut toggling: Signal<bool>,
    mut action_error: Signal<Option<String>>,
) {
    toggling.set(true);
    spawn(async move {
        match library_api::set_favorite(&exercise_id, favorite).await {
            Ok(result) => {
                if let Some(item) = detail.write().as_mut() {
                    item.favorite = result.favorite;
                }
                action_error.set(None);
            }
            Err(_) => action_error.set(Some(text(Key::ExerciseFavoriteFailed).to_owned())),
        }
        toggling.set(false);
    });
}
