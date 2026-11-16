//! プログラムの推奨、選択、進行、開始 (REQ-PRG-002、REQ-PRG-005、REQ-PRG-019)。

use dioxus::prelude::*;

use crate::program as program_api;
use crate::routes::navigate;
use crate::routine as routine_api;
use crate::text::{text, Key};

/// プログラムの推奨と、進行中のプログラムの次の日。
#[component]
pub fn ProgramSection() -> Element {
    let programs = use_signal(|| Option::<program_api::ProgramsResponse>::None);
    let error = use_signal(|| Option::<String>::None);
    let busy = use_signal(|| false);
    let message = use_signal(|| Option::<String>::None);
    let reload = use_signal(|| 0u32);
    let reload_effect = reload;
    use_effect(move || {
        let _ = (reload_effect)();
        spawn(load(programs, error));
    });
    let response = (programs)();
    let busy_value = busy();
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-programs": "true",
            h2 { class: "text-sm font-bold", "{text(Key::ProgramSectionTitle)}" }
            if let Some(message) = (message)() {
                p { class: "text-[12px] text-primary", "data-program-message": "true", "{message}" }
            }
            if let Some(message) = (error)() {
                p { class: "text-[12px] text-primary", "{message}" }
            } else if let Some(response) = response {
                if let Some(current) = response.current {
                    CurrentProgramView {
                        current,
                        busy: busy_value,
                        on_start: move |routine_id: String| {
                            spawn(start(routine_id, busy, error, message));
                        },
                        on_stop: move |_| {
                            spawn(stop(busy, reload, error));
                        },
                    }
                } else if response.recommended.is_empty() {
                    p { class: "text-[12px] text-text-sub", "data-programs-empty": "true", "{text(Key::ProgramEmpty)}" }
                }
                if !response.recommended.is_empty() {
                    h3 { class: "text-[12px] font-bold", "{text(Key::ProgramRecommended)}" }
                    ProgramChoices {
                        programs: response.recommended,
                        busy: busy_value,
                        on_select: move |id: String| {
                            spawn(select(id, busy, reload, error, message));
                        },
                    }
                }
                if !response.references.is_empty() {
                    h3 { class: "text-[12px] font-bold", "{text(Key::ProgramReferences)}" }
                    ProgramChoices {
                        programs: response.references,
                        busy: busy_value,
                        on_select: move |id: String| {
                            spawn(select(id, busy, reload, error, message));
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn CurrentProgramView(
    current: program_api::CurrentProgram,
    busy: bool,
    on_start: EventHandler<String>,
    on_stop: EventHandler<()>,
) -> Element {
    let routine_id = current.routine_id.clone();
    let total = current.total_days.max(1);
    rsx! {
        div {
            class: "flex flex-col gap-2",
            "data-current-program": current.program.id,
            p { class: "text-[13px] font-bold", "{current.program.name}" }
            p { class: "text-[11px] text-text-sub", "data-program-position": "true", "{text(Key::ProgramNextDay)} {current.day_position} / {total}" }
            h3 { class: "text-[12px] font-bold", "{current.day.name}" }
            ul { class: "flex flex-col gap-1 text-[11px]", "data-program-day-exercises": "true",
                for exercise in current.day.exercises {
                    li {
                        span { "{exercise.name}" }
                        span { class: "text-text-sub", " {exercise.set_count} セット" }
                    }
                }
            }
            button {
                r#type: "button",
                class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                "data-program-start": "true",
                disabled: busy,
                onclick: move |_| on_start.call(routine_id.clone()),
                "{text(Key::ProgramStart)}"
            }
            button {
                r#type: "button",
                class: "min-h-11 border border-line px-3 text-[11px]",
                "data-program-stop": "true",
                disabled: busy,
                onclick: move |_| on_stop.call(()),
                "{text(Key::ProgramStop)}"
            }
        }
    }
}

#[component]
fn ProgramChoices(
    programs: Vec<program_api::ProgramSummary>,
    busy: bool,
    on_select: EventHandler<String>,
) -> Element {
    rsx! {
        ul { class: "flex flex-col gap-1", "data-program-choices": "true",
            for program in programs {
                li { class: "flex items-center justify-between gap-2 border border-line p-2",
                    div { class: "flex flex-col",
                        span { class: "text-[12px] font-bold", "{program.name}" }
                        span { class: "text-[11px] text-text-sub", "{program.days_per_week} 日 / 週" }
                    }
                    button {
                        r#type: "button",
                        class: "min-h-11 border border-primary px-2 text-[11px] text-primary",
                        "data-program-select": "{program.id}",
                        disabled: busy,
                        onclick: move |_| on_select.call(program.id.clone()),
                        "{text(Key::ProgramSelect)}"
                    }
                }
            }
        }
    }
}

async fn load(
    mut programs: Signal<Option<program_api::ProgramsResponse>>,
    mut error: Signal<Option<String>>,
) {
    match program_api::list().await {
        Ok(response) => {
            error.set(None);
            programs.set(Some(response));
        }
        Err(_) => error.set(Some(text(Key::ProgramLoadFailed).to_owned())),
    }
}

async fn select(
    program_id: String,
    mut busy: Signal<bool>,
    mut reload: Signal<u32>,
    mut error: Signal<Option<String>>,
    mut message: Signal<Option<String>>,
) {
    busy.set(true);
    match program_api::select(&program_id).await {
        Ok(_) => {
            error.set(None);
            message.set(Some(text(Key::ProgramChanged).to_owned()));
            reload += 1;
        }
        Err(_) => error.set(Some(text(Key::ProgramLoadFailed).to_owned())),
    }
    busy.set(false);
}

async fn stop(mut busy: Signal<bool>, mut reload: Signal<u32>, mut error: Signal<Option<String>>) {
    busy.set(true);
    match program_api::stop().await {
        Ok(()) => {
            error.set(None);
            reload += 1;
        }
        Err(_) => error.set(Some(text(Key::ProgramLoadFailed).to_owned())),
    }
    busy.set(false);
}

async fn start(
    routine_id: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut message: Signal<Option<String>>,
) {
    busy.set(true);
    match routine_api::start_and_store(&routine_id).await {
        Ok(training_id) => {
            message.set(None);
            navigate(&format!("/record/{training_id}"));
        }
        Err(_) => error.set(Some(text(Key::ProgramStartFailed).to_owned())),
    }
    busy.set(false);
}
