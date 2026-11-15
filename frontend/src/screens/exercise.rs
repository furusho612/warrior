//! 種目の詳細。名称、部位区分、主働筋、補助筋、使用器具、やり方、注意点、コツと、
//! お気に入りの登録、履歴とグラフへの入口を表示する。

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::exercise::{GripMode, GripSelection, GripStyle, MuscleUnit, OtherScope};
use warrior_core::library::LibraryExercise;

use crate::body_figure::{figure_muscles, figure_svg, FigureMuscles, FigureSide};
use crate::calendar_view::part_label;
use crate::grip_options::{
    attachment_styles, attachment_with, attachment_with_style, grip_attachments, grip_styles,
    grip_widths, grip_with_style, grip_with_width, initial_grip,
};
use crate::grip_text::{
    attachment_from_key, attachment_text, grip_style_from_key, grip_style_text,
    grip_width_from_key, grip_width_text,
};
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
    let grip = use_signal(|| Option::<GripSelection>::None);
    let route_id = exercise_id.clone();
    use_effect(move || {
        let _ = (reload)();
        spawn(load(route_id.clone(), detail, error, grip));
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
                ExerciseDetailBody { item, grip }
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
fn ExerciseDetailBody(item: LibraryExercise, grip: Signal<Option<GripSelection>>) -> Element {
    let exercise = item.exercise;
    let selection = (grip)();
    let muscles = figure_muscles(&exercise, selection);
    let primary = muscle_names(&muscles.primary);
    let secondary = muscle_names(&muscles.secondary);
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
        if let Some(selection) = selection {
            GripPreviewSelector {
                mode: exercise.grip_mode,
                options: exercise.grip_options.clone(),
                selection,
                grip,
            }
        }
        if !exercise.body_part.is_other() {
            BodyFigure { muscles }
        }
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

/// 種目の詳細の人体図の色を確かめるためのグリップの選択。
///
/// この選択は表示の切り替えだけに使い、記録には保存しない。
#[component]
fn GripPreviewSelector(
    mode: GripMode,
    options: Vec<GripSelection>,
    selection: GripSelection,
    grip: Signal<Option<GripSelection>>,
) -> Element {
    match mode {
        GripMode::None => rsx! {},
        GripMode::Grip => rsx! {
            GripPreviewFields { selection, options, grip }
        },
        GripMode::Attachment => rsx! {
            AttachmentPreviewFields { selection, options, grip }
        },
    }
}

/// 握り方と手幅を選ぶ種目のグリップの選択。
#[component]
fn GripPreviewFields(
    selection: GripSelection,
    options: Vec<GripSelection>,
    grip: Signal<Option<GripSelection>>,
) -> Element {
    let GripSelection::Grip { style, width } = selection else {
        return rsx! {};
    };
    let style_values = style_select_options(&options);
    let width_values = width_select_options(&options, style);
    let style_options = options.clone();
    let width_options = options;
    rsx! {
        div {
            class: "flex flex-wrap items-center gap-2",
            "data-exercise-grip-selector": "true",
            GripPreviewSelect {
                label: text(Key::ExerciseGripStyle).to_owned(),
                field: "style",
                value: style.as_str().to_owned(),
                options: style_values,
                on_change: move |value: String| {
                    if let Some(style) = grip_style_from_key(&value) {
                        let mut grip = grip;
                        grip.set(Some(grip_with_style(selection, style, &style_options)));
                    }
                },
            }
            GripPreviewSelect {
                label: text(Key::ExerciseGripWidth).to_owned(),
                field: "width",
                value: width.as_str().to_owned(),
                options: width_values,
                on_change: move |value: String| {
                    if let Some(width) = grip_width_from_key(&value) {
                        let mut grip = grip;
                        grip.set(Some(grip_with_width(selection, width, &width_options)));
                    }
                },
            }
        }
    }
}

/// アタッチメントと、必要なら握り方を選ぶ種目のグリップの選択。
#[component]
fn AttachmentPreviewFields(
    selection: GripSelection,
    options: Vec<GripSelection>,
    grip: Signal<Option<GripSelection>>,
) -> Element {
    let GripSelection::Attachment { attachment, style } = selection else {
        return rsx! {};
    };
    let attachment_values = attachment_select_options(&options);
    let styles = attachment_styles(&options, attachment);
    let current_style = style.or_else(|| styles.first().copied());
    let style_values = styles
        .into_iter()
        .map(|style| (style.as_str().to_owned(), grip_style_text(style).to_owned()))
        .collect();
    let attachment_options = options.clone();
    let style_options = options;
    rsx! {
        div {
            class: "flex flex-wrap items-center gap-2",
            "data-exercise-grip-selector": "true",
            GripPreviewSelect {
                label: text(Key::ExerciseGripAttachment).to_owned(),
                field: "attachment",
                value: attachment.as_str().to_owned(),
                options: attachment_values,
                on_change: move |value: String| {
                    if let Some(attachment) = attachment_from_key(&value) {
                        let mut grip = grip;
                        grip.set(Some(attachment_with(
                            selection,
                            attachment,
                            &attachment_options,
                        )));
                    }
                },
            }
            if let Some(current_style) = current_style {
                GripPreviewSelect {
                    label: text(Key::ExerciseGripStyle).to_owned(),
                    field: "style",
                    value: current_style.as_str().to_owned(),
                    options: style_values,
                    on_change: move |value: String| {
                        if let Some(style) = grip_style_from_key(&value) {
                            let mut grip = grip;
                            grip.set(Some(attachment_with_style(
                                selection,
                                style,
                                &style_options,
                            )));
                        }
                    },
                }
            }
        }
    }
}

/// 種目の詳細のグリップの選択欄。
#[component]
fn GripPreviewSelect(
    label: String,
    field: &'static str,
    value: String,
    options: Vec<(String, String)>,
    on_change: EventHandler<String>,
) -> Element {
    let current = value.clone();
    rsx! {
        label {
            class: "flex items-center gap-1 text-[11px] font-bold",
            "{label}"
            select {
                class: "min-h-11 border border-line bg-bg px-1 text-[12px]",
                "data-exercise-grip-field": field,
                "aria-label": "{label}",
                value: "{value}",
                onchange: move |event| on_change.call(event.value()),
                for (option_value, option_label) in options {
                    option {
                        value: "{option_value}",
                        selected: option_value == current,
                        "{option_label}"
                    }
                }
            }
        }
    }
}

fn style_select_options(options: &[GripSelection]) -> Vec<(String, String)> {
    grip_styles(options)
        .into_iter()
        .map(|style| (style.as_str().to_owned(), grip_style_text(style).to_owned()))
        .collect()
}

fn width_select_options(options: &[GripSelection], style: GripStyle) -> Vec<(String, String)> {
    grip_widths(options, style)
        .into_iter()
        .map(|width| (width.as_str().to_owned(), grip_width_text(width).to_owned()))
        .collect()
}

fn attachment_select_options(options: &[GripSelection]) -> Vec<(String, String)> {
    grip_attachments(options)
        .into_iter()
        .map(|attachment| {
            (
                attachment.as_str().to_owned(),
                attachment_text(attachment).to_owned(),
            )
        })
        .collect()
}

/// 前面と背面の人体図と、その面の名前。
#[component]
fn BodyFigure(muscles: FigureMuscles) -> Element {
    rsx! {
        div {
            class: "flex w-full max-w-[240px] gap-2 self-center",
            "data-body-figure": "true",
            for side in [FigureSide::Front, FigureSide::Back] {
                div { class: "flex min-w-0 flex-1 flex-col items-center gap-1",
                    div {
                        class: "w-full",
                        dangerous_inner_html: figure_svg(side, &muscles),
                    }
                    span { class: "text-[11px] text-text-sub", "{side_label(side)}" }
                }
            }
        }
    }
}

fn side_label(side: FigureSide) -> &'static str {
    match side {
        FigureSide::Front => text(Key::ExerciseFigureFront),
        FigureSide::Back => text(Key::ExerciseFigureBack),
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

fn muscle_names(muscles: &[MuscleUnit]) -> String {
    muscles
        .iter()
        .map(|muscle| muscle.name_ja())
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
    mut grip: Signal<Option<GripSelection>>,
) {
    error.set(None);
    match library_api::exercise(&exercise_id).await {
        Ok(item) => {
            grip.set(initial_grip(
                item.exercise.grip_mode,
                &item.exercise.grip_options,
            ));
            detail.set(Some(item));
        }
        Err(_) => {
            grip.set(None);
            error.set(Some(text(Key::ExerciseLoadFailed).to_owned()));
        }
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
