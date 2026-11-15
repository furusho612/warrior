//! 種目の追加と編集の入力欄。
//!
//! 名称、読み、部位区分、記録方式を必須とし、使用器具、左右別、範囲、筋肉、
//! グリップ、やり方、注意点、コツを任意で受け取る。入力の検証は
//! `Exercise::validate` と同じ規則を保存の前に適用する。

use dioxus::prelude::*;
use warrior_core::exercise::{
    Attachment, BodyPart, Equipment, ExerciseInput, GripMode, GripSelection, GripStyle, GripWidth,
    Laterality, MuscleAssignment, MuscleRole, MuscleUnit, OtherScope, RecordMode,
};

use crate::calendar_view::part_label;
use crate::library::equipment_label;
use crate::text::{text, Key};

/// 種目の入力欄。
#[component]
pub fn ExerciseForm(
    initial: ExerciseInput,
    title: String,
    busy: bool,
    on_submit: EventHandler<ExerciseInput>,
    on_cancel: EventHandler<()>,
) -> Element {
    let state = use_signal(|| initial);
    let mut error = use_signal(|| Option::<String>::None);
    let current = state();
    let is_other = current.body_part == BodyPart::Other;

    rsx! {
        div {
            class: "flex flex-col gap-3 border border-line bg-surface p-3",
            "data-exercise-form": "true",
            h2 { class: "text-[15px] font-bold", "{title}" }
            if let Some(message) = (error)() {
                p { class: "text-[13px] text-primary", "data-exercise-form-error": "true", "{message}" }
            }
            FormTextField {
                label: text(Key::ExerciseFormName).to_owned(),
                field: "name",
                value: current.name.clone(),
                on_input: move |value: String| {
                    let mut state = state;
                    state.write().name = value;
                },
            }
            FormTextField {
                label: text(Key::ExerciseFormKana).to_owned(),
                field: "kana",
                value: current.name_kana.clone(),
                on_input: move |value: String| {
                    let mut state = state;
                    state.write().name_kana = value;
                },
            }
            FormChoice {
                label: text(Key::ExerciseFormBodyPart).to_owned(),
                field: "body-part",
                value: current.body_part.as_str().to_owned(),
                options: BodyPart::ALL
                    .into_iter()
                    .map(|part| (part.as_str().to_owned(), part_label(part).to_owned()))
                    .collect(),
                on_change: move |value: String| {
                    let Some(part) = BodyPart::parse(&value) else {
                        return;
                    };
                    let mut state = state;
                    let mut form = state.write();
                    form.body_part = part;
                    if part.is_other() {
                        form.muscles.clear();
                        form.other_scope.get_or_insert(OtherScope::FullBody);
                    } else {
                        form.other_scope = None;
                    }
                },
            }
            FormChoice {
                label: text(Key::ExerciseFormEquipment).to_owned(),
                field: "equipment",
                value: current.equipment.as_str().to_owned(),
                options: Equipment::ALL
                    .into_iter()
                    .map(|item| (item.as_str().to_owned(), equipment_label(item).to_owned()))
                    .collect(),
                on_change: move |value: String| {
                    let Some(equipment) = Equipment::parse(&value) else {
                        return;
                    };
                    let mut state = state;
                    state.write().equipment = equipment;
                },
            }
            FormChoice {
                label: text(Key::ExerciseFormRecordMode).to_owned(),
                field: "record-mode",
                value: record_mode_value(current.record_mode).to_owned(),
                options: vec![
                    (
                        record_mode_value(RecordMode::WeightAndReps).to_owned(),
                        text(Key::ExerciseFormRecordWeight).to_owned(),
                    ),
                    (
                        record_mode_value(RecordMode::BodyweightAndReps).to_owned(),
                        text(Key::ExerciseFormRecordBodyweight).to_owned(),
                    ),
                    (
                        record_mode_value(RecordMode::Time).to_owned(),
                        text(Key::ExerciseFormRecordTime).to_owned(),
                    ),
                ],
                on_change: move |value: String| {
                    let Some(mode) = parse_record_mode(&value) else {
                        return;
                    };
                    let mut state = state;
                    let mut form = state.write();
                    form.record_mode = mode;
                    if mode == RecordMode::Time {
                        form.grip_mode = GripMode::None;
                        form.grip_options.clear();
                    }
                },
            }
            FormChoice {
                label: text(Key::ExerciseFormLaterality).to_owned(),
                field: "laterality",
                value: laterality_value(current.laterality).to_owned(),
                options: vec![
                    (
                        laterality_value(Laterality::Bilateral).to_owned(),
                        text(Key::ExerciseFormBilateral).to_owned(),
                    ),
                    (
                        laterality_value(Laterality::Unilateral).to_owned(),
                        text(Key::ExerciseFormUnilateral).to_owned(),
                    ),
                ],
                on_change: move |value: String| {
                    let Some(laterality) = parse_laterality(&value) else {
                        return;
                    };
                    let mut state = state;
                    state.write().laterality = laterality;
                },
            }
            if is_other {
                FormChoice {
                    label: text(Key::ExerciseFormOtherScope).to_owned(),
                    field: "other-scope",
                    value: current
                        .other_scope
                        .map(other_scope_value)
                        .unwrap_or(other_scope_value(OtherScope::FullBody))
                        .to_owned(),
                    options: vec![
                        (
                            other_scope_value(OtherScope::FullBody).to_owned(),
                            text(Key::ExerciseScopeFullBody).to_owned(),
                        ),
                        (
                            other_scope_value(OtherScope::LowerBody).to_owned(),
                            text(Key::ExerciseScopeLowerBody).to_owned(),
                        ),
                    ],
                    on_change: move |value: String| {
                        let Some(scope) = parse_other_scope(&value) else {
                            return;
                        };
                        let mut state = state;
                        state.write().other_scope = Some(scope);
                    },
                }
            } else {
                MuscleChecks { state }
            }
            FormChoice {
                label: text(Key::ExerciseFormGripMode).to_owned(),
                field: "grip-mode",
                value: grip_mode_value(current.grip_mode).to_owned(),
                options: vec![
                    (
                        grip_mode_value(GripMode::None).to_owned(),
                        text(Key::ExerciseFormGripNone).to_owned(),
                    ),
                    (
                        grip_mode_value(GripMode::Grip).to_owned(),
                        text(Key::ExerciseFormGrip).to_owned(),
                    ),
                    (
                        grip_mode_value(GripMode::Attachment).to_owned(),
                        text(Key::ExerciseFormAttachment).to_owned(),
                    ),
                ],
                on_change: move |value: String| {
                    let Some(mode) = parse_grip_mode(&value) else {
                        return;
                    };
                    let mut state = state;
                    let mut form = state.write();
                    form.grip_mode = mode;
                    form.grip_options.clear();
                },
            }
            if current.grip_mode == GripMode::Grip {
                GripChecks { state }
            }
            if current.grip_mode == GripMode::Attachment {
                AttachmentChecks { state }
            }
            FormTextArea {
                label: text(Key::ExerciseFormInstructions).to_owned(),
                field: "instructions",
                value: current.instructions.clone(),
                on_input: move |value: String| {
                    let mut state = state;
                    state.write().instructions = value;
                },
            }
            FormTextArea {
                label: text(Key::ExerciseFormCautions).to_owned(),
                field: "cautions",
                value: current.cautions.clone(),
                on_input: move |value: String| {
                    let mut state = state;
                    state.write().cautions = value;
                },
            }
            FormTextArea {
                label: text(Key::ExerciseFormTips).to_owned(),
                field: "tips",
                value: current.tips.clone(),
                on_input: move |value: String| {
                    let mut state = state;
                    state.write().tips = value;
                },
            }
            div { class: "flex gap-2",
                button {
                    class: "min-h-11 flex-1 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-exercise-form-save": "true",
                    disabled: busy,
                    onclick: move |_| {
                        let input = state();
                        if input.name.trim().is_empty()
                            || input.name_kana.trim().is_empty()
                            || input.body_part.is_other() && input.other_scope.is_none()
                        {
                            error.set(Some(text(Key::ExerciseFormRequired).to_owned()));
                            return;
                        }
                        let exercise = input
                            .clone()
                            .into_exercise("3f2504e0-4f89-41d3-9a0c-0305e82c3301".to_owned(), 0);
                        if exercise.validate().is_err() {
                            error.set(Some(text(Key::ExerciseFormInvalid).to_owned()));
                            return;
                        }
                        error.set(None);
                        on_submit.call(input);
                    },
                    "{text(Key::ExerciseFormSave)}"
                }
                button {
                    class: "min-h-11 flex-1 border border-line px-3 text-[13px]",
                    "data-exercise-form-cancel": "true",
                    disabled: busy,
                    onclick: move |_| on_cancel.call(()),
                    "{text(Key::ExerciseFormCancel)}"
                }
            }
        }
    }
}

/// 主働筋と補助筋のチェック。
#[component]
fn MuscleChecks(state: Signal<ExerciseInput>) -> Element {
    let current = state();
    rsx! {
        div { class: "flex flex-col gap-1", "data-exercise-form-muscles": "true",
            span { class: "text-[11px] font-bold text-text-sub", "{text(Key::ExerciseFormPrimary)} / {text(Key::ExerciseFormSecondary)}" }
            for unit in MuscleUnit::ALL {
                {
                    let primary = has_role(&current.muscles, unit, MuscleRole::Primary);
                    let secondary = has_role(&current.muscles, unit, MuscleRole::Secondary);
                    rsx! {
                        div { class: "flex items-center justify-between gap-2 border-b border-line py-1",
                            span { class: "text-[12px]", "{unit.name_ja()}" }
                            div { class: "flex gap-2",
                                label { class: "flex items-center gap-1 text-[11px]",
                                    input {
                                        r#type: "checkbox",
                                        checked: primary,
                                        "data-exercise-form-primary": unit.id(),
                                        onchange: move |_| {
                                            let mut state = state;
                                            toggle_role(&mut state.write().muscles, unit, MuscleRole::Primary);
                                        },
                                    }
                                    "{text(Key::ExerciseFormPrimary)}"
                                }
                                label { class: "flex items-center gap-1 text-[11px]",
                                    input {
                                        r#type: "checkbox",
                                        checked: secondary,
                                        "data-exercise-form-secondary": unit.id(),
                                        onchange: move |_| {
                                            let mut state = state;
                                            toggle_role(&mut state.write().muscles, unit, MuscleRole::Secondary);
                                        },
                                    }
                                    "{text(Key::ExerciseFormSecondary)}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 握り方と手幅のチェック。
#[component]
fn GripChecks(state: Signal<ExerciseInput>) -> Element {
    let current = state();
    rsx! {
        div { class: "grid grid-cols-3 gap-1", "data-exercise-form-grips": "true",
            for style in GripStyle::ALL {
                for width in GripWidth::ALL {
                    label { class: "flex items-center gap-1 text-[11px]",
                        input {
                            r#type: "checkbox",
                            checked: current.grip_options.contains(&GripSelection::Grip { style, width }),
                            "data-exercise-form-grip": format!("{}_{}", style.as_str(), width.as_str()),
                            onchange: move |_| {
                                let mut state = state;
                                toggle_grip(&mut state.write().grip_options, GripSelection::Grip { style, width });
                            },
                        }
                        "{grip_label(style)} {grip_width_label(width)}"
                    }
                }
            }
        }
    }
}

/// アタッチメントのチェック。
#[component]
fn AttachmentChecks(state: Signal<ExerciseInput>) -> Element {
    let current = state();
    rsx! {
        div { class: "flex flex-col gap-1", "data-exercise-form-attachments": "true",
            for attachment in Attachment::ALL {
                div { class: "flex items-center justify-between gap-2 border-b border-line py-1",
                    span { class: "text-[12px]", "{attachment_label(attachment)}" }
                    if attachment.requires_style() {
                        div { class: "flex gap-2",
                            for style in [GripStyle::Pronated, GripStyle::Supinated] {
                                label { class: "flex items-center gap-1 text-[11px]",
                                    input {
                                        r#type: "checkbox",
                                        checked: current.grip_options.contains(&GripSelection::Attachment { attachment, style: Some(style) }),
                                        "data-exercise-form-attachment": format!("{}_{}", attachment.as_str(), style.as_str()),
                                        onchange: move |_| {
                                            let mut state = state;
                                            toggle_grip(&mut state.write().grip_options, GripSelection::Attachment { attachment, style: Some(style) });
                                        },
                                    }
                                    "{grip_label(style)}"
                                }
                            }
                        }
                    } else {
                        label { class: "flex items-center gap-1 text-[11px]",
                            input {
                                r#type: "checkbox",
                                checked: current.grip_options.contains(&GripSelection::Attachment { attachment, style: None }),
                                "data-exercise-form-attachment": attachment.as_str(),
                                onchange: move |_| {
                                    let mut state = state;
                                    toggle_grip(&mut state.write().grip_options, GripSelection::Attachment { attachment, style: None });
                                },
                            }
                            "{text(Key::ExerciseFormAttachment)}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FormTextField(
    label: String,
    field: &'static str,
    value: String,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "flex flex-col gap-1", "data-exercise-form-field": field,
            span { class: "text-[11px] font-bold text-text-sub", "{label}" }
            input {
                class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                "data-exercise-form-input": field,
                value: "{value}",
                oninput: move |event| on_input.call(event.value()),
            }
        }
    }
}

#[component]
fn FormTextArea(
    label: String,
    field: &'static str,
    value: String,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "flex flex-col gap-1", "data-exercise-form-field": field,
            span { class: "text-[11px] font-bold text-text-sub", "{label}" }
            textarea {
                class: "min-h-20 border border-line bg-surface px-3 py-2 text-[13px]",
                "data-exercise-form-input": field,
                value: "{value}",
                oninput: move |event| on_input.call(event.value()),
            }
        }
    }
}

#[component]
fn FormChoice(
    label: String,
    field: &'static str,
    value: String,
    options: Vec<(String, String)>,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "flex flex-col gap-1", "data-exercise-form-field": field,
            span { class: "text-[11px] font-bold text-text-sub", "{label}" }
            select {
                class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                "data-exercise-form-input": field,
                value: "{value}",
                onchange: move |event| on_change.call(event.value()),
                for (option_value, option_label) in options {
                    option { value: "{option_value}", "{option_label}" }
                }
            }
        }
    }
}

fn record_mode_value(mode: RecordMode) -> &'static str {
    match mode {
        RecordMode::WeightAndReps => "weight_and_reps",
        RecordMode::BodyweightAndReps => "bodyweight_and_reps",
        RecordMode::Time => "time",
    }
}

fn parse_record_mode(value: &str) -> Option<RecordMode> {
    match value {
        "weight_and_reps" => Some(RecordMode::WeightAndReps),
        "bodyweight_and_reps" => Some(RecordMode::BodyweightAndReps),
        "time" => Some(RecordMode::Time),
        _ => None,
    }
}

fn laterality_value(value: Laterality) -> &'static str {
    match value {
        Laterality::Bilateral => "bilateral",
        Laterality::Unilateral => "unilateral",
    }
}

fn parse_laterality(value: &str) -> Option<Laterality> {
    match value {
        "bilateral" => Some(Laterality::Bilateral),
        "unilateral" => Some(Laterality::Unilateral),
        _ => None,
    }
}

fn other_scope_value(value: OtherScope) -> &'static str {
    match value {
        OtherScope::FullBody => "full_body",
        OtherScope::LowerBody => "lower_body",
    }
}

fn parse_other_scope(value: &str) -> Option<OtherScope> {
    match value {
        "full_body" => Some(OtherScope::FullBody),
        "lower_body" => Some(OtherScope::LowerBody),
        _ => None,
    }
}

fn grip_mode_value(value: GripMode) -> &'static str {
    match value {
        GripMode::None => "none",
        GripMode::Grip => "grip",
        GripMode::Attachment => "attachment",
    }
}

fn parse_grip_mode(value: &str) -> Option<GripMode> {
    match value {
        "none" => Some(GripMode::None),
        "grip" => Some(GripMode::Grip),
        "attachment" => Some(GripMode::Attachment),
        _ => None,
    }
}

fn grip_label(style: GripStyle) -> &'static str {
    match style {
        GripStyle::Pronated => text(Key::CalendarGripPronated),
        GripStyle::Supinated => text(Key::CalendarGripSupinated),
        GripStyle::Parallel => text(Key::CalendarGripParallel),
    }
}

fn grip_width_label(width: GripWidth) -> &'static str {
    match width {
        GripWidth::Wide => text(Key::CalendarGripWide),
        GripWidth::Medium => text(Key::CalendarGripMedium),
        GripWidth::Narrow => text(Key::CalendarGripNarrow),
    }
}

fn attachment_label(attachment: Attachment) -> &'static str {
    match attachment {
        Attachment::WideBar => text(Key::CalendarAttachmentWideBar),
        Attachment::StraightBar => text(Key::CalendarAttachmentStraightBar),
        Attachment::ParallelGrip => text(Key::CalendarAttachmentParallelGrip),
        Attachment::MagNarrowParallel => text(Key::CalendarAttachmentMagNarrowParallel),
        Attachment::MagNarrowOver => text(Key::CalendarAttachmentMagNarrowOver),
        Attachment::MagNarrowUnder => text(Key::CalendarAttachmentMagNarrowUnder),
        Attachment::MagMediumParallel => text(Key::CalendarAttachmentMagMediumParallel),
        Attachment::MagMediumOver => text(Key::CalendarAttachmentMagMediumOver),
        Attachment::MagMediumUnder => text(Key::CalendarAttachmentMagMediumUnder),
        Attachment::MagWideOver => text(Key::CalendarAttachmentMagWideOver),
    }
}

fn has_role(muscles: &[MuscleAssignment], unit: MuscleUnit, role: MuscleRole) -> bool {
    muscles
        .iter()
        .any(|assignment| assignment.muscle == unit && assignment.role == role)
}

fn toggle_role(muscles: &mut Vec<MuscleAssignment>, unit: MuscleUnit, role: MuscleRole) {
    let selected = has_role(muscles, unit, role);
    muscles.retain(|assignment| assignment.muscle != unit);
    if !selected {
        muscles.push(MuscleAssignment { muscle: unit, role });
    }
}

fn toggle_grip(options: &mut Vec<GripSelection>, selection: GripSelection) {
    if let Some(index) = options.iter().position(|option| *option == selection) {
        options.remove(index);
    } else {
        options.push(selection);
    }
}
