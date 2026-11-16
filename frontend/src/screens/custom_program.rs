//! 独自のプログラムの一覧、作成、編集、削除、選択 (REQ-PRG-021、REQ-PRG-022、REQ-PRG-023、REQ-PRG-024)。

use dioxus::prelude::*;
use warrior_core::routine::Routine;

use crate::custom_program as custom_api;
use crate::routine as routine_api;
use crate::text::{text, Key};

/// 独自のプログラムの管理。
#[component]
pub fn CustomProgramSection() -> Element {
    let programs = use_signal(Vec::<custom_api::CustomProgram>::new);
    let routines = use_signal(Vec::<Routine>::new);
    let mut name = use_signal(String::new);
    let mut selected = use_signal(Vec::<String>::new);
    let mut editing = use_signal(|| Option::<String>::None);
    let error = use_signal(|| Option::<String>::None);
    let busy = use_signal(|| false);
    let reload = use_signal(|| 0u32);
    let reload_effect = reload;
    use_effect(move || {
        let _ = (reload_effect)();
        spawn(load(programs, routines, error));
    });
    let current = (programs)();
    let routine_list = (routines)();
    let selected_value = (selected)();
    let name_value = (name)();
    let editing_value = (editing)();
    let busy_value = (busy)();
    let can_submit =
        (2..=7).contains(&selected_value.len()) && !name_value.trim().is_empty() && !busy_value;

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-custom-programs": "true",
            h2 { class: "text-sm font-bold", "{text(Key::CustomProgramTitle)}" }
            if let Some(message) = (error)() {
                p { class: "text-[12px] text-primary", "{message}" }
            }
            if current.is_empty() {
                p { class: "text-[12px] text-text-sub", "data-custom-programs-empty": "true", "{text(Key::CustomProgramEmpty)}" }
            } else {
                ul { class: "flex flex-col gap-1", "data-custom-program-list": "true",
                    for program in current {
                        {
                            let program_id = program.id.clone();
                            let edit_program = program.clone();
                            let delete_id = program.id.clone();
                            rsx! {
                                li { class: "flex items-center justify-between gap-2 border border-line p-2",
                                    div { class: "flex flex-col",
                                        span { class: "text-[12px] font-bold", "{program.name}" }
                                        span { class: "text-[11px] text-text-sub", "{program.routine_ids.len()} 日" }
                                    }
                                    div { class: "flex gap-1",
                                        button {
                                            r#type: "button",
                                            class: "min-h-11 border border-primary px-2 text-[11px] text-primary",
                                            "data-custom-program-select": "{program.id}",
                                            disabled: busy_value,
                                            onclick: move |_| {
                                                spawn(select(program_id.clone(), busy, reload, error));
                                            },
                                            "{text(Key::CustomProgramSelect)}"
                                        }
                                        button {
                                            r#type: "button",
                                            class: "min-h-11 border border-line px-2 text-[11px]",
                                            "data-custom-program-edit": "{program.id}",
                                            disabled: busy_value,
                                            onclick: move |_| {
                                                editing.set(Some(edit_program.id.clone()));
                                                name.set(edit_program.name.clone());
                                                selected.set(edit_program.routine_ids.clone());
                                            },
                                            "{text(Key::CustomProgramEdit)}"
                                        }
                                        button {
                                            r#type: "button",
                                            class: "min-h-11 border border-line px-2 text-[11px] text-primary",
                                            "data-custom-program-delete": "{program.id}",
                                            disabled: busy_value,
                                            onclick: move |_| {
                                                spawn(delete(delete_id.clone(), busy, reload, error));
                                            },
                                            "{text(Key::CustomProgramDelete)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex flex-col gap-2 border-t border-line pt-2", "data-custom-program-form": "true",
                label { class: "flex min-h-11 items-center gap-2 text-[12px]",
                    span { "{text(Key::CustomProgramName)}" }
                    input {
                        class: "min-h-11 flex-1 border border-line bg-bg px-2 text-[13px]",
                        r#type: "text",
                        "data-custom-program-name": "true",
                        value: "{name()}",
                        oninput: move |event| name.set(event.value()),
                    }
                }
                div { class: "flex flex-col gap-1", "data-custom-program-routines": "true",
                    for routine in routine_list {
                        {
                            let toggle_id = routine.id.clone();
                            let list_for_toggle = selected_value.clone();
                            let checked = selected_value.contains(&routine.id);
                            rsx! {
                                label { class: "flex min-h-11 items-center gap-2 text-[12px]",
                                    input {
                                        r#type: "checkbox",
                                        class: "h-5 w-5",
                                        "data-custom-program-routine": "{routine.id}",
                                        checked,
                                        onchange: move |event| {
                                            let mut list = list_for_toggle.clone();
                                            if event.checked() {
                                                if !list.contains(&toggle_id) {
                                                    list.push(toggle_id.clone());
                                                }
                                            } else {
                                                list.retain(|value| value != &toggle_id);
                                            }
                                            selected.set(list);
                                        },
                                    }
                                    "{routine.name}"
                                }
                            }
                        }
                    }
                }
                button {
                    r#type: "button",
                    class: "min-h-11 border border-primary bg-primary px-3 text-[12px] font-bold text-on-primary",
                    "data-custom-program-submit": "true",
                    disabled: !can_submit,
                    onclick: move |_| {
                        spawn(submit(
                            (name)(),
                            (selected)(),
                            (editing)(),
                            busy,
                            reload,
                            error,
                            editing,
                        ));
                    },
                    if editing_value.is_some() {
                        "{text(Key::ProfileSave)}"
                    } else {
                        "{text(Key::CustomProgramCreate)}"
                    }
                }
            }
        }
    }
}

async fn load(
    mut programs: Signal<Vec<custom_api::CustomProgram>>,
    mut routines: Signal<Vec<Routine>>,
    mut error: Signal<Option<String>>,
) {
    match custom_api::list().await {
        Ok(value) => programs.set(value),
        Err(_) => error.set(Some(text(Key::CustomProgramFailed).to_owned())),
    }
    match routine_api::list().await {
        Ok(list) => routines.set(list.routines),
        Err(_) => error.set(Some(text(Key::CustomProgramFailed).to_owned())),
    }
}

async fn submit(
    name: String,
    routine_ids: Vec<String>,
    editing: Option<String>,
    mut busy: Signal<bool>,
    mut reload: Signal<u32>,
    mut error: Signal<Option<String>>,
    mut editing_signal: Signal<Option<String>>,
) {
    busy.set(true);
    let result = match editing {
        Some(id) => custom_api::update(&id, &name, &routine_ids)
            .await
            .map(|_| ()),
        None => custom_api::create(&name, &routine_ids).await.map(|_| ()),
    };
    match result {
        Ok(()) => {
            error.set(None);
            editing_signal.set(None);
            reload += 1;
        }
        Err(_) => error.set(Some(text(Key::CustomProgramFailed).to_owned())),
    }
    busy.set(false);
}

async fn select(
    id: String,
    mut busy: Signal<bool>,
    mut reload: Signal<u32>,
    mut error: Signal<Option<String>>,
) {
    busy.set(true);
    match custom_api::select(&id).await {
        Ok(()) => {
            error.set(None);
            reload += 1;
        }
        Err(_) => error.set(Some(text(Key::CustomProgramFailed).to_owned())),
    }
    busy.set(false);
}

async fn delete(
    id: String,
    mut busy: Signal<bool>,
    mut reload: Signal<u32>,
    mut error: Signal<Option<String>>,
) {
    busy.set(true);
    match custom_api::delete(&id).await {
        Ok(()) => {
            error.set(None);
            reload += 1;
        }
        Err(_) => error.set(Some(text(Key::CustomProgramFailed).to_owned())),
    }
    busy.set(false);
}
