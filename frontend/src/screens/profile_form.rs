//! 身体情報の入力欄、選択肢、満年齢と BMI、注意書きの共通部品。

use dioxus::prelude::*;
use warrior_core::nutrition::{ActivityLevel, Goal, Sex};
use warrior_core::profile::{Profile, SafetyNotice, TrainingExperience};

use crate::profile::{
    activity_options, choice_class, experience_options, goal_options, pressed, sex_options, today,
    ProfileForm,
};
use crate::text::{text, Key};

/// 身体情報の入力欄と選択肢。`show_settings` が真のとき、目的、日数、活動量も出す。
#[component]
pub fn ProfileFields(form: Signal<ProfileForm>, show_settings: bool) -> Element {
    let current = form();
    let sex_choices = choice_options(&sex_options(), Sex::as_str, current.sex);
    let experience_choices = choice_options(
        &experience_options(),
        TrainingExperience::as_str,
        current.training_experience,
    );

    rsx! {
        div { class: "flex flex-col gap-3",
            BodyFieldInput {
                field: BodyField::Height,
                value: current.height_cm.clone(),
                on_input: move |value| {
                    let mut form = form;
                    form.write().height_cm = value;
                },
            }
            BodyFieldInput {
                field: BodyField::Weight,
                value: current.weight_kg.clone(),
                on_input: move |value| {
                    let mut form = form;
                    form.write().weight_kg = value;
                },
            }
            BodyFieldInput {
                field: BodyField::BirthDate,
                value: current.birth_date.clone(),
                on_input: move |value| {
                    let mut form = form;
                    form.write().birth_date = value;
                },
            }
            ChoiceGroup { label: text(Key::ProfileSexLabel) }
            div { "data-profile-sex": "true",
                ChoiceButtons {
                    options: sex_choices,
                    on_select: move |index| {
                        if let Some((value, _)) = sex_options().get(index) {
                            let mut form = form;
                            form.write().sex = Some(*value);
                        }
                    },
                }
            }
            ChoiceGroup { label: text(Key::ProfileExperienceLabel) }
            div { "data-profile-experience": "true",
                ChoiceButtons {
                    options: experience_choices,
                    on_select: move |index| {
                        if let Some((value, _)) = experience_options().get(index) {
                            let mut form = form;
                            form.write().training_experience = Some(*value);
                        }
                    },
                }
            }
            if show_settings {
                SettingsFields { form }
            }
        }
    }
}

/// 目的、週あたりの日数、日常の活動量の選択肢。
#[component]
fn SettingsFields(form: Signal<ProfileForm>) -> Element {
    let current = form();
    let goal_choices = choice_options(&goal_options(), Goal::as_str, current.goal);
    let activity_choices = choice_options(
        &activity_options(),
        ActivityLevel::as_str,
        current.activity_level,
    );

    rsx! {
        ChoiceGroup { label: text(Key::ProfileGoalLabel) }
        div { "data-profile-goal": "true",
            ChoiceButtons {
                options: goal_choices,
                on_select: move |index| {
                    if let Some((value, _)) = goal_options().get(index) {
                        let mut form = form;
                        form.write().goal = Some(*value);
                    }
                },
            }
        }
        ChoiceGroup { label: text(Key::ProfileTrainingDaysLabel) }
        div { "data-profile-days": "true",
            ChoiceButtons {
                options: day_choices(current.training_days_per_week),
                on_select: move |index| {
                    let mut form = form;
                    form.write().training_days_per_week = Some(index as u32);
                },
            }
        }
        ChoiceGroup { label: text(Key::ProfileActivityLabel) }
        div { "data-profile-activity": "true",
            ChoiceButtons {
                options: activity_choices,
                on_select: move |index| {
                    if let Some((value, _)) = activity_options().get(index) {
                        let mut form = form;
                        form.write().activity_level = Some(*value);
                    }
                },
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyField {
    Height,
    Weight,
    BirthDate,
}

impl BodyField {
    fn id(self) -> &'static str {
        match self {
            BodyField::Height => "profile-height",
            BodyField::Weight => "profile-weight",
            BodyField::BirthDate => "profile-birth-date",
        }
    }

    fn data_name(self) -> &'static str {
        match self {
            BodyField::Height => "height",
            BodyField::Weight => "weight",
            BodyField::BirthDate => "birth-date",
        }
    }

    fn label(self) -> Key {
        match self {
            BodyField::Height => Key::ProfileHeightLabel,
            BodyField::Weight => Key::ProfileWeightLabel,
            BodyField::BirthDate => Key::ProfileBirthDateLabel,
        }
    }

    fn input_type(self) -> &'static str {
        match self {
            BodyField::Height | BodyField::Weight => "number",
            BodyField::BirthDate => "text",
        }
    }

    fn inputmode(self) -> &'static str {
        match self {
            BodyField::Height | BodyField::Weight => "decimal",
            BodyField::BirthDate => "numeric",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            BodyField::Height | BodyField::Weight => "",
            BodyField::BirthDate => text(Key::ProfileBirthDatePlaceholder),
        }
    }
}

#[component]
fn BodyFieldInput(field: BodyField, value: String, on_input: EventHandler<String>) -> Element {
    let id = field.id();
    rsx! {
        div { class: "flex flex-col gap-1", "data-profile-field": field.data_name(),
            label { class: "text-[13px] font-bold", r#for: "{id}", "{text(field.label())}" }
            input {
                id: "{id}",
                r#type: field.input_type(),
                inputmode: field.inputmode(),
                step: if field == BodyField::BirthDate { "" } else { "0.1" },
                placeholder: field.placeholder(),
                class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                value: "{value}",
                oninput: move |event| on_input.call(event.value()),
            }
        }
    }
}

/// 選択肢 1 つ。`value` は E2E と表示に使う文字列、`selected` は選択の状態。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChoiceOption {
    value: String,
    label: String,
    selected: bool,
}

fn choice_options<T: Copy + PartialEq>(
    options: &[(T, Key)],
    value: impl Fn(T) -> &'static str,
    selected: Option<T>,
) -> Vec<ChoiceOption> {
    options
        .iter()
        .map(|(item, label)| ChoiceOption {
            value: value(*item).to_owned(),
            label: text(*label).to_owned(),
            selected: selected == Some(*item),
        })
        .collect()
}

fn day_choices(selected: Option<u32>) -> Vec<ChoiceOption> {
    (0..=7u32)
        .map(|days| ChoiceOption {
            value: days.to_string(),
            label: format!("{days} {}", text(Key::ProfileTrainingDaysUnit)),
            selected: selected == Some(days),
        })
        .collect()
}

#[component]
fn ChoiceButtons(options: Vec<ChoiceOption>, on_select: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "flex flex-wrap gap-2",
            for (index, option) in options.into_iter().enumerate() {
                {
                    let class = choice_class(option.selected);
                    rsx! {
                        button {
                            r#type: "button",
                            class,
                            "data-choice-value": "{option.value}",
                            "aria-pressed": pressed(option.selected),
                            onclick: move |_| on_select.call(index),
                            "{option.label}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChoiceGroup(label: String) -> Element {
    rsx! {
        div { class: "text-[13px] font-bold", "{label}" }
    }
}

/// 保存済みの満年齢と BMI。
#[component]
pub fn ProfileSummary(profile: Profile) -> Element {
    let current_date = today();
    let age = current_date.and_then(|date| profile.age_on(date));
    let bmi = profile.bmi();
    let age_value = age.map(|age| age.to_string()).unwrap_or_default();
    let bmi_value = bmi.map(|bmi| format!("{bmi:.1}")).unwrap_or_default();

    rsx! {
        dl { class: "flex flex-wrap gap-x-4 gap-y-1 text-[13px]",
            div { class: "flex gap-2",
                dt { class: "text-text-sub", "{text(Key::ProfileAgeLabel)}" }
                dd {
                    "data-profile-age": "{age_value}",
                    if let Some(age) = age {
                        "{age} {text(Key::ProfileAgeUnit)}"
                    } else {
                        "{text(Key::ProfileNotSet)}"
                    }
                }
            }
            div { class: "flex gap-2",
                dt { class: "text-text-sub", "{text(Key::ProfileBmiLabel)}" }
                dd {
                    "data-profile-bmi": "{bmi_value}",
                    if bmi.is_some() {
                        "{bmi_value}"
                    } else {
                        "{text(Key::ProfileNotSet)}"
                    }
                }
            }
        }
    }
}

/// 満年齢 18 歳未満または BMI 18.5 未満の利用者への注意書きと相談のガイド。
#[component]
pub fn SafetyNoticePanel(notice: SafetyNotice) -> Element {
    if !notice.needs_attention() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-primary bg-surface p-3",
            "data-profile-safety": "true",
            p { class: "text-[13px] leading-5", "{text(Key::ProfileSafetyNotice)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::ProfileSafetyConsultation)}" }
        }
    }
}
