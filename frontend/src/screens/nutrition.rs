//! 目標カロリーと PFC の目安の表示 (REQ-PRG-011、REQ-PRG-013、REQ-PRG-014、REQ-PRG-018)。
//!
//! 身体情報、目的、活動量、週あたりの日数が揃っている場合だけ、目標カロリーと
//! PFC のグラム数を表示する。揃っていない場合は未入力の項目を並べる。
//! 表示は目安であり、医療上や栄養指導上の助言ではないことを同じ枠に置く。

use dioxus::prelude::*;
use warrior_core::nutrition::{DisplayMacros, MissingField};

use crate::nutrition as nutrition_api;
use crate::text::{text, Key};

/// 目標カロリーと PFC の目安。
#[component]
pub fn NutritionPanel() -> Element {
    let view = use_signal(|| Option::<nutrition_api::NutritionView>::None);
    let error = use_signal(|| Option::<String>::None);
    use_effect(move || {
        spawn(load(view, error));
    });

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-nutrition-panel": "true",
            "data-nutrition-loaded": if (view)().is_some() { "true" } else { "false" },
            h2 { class: "text-sm font-bold", "{text(Key::NutritionSectionTitle)}" }
            if let Some(message) = (error)() {
                p { class: "text-[13px] text-primary", "{message}" }
            } else if let Some(view) = (view)() {
                if let Some(display) = view.display {
                    NutritionFigures { display }
                    if view.result.as_ref().is_some_and(|result| result.macro_clamped) {
                        p {
                            class: "text-[12px] leading-5 text-primary",
                            "data-nutrition-macro-clamped": "true",
                            "{text(Key::NutritionMacroClamped)}"
                        }
                    }
                    if view.result.and_then(|result| result.diet_loss_per_week).is_some() {
                        p {
                            class: "text-[12px] leading-5",
                            "data-nutrition-diet-range": "true",
                            "{text(Key::NutritionDietRange)}"
                        }
                    }
                } else {
                    div {
                        class: "flex flex-col gap-1",
                        "data-nutrition-missing": "true",
                        p { class: "text-[12px] leading-5 text-text-sub", "{text(Key::NutritionMissing)}" }
                        ul { class: "flex flex-wrap gap-2 text-[12px]",
                            for field in view.missing_fields {
                                li { "data-nutrition-missing-field": missing_field_name(field), "{missing_field_label(field)}" }
                            }
                        }
                    }
                }
                p {
                    class: "text-[11px] leading-5 text-text-sub",
                    "data-nutrition-notice": "true",
                    "{text(Key::NutritionNotice)}"
                }
            }
        }
    }
}

/// 目標カロリーと PFC の数値。
#[component]
fn NutritionFigures(display: DisplayMacros) -> Element {
    rsx! {
        dl {
            class: "grid grid-cols-2 gap-2 text-[13px]",
            "data-nutrition-figures": "true",
            Figure { label: text(Key::NutritionTargetKcal).to_owned(), value: format!("{} kcal", display.target_kcal), field: "target-kcal" }
            Figure { label: text(Key::NutritionProtein).to_owned(), value: format!("{} g", display.protein_g), field: "protein" }
            Figure { label: text(Key::NutritionFat).to_owned(), value: format!("{} g", display.fat_g), field: "fat" }
            Figure { label: text(Key::NutritionCarbs).to_owned(), value: format!("{} g", display.carbohydrate_g), field: "carbohydrate" }
        }
    }
}

#[component]
fn Figure(label: String, value: String, field: &'static str) -> Element {
    rsx! {
        div { class: "flex flex-col border-b border-line pb-1", "data-nutrition-figure": field,
            dt { class: "text-[11px] font-bold text-text-sub", "{label}" }
            dd { class: "font-condensed text-[16px] font-bold tabular-nums", "{value}" }
        }
    }
}

async fn load(
    mut view: Signal<Option<nutrition_api::NutritionView>>,
    mut error: Signal<Option<String>>,
) {
    match nutrition_api::fetch().await {
        Ok(value) => {
            error.set(None);
            view.set(Some(value));
        }
        Err(_) => error.set(Some(text(Key::NutritionLoadFailed).to_owned())),
    }
}

fn missing_field_label(field: MissingField) -> &'static str {
    match field {
        MissingField::Height => text(Key::ProfileHeightLabel),
        MissingField::Weight => text(Key::ProfileWeightLabel),
        MissingField::BirthDate => text(Key::ProfileBirthDateLabel),
        MissingField::Sex => text(Key::ProfileSexLabel),
        MissingField::Goal => text(Key::ProfileGoalLabel),
        MissingField::TrainingDays => text(Key::ProfileTrainingDaysLabel),
        MissingField::ActivityLevel => text(Key::ProfileActivityLabel),
    }
}

fn missing_field_name(field: MissingField) -> &'static str {
    match field {
        MissingField::Height => "height",
        MissingField::Weight => "weight",
        MissingField::BirthDate => "birth-date",
        MissingField::Sex => "sex",
        MissingField::Goal => "goal",
        MissingField::TrainingDays => "training-days",
        MissingField::ActivityLevel => "activity-level",
    }
}
