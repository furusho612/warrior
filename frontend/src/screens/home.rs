//! ホーム。次のメニュー、今週の実施状況、部位ごとの回復状況、トレーニングの開始。

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::body_part_analysis::{AnalysisMetric, AnalysisPeriod, BodyPartAnalysis};
use warrior_core::recovery::RecoveryStatus;
use warrior_core::routine::Routine;

use crate::analysis as analysis_api;
use crate::calendar_view::part_label;
use crate::program as program_api;
use crate::routes::{navigate, Route};
use crate::routine as routine_api;
use crate::screens::record::start_empty_session;
use crate::screens::OnboardingPanel;
use crate::storage;
use crate::text::{text, Key};

#[component]
pub fn HomeScreen() -> Element {
    let mut checked = use_signal(|| false);
    let starting = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let mut show_onboarding = use_signal(|| false);
    let profile_checked = use_signal(|| false);
    let current = use_signal(|| Option::<program_api::CurrentProgram>::None);
    let routines = use_signal(Vec::<Routine>::new);
    let unsaved = use_signal(|| false);
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
        let mut unsaved = unsaved;
        spawn(async move {
            if let Ok(storage) = storage::current().await {
                if let Ok(Some(session)) = storage.active_session().await {
                    navigate(&format!("/record/{}", session.id));
                    return;
                }
                if let Ok(entries) = storage.outbox_entries().await {
                    unsaved.set(!entries.is_empty());
                }
            }
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
    let start = EventHandler::new(move |routine_id: String| {
        spawn(start_program(routine_id, starting, error));
    });

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-4 p-5",
            "data-route": "/",
            "data-profile-checked": if profile_checked() { "true" } else { "false" },
            h1 { class: "text-lg font-bold", "{text(Key::HomeTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::HomeDescription)}" }
            if show_onboarding() {
                OnboardingPanel { on_done: close_onboarding }
            }
            if let Some(message) = (load_error)() {
                p { class: "text-[12px] text-primary", "data-home-load-error": "true", "{message}" }
            }
            if unsaved() {
                p { class: "text-[12px] text-primary", "data-home-unsaved": "true", "{text(Key::HomeUnsaved)}" }
            }
            div {
                class: "flex flex-col gap-2 border border-line bg-surface p-3",
                "data-home-next-menu": "true",
                h2 { class: "text-sm font-bold", "{text(Key::HomeNextMenu)}" }
                if let Some(program) = program {
                    ProgramNext { current: program, on_start: start, busy }
                } else {
                    p { class: "text-[12px] text-text-sub", "data-home-no-program": "true", "{text(Key::HomeNoProgram)}" }
                    if routines().is_empty() {
                        Link {
                            to: Route::Routines {},
                            class: "flex min-h-11 items-center justify-center border border-line px-3 text-[12px] text-primary",
                            "data-home-choose-program": "true",
                            "{text(Key::HomeChooseProgram)}"
                        }
                    } else {
                        ul { class: "flex flex-col gap-1", "data-home-routines": "true",
                            for routine in routines() {
                                li { class: "flex items-center justify-between gap-2 border border-line p-2",
                                    span { class: "text-[12px]", "{routine.name}" }
                                    button {
                                        r#type: "button",
                                        class: "min-h-11 border border-primary px-2 text-[11px] text-primary",
                                        "data-home-routine-start": "{routine.id}",
                                        disabled: busy,
                                        onclick: {
                                            let id = routine.id.clone();
                                            move |_| start.call(id.clone())
                                        },
                                        "{text(Key::HomeStart)}"
                                    }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-start": "true",
                    "data-active-session-checked": if checked_now { "true" } else { "false" },
                    disabled: !checked_now || busy,
                    onclick: move |_| {
                        spawn(start_empty_session(starting, error));
                    },
                    "{text(Key::HomeStart)}"
                }
                if let Some(message) = error() {
                    p { class: "text-[13px] text-primary", "{message}" }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| {
                            spawn(start_empty_session(starting, error));
                        },
                        "{text(Key::RecordRetry)}"
                    }
                }
            }
            WeeklyStatus { weekly: (weekly)() }
            RecoveryRings { recovery: (recovery)() }
        }
    }
}

#[component]
fn ProgramNext(
    current: program_api::CurrentProgram,
    on_start: EventHandler<String>,
    busy: bool,
) -> Element {
    let routine_id = current.routine_id.clone();
    rsx! {
        div { class: "flex flex-col gap-1", "data-home-program": current.program.id,
            p { class: "text-[12px] font-bold", "{current.program.name}" }
            p { class: "text-[11px] text-text-sub", "{current.day.name} ({current.day_position} / {current.total_days})" }
            ul { class: "flex flex-col gap-0.5 text-[11px]",
                for exercise in current.day.exercises {
                    li { "{exercise.name} {exercise.set_count} セット" }
                }
            }
            button {
                r#type: "button",
                class: "min-h-11 border border-primary px-3 text-[12px] font-bold text-primary",
                "data-home-program-start": "true",
                disabled: busy,
                onclick: move |_| on_start.call(routine_id.clone()),
                "{text(Key::ProgramStart)}"
            }
        }
    }
}

#[component]
fn WeeklyStatus(weekly: Option<BodyPartAnalysis>) -> Element {
    let Some(weekly) = weekly else {
        return rsx! {
            div { class: "border border-line bg-surface p-3 text-[12px] text-text-sub",
                "data-home-weekly": "unavailable",
                "{text(Key::HomeWeeklyUnavailable)}"
            }
        };
    };
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-line bg-surface p-3",
            "data-home-weekly": "true",
            "data-home-weekly-days": "{weekly.training_days}",
            h2 { class: "text-sm font-bold", "{text(Key::HomeWeekly)}" }
            p { class: "text-[12px]", "{text(Key::HomeWeeklyDays)}: {weekly.training_days} 日" }
            ul { class: "grid grid-cols-2 gap-1 text-[11px]",
                for part in weekly.parts.iter().filter(|part| part.sets > 0) {
                    li { "data-home-weekly-part": part_label(part.body_part),
                        "{part_label(part.body_part)} {part.sets} セット"
                    }
                }
            }
        }
    }
}

#[component]
fn RecoveryRings(recovery: Option<RecoveryStatus>) -> Element {
    let Some(recovery) = recovery else {
        return rsx! {
            div { class: "flex flex-col gap-1 border border-line bg-surface p-3",
                "data-home-recovery": "unavailable",
                h2 { class: "text-sm font-bold", "{text(Key::HomeRecovery)}" }
                div { class: "flex flex-wrap gap-2",
                    for _ in 0..6 {
                        div { class: "flex h-16 w-16 items-center justify-center recovery-ring border-4 border-line text-[10px] text-text-sub",
                            "data-home-recovery-placeholder": "true",
                            "-"
                        }
                    }
                }
                p { class: "text-[10px] text-text-sub", "{text(Key::HomeRecoveryUnavailable)}" }
            }
        };
    };
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-line bg-surface p-3",
            "data-home-recovery": "true",
            h2 { class: "text-sm font-bold", "{text(Key::HomeRecovery)}" }
            div { class: "flex flex-wrap gap-2",
                for entry in recovery.parts {
                    {
                        let state = analysis_api::recovery_state_value(entry.state);
                        let class = match entry.state {
                            Some(warrior_core::recovery::RecoveryState::Recovering) => {
                                "flex h-16 w-16 flex-col items-center justify-center recovery-ring border-4 border-primary text-[10px]"
                            }
                            Some(warrior_core::recovery::RecoveryState::AlmostRecovered) => {
                                "flex h-16 w-16 flex-col items-center justify-center recovery-ring border-4 border-text-sub text-[10px]"
                            }
                            _ => {
                                "flex h-16 w-16 flex-col items-center justify-center recovery-ring border-4 border-line text-[10px]"
                            }
                        };
                        rsx! {
                            div {
                                class,
                                "data-home-recovery-part": part_label(entry.body_part),
                                "data-home-recovery-state": state,
                                span { "{part_label(entry.body_part)}" }
                                span { class: "text-text-sub", "{analysis_api::recovery_label(entry.state.unwrap_or(warrior_core::recovery::RecoveryState::Recovered))}" }
                            }
                        }
                    }
                }
            }
            p { class: "text-[10px] text-text-sub", "{text(Key::HomeRecoveryNote)}" }
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
