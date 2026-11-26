//! ホーム。次のメニュー、開始、今週の実施状況、部位ごとの回復状況。

use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::body_part_analysis::{AnalysisMetric, AnalysisPeriod, BodyPartAnalysis};
use warrior_core::exercise::BodyPart;
use warrior_core::recovery::{RecoveryState, RecoveryStatus};
use warrior_core::routine::Routine;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::analysis as analysis_api;
use crate::calendar_view::part_label;
use crate::program as program_api;
use crate::routes::{navigate, Route};
use crate::routine as routine_api;
use crate::screens::record::start_empty_session;
use crate::screens::OnboardingPanel;
use crate::storage;
use crate::text::{text, Key};

const DIAL_PARTS: [(BodyPart, &str, &str); 7] = [
    (BodyPart::Chest, "50%", "6.5%"),
    (BodyPart::Back, "84%", "22.9%"),
    (BodyPart::Shoulders, "92.4%", "59.7%"),
    (BodyPart::BicepsForearm, "68.9%", "89.2%"),
    (BodyPart::Triceps, "31.1%", "89.2%"),
    (BodyPart::Legs, "7.6%", "59.7%"),
    (BodyPart::Abs, "16%", "22.9%"),
];

/// 端末内の確認が終わらないときに、開始ボタンを有効にするまでの時間。
const STORAGE_CHECK_TIMEOUT_MS: i32 = 10_000;

/// 端末内の確認が終わらない場合に備え、開始ボタンを有効にするタイマーを仕掛ける。
fn schedule_checked_fallback(mut checked: Signal<bool>, done: Rc<Cell<bool>>) -> Option<i32> {
    let window = web_sys::window()?;
    let callback = Closure::once_into_js(move || {
        if !done.get() {
            checked.set(true);
        }
    });
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            STORAGE_CHECK_TIMEOUT_MS,
        )
        .ok()
}

/// 仕掛けたタイマーを取り消す。
fn clear_checked_fallback(timeout: Option<i32>) {
    if let (Some(timeout), Some(window)) = (timeout, web_sys::window()) {
        window.clear_timeout_with_handle(timeout);
    }
}

#[component]
pub fn HomeScreen() -> Element {
    let checked = use_signal(|| false);
    let starting = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let mut show_onboarding = use_signal(|| false);
    let profile_checked = use_signal(|| false);
    let current = use_signal(|| Option::<program_api::CurrentProgram>::None);
    let routines = use_signal(Vec::<Routine>::new);
    let unsaved = use_signal(|| false);
    let check_timeout = use_hook(|| Rc::new(Cell::new(None::<i32>)));
    let drop_timeout = check_timeout.clone();
    use_drop(move || clear_checked_fallback(drop_timeout.take()));
    let weekly = use_signal(|| Option::<BodyPartAnalysis>::None);
    let recovery = use_signal(|| Option::<RecoveryStatus>::None);
    let load_error = use_signal(|| Option::<String>::None);

    use_effect(move || {
        let mut show_onboarding = show_onboarding;
        let mut profile_checked = profile_checked;
        spawn(async move {
            if let Ok(profile) = crate::profile::fetch().await {
                show_onboarding.set(profile.is_none());
                profile_checked.set(true);
            }
        });
    });

    use_effect(move || {
        let mut checked = checked;
        let mut unsaved = unsaved;
        let done = Rc::new(Cell::new(false));
        check_timeout.set(schedule_checked_fallback(checked, done.clone()));
        let done_check = done;
        let timeout_check = check_timeout.clone();
        spawn(async move {
            if let Ok(storage) = storage::current().await {
                if let Ok(Some(session)) = storage.active_session().await {
                    done_check.set(true);
                    clear_checked_fallback(timeout_check.take());
                    navigate(&format!("/record/{}", session.id));
                    return;
                }
                if let Ok(entries) = storage.outbox_entries().await {
                    unsaved.set(!entries.is_empty());
                }
            }
            done_check.set(true);
            clear_checked_fallback(timeout_check.take());
            checked.set(true);
        });
    });

    let load_signals = (current, routines, weekly, recovery, load_error);
    use_effect(move || {
        let (mut current, mut routines, mut weekly, mut recovery, mut error) = load_signals;
        spawn(async move {
            match program_api::list().await {
                Ok(response) => current.set(response.current),
                Err(_) => error.set(Some(text(Key::HomeLoadFailed).to_owned())),
            }
            match routine_api::list().await {
                Ok(list) => routines.set(list.routines),
                Err(_) => error.set(Some(text(Key::HomeLoadFailed).to_owned())),
            }
            match analysis_api::body_parts(AnalysisPeriod::Week, AnalysisMetric::Sets).await {
                Ok(value) => weekly.set(Some(value)),
                Err(_) => error.set(Some(text(Key::HomeLoadFailed).to_owned())),
            }
            match analysis_api::recovery().await {
                Ok(value) => recovery.set(Some(value)),
                Err(_) => error.set(Some(text(Key::HomeLoadFailed).to_owned())),
            }
        });
    });

    let checked_now = checked();
    let busy = starting();
    let close_onboarding = EventHandler::new(move |_: ()| show_onboarding.set(false));
    let program = (current)();
    let program_start = program.as_ref().map(|current| current.routine_id.clone());
    let routine_start = EventHandler::new(move |routine_id: String| {
        spawn(start_program(routine_id, starting, error));
    });
    let empty_start = EventHandler::new(move |_: ()| {
        spawn(start_empty_session(starting, error));
    });

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-2 bg-bg px-5 pb-4 pt-2",
            "data-route": "/",
            "data-profile-checked": if profile_checked() { "true" } else { "false" },
            if show_onboarding() {
                OnboardingPanel { on_done: close_onboarding }
            }
            if let Some(message) = (load_error)() {
                p { class: "text-[12px] text-primary", "data-home-load-error": "true", "{message}" }
            }
            if unsaved() {
                p { class: "text-[12px] text-primary", "data-home-unsaved": "true", "{text(Key::HomeUnsaved)}" }
            }
            if let Some(program) = program {
                NextMenuHeader { current: program.clone() }
                RecoveryDial {
                    recovery: (recovery)(),
                    busy,
                    checked: checked_now,
                    on_start: move |_| {
                        if let Some(routine_id) = program_start.clone() {
                            spawn(start_program(routine_id, starting, error));
                        } else {
                            spawn(start_empty_session(starting, error));
                        }
                    },
                }
                RecoveryLegend {}
                div { class: "flex gap-2",
                    Link {
                        to: Route::Routines {},
                        class: "flex h-11 flex-1 items-center justify-center border border-line text-[13px] text-text",
                        "data-home-choose-program": "true",
                        "{text(Key::HomeChooseProgram)}"
                    }
                    button {
                        r#type: "button",
                        class: "flex h-11 flex-1 items-center justify-center border border-line text-[13px] text-text",
                        disabled: !checked_now || busy,
                        onclick: move |_| empty_start.call(()),
                        "{text(Key::HomeStartEmpty)}"
                    }
                }
                ExerciseMenu { current: program.clone() }
            } else {
                p { class: "text-[13px] text-text-sub", "data-home-no-program": "true", "{text(Key::HomeNoProgram)}" }
                RecoveryDial {
                    recovery: (recovery)(),
                    busy,
                    checked: checked_now,
                    on_start: move |_| empty_start.call(()),
                }
                RecoveryLegend {}
                if !(routines)().is_empty() {
                    div { class: "flex flex-col border-t border-line", "data-home-routines": "true",
                        for routine in routines() {
                            div { class: "flex min-h-[30px] items-center gap-2 border-b border-line",
                                span { class: "flex-1 truncate text-[13px]", "{routine.name}" }
                                button {
                                    r#type: "button",
                                    class: "min-h-11 border border-primary px-2 text-[11px] text-primary",
                                    "data-home-routine-start": "{routine.id}",
                                    disabled: busy,
                                    onclick: {
                                        let id = routine.id.clone();
                                        move |_| routine_start.call(id.clone())
                                    },
                                    "{text(Key::HomeStart)}"
                                }
                            }
                        }
                    }
                } else {
                    Link {
                        to: Route::Routines {},
                        class: "flex h-11 items-center justify-center border border-line text-[13px] text-primary",
                        "data-home-choose-program": "true",
                        "{text(Key::HomeChooseProgram)}"
                    }
                }
                button {
                    r#type: "button",
                    class: "flex h-11 items-center justify-center border border-primary bg-primary text-[13px] font-bold text-on-primary",
                    disabled: !checked_now || busy,
                    onclick: move |_| empty_start.call(()),
                    "{text(Key::HomeStartEmpty)}"
                }
            }
            WeeklyStatus { weekly: (weekly)() }
            if let Some(message) = error() {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| empty_start.call(()),
                    "{text(Key::RecordRetry)}"
                }
            }
        }
    }
}

#[component]
fn NextMenuHeader(current: program_api::CurrentProgram) -> Element {
    let total_sets: u32 = current
        .day
        .exercises
        .iter()
        .map(|exercise| exercise.set_count)
        .sum();
    rsx! {
        div { class: "flex items-baseline justify-between gap-2", "data-home-program": current.program.id,
            span { class: "truncate text-[12px] font-medium tracking-[0.06em] text-text-sub",
                "{text(Key::HomeNextMenu)}｜{current.program.name} {current.day.name}"
                span { class: "hidden", "data-program-position": "true", "{current.day_position} / {current.total_days}" }
            }
            span { class: "shrink-0 text-[12px]", "{current.day.exercises.len()} 種目・{total_sets} セット" }
        }
    }
}

#[component]
fn RecoveryDial(
    recovery: Option<RecoveryStatus>,
    busy: bool,
    checked: bool,
    on_start: EventHandler<()>,
) -> Element {
    let states: Vec<Option<RecoveryState>> = DIAL_PARTS
        .iter()
        .map(|(part, _, _)| {
            recovery
                .as_ref()
                .and_then(|status| status.parts.iter().find(|entry| entry.body_part == *part))
                .and_then(|entry| entry.state)
        })
        .collect();
    rsx! {
        section {
            class: "relative mx-auto h-[280px] w-[280px] shrink-0",
            "data-home-recovery": "true",
            svg {
                class: "absolute left-0 top-0 h-[280px] w-[280px]",
                view_box: "0 0 340 340",
                "aria-hidden": "true",
                circle {
                    cx: "170",
                    cy: "170",
                    r: "112",
                    fill: "none",
                    stroke: "var(--line)",
                    stroke_width: "20",
                }
                for (index, state) in states.iter().copied().enumerate() {
                    {
                        let (color, width) = match state {
                            Some(RecoveryState::Recovering) => ("var(--primary)", 5),
                            Some(RecoveryState::AlmostRecovered) => ("var(--text-sub)", 11),
                            Some(RecoveryState::Recovered) => ("var(--text)", 20),
                            None => ("var(--line)", 20),
                        };
                        let angle = -113.71 + index as f64 * (360.0 / 7.0);
                        rsx! {
                            circle {
                                cx: "170",
                                cy: "170",
                                r: "112",
                                fill: "none",
                                stroke: color,
                                stroke_width: "{width}",
                                stroke_dasharray: "92.7 611.1",
                                transform: "rotate({angle} 170 170)",
                            }
                        }
                    }
                }
            }
            for ((part, left, top), state) in DIAL_PARTS.iter().zip(states.iter().copied()) {
                {
                    let color = match state {
                        Some(RecoveryState::Recovering) => "var(--primary)",
                        _ => "var(--text-sub)",
                    };
                    let weight = if matches!(state, Some(RecoveryState::Recovering)) { "700" } else { "500" };
                    rsx! {
                        span {
                            class: "absolute -translate-x-1/2 -translate-y-1/2 whitespace-nowrap text-[12px]",
                            style: "left: {left}; top: {top}; color: {color}; font-weight: {weight};",
                            "data-home-recovery-part": part.as_str(),
                            "{part_label(*part)}"
                        }
                    }
                }
            }
            button {
                r#type: "button",
                class: "recovery-ring absolute left-1/2 top-1/2 flex h-[140px] w-[140px] -translate-x-1/2 -translate-y-1/2 flex-col items-center justify-center gap-1 bg-primary text-on-primary",
                "data-start": "true",
                "data-home-program-start": "true",
                "data-active-session-checked": if checked { "true" } else { "false" },
                disabled: !checked || busy,
                onclick: move |_| on_start.call(()),
                span { class: "text-[36px] font-black leading-none tracking-[0.08em]", "{text(Key::HomeStartShort)}" }
            }
        }
    }
}

#[component]
fn RecoveryLegend() -> Element {
    rsx! {
        div { class: "flex flex-wrap justify-center gap-x-3 gap-y-1 text-[11px] text-text-sub",
            span { class: "flex items-center gap-1", span { class: "h-[10px] w-[14px] bg-text" } "回復済み" }
            span { class: "flex items-center gap-1", span { class: "h-[6px] w-[14px] bg-text-sub" } "ほぼ回復" }
            span { class: "flex items-center gap-1", span { class: "h-[3px] w-[14px] bg-primary" } "回復中" }
            span { "{text(Key::HomeRecoveryNote)}" }
        }
    }
}

#[component]
fn ExerciseMenu(current: program_api::CurrentProgram) -> Element {
    rsx! {
        ol { class: "flex flex-col border-t border-line", "data-program-day-exercises": "true",
            for (index, exercise) in current.day.exercises.iter().enumerate() {
                li { class: "flex h-[30px] items-center gap-2 border-b border-line",
                    span { class: "w-[18px] font-condensed text-[14px] font-semibold text-text-sub", "{index + 1:02}" }
                    span { class: "min-w-0 flex-1 truncate text-[13px]", "{exercise.name}" }
                    span { class: "font-condensed text-[16px] font-semibold", "{exercise.set_count} セット" }
                }
            }
        }
    }
}

#[component]
fn WeeklyStatus(weekly: Option<BodyPartAnalysis>) -> Element {
    let Some(weekly) = weekly else {
        return rsx! {
            div { class: "border-t border-line pt-2 text-[12px] text-text-sub",
                "data-home-weekly": "unavailable",
                "{text(Key::HomeWeeklyUnavailable)}"
            }
        };
    };
    rsx! {
        div {
            class: "flex flex-col gap-1 border-t border-line pt-2",
            "data-home-weekly": "true",
            "data-home-weekly-days": "{weekly.training_days}",
            div { class: "flex items-baseline gap-2",
                span { class: "text-[12px] font-bold tracking-[0.08em]", "{text(Key::HomeWeekly)}" }
                span { class: "text-[11px] text-text-sub", "{weekly.training_days} 日" }
            }
            div { class: "grid grid-cols-2 gap-x-3 gap-y-0.5 text-[11px] text-text-sub",
                for part in weekly.parts.iter().filter(|part| part.sets > 0) {
                    span { "data-home-weekly-part": part_label(part.body_part),
                        "{part_label(part.body_part)} {part.sets} セット"
                    }
                }
            }
        }
    }
}

async fn start_program(
    routine_id: String,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    busy.set(true);
    error.set(None);
    match routine_api::start_and_store(&routine_id).await {
        Ok(training_id) => navigate(&format!("/record/{training_id}")),
        Err(_) => error.set(Some(text(Key::ProgramStartFailed).to_owned())),
    }
    busy.set(false);
}
