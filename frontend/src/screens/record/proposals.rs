//! 記録中の種目の、AI の次回セットの提案 (REQ-AI-006、REQ-AI-007、REQ-AI-008、REQ-AI-018)。
//!
//! 保存済みの提案を読み、重量、回数または時間、セット数、理由を表示する。
//! 採用の操作は、提案のセット数を入力欄へコピーする。採用の後に自由に修正できる。
//! 提案が無い場合は、生成中か失敗かだけを表示し、記録の操作を止めない。

use dioxus::prelude::*;
use warrior_core::ai::output::SetProposal;

use crate::ai as ai_api;
use crate::text::{text, Key};

/// 次回セットの提案。
#[component]
pub(super) fn ProposalPanel(
    exercise_id: String,
    disabled: bool,
    on_apply: EventHandler<SetProposal>,
) -> Element {
    let state = use_signal(|| PanelState::Loading);
    let message = use_signal(|| Option::<String>::None);
    let reload = use_signal(|| 0u32);
    let mut editing = use_signal(|| false);
    let load_id = exercise_id.clone();
    use_effect(move || {
        let _ = (reload)();
        spawn(load(load_id.clone(), state));
    });
    match (state)() {
        PanelState::Hidden => rsx! {},
        PanelState::Loading => rsx! {
            p { class: "text-[11px] text-text-sub", "data-ai-proposal-pending": "true", "{text(Key::RecordProposalPending)}" }
        },
        PanelState::Failed => rsx! {
            p { class: "text-[11px] text-text-sub", "data-ai-proposal-failed": "true", "{text(Key::RecordProposalFailed)}" }
        },
        PanelState::Ready {
            id,
            proposal,
            status,
        } => rsx! {
            div {
                class: "flex flex-col gap-1 border border-line p-2",
                "data-ai-proposal": "true",
                "data-ai-proposal-status": status,
                h4 { class: "text-[12px] font-bold", "{text(Key::RecordProposalTitle)}" }
                ProposalFigures { proposal: proposal.clone() }
                p { class: "text-[11px] leading-5", "data-ai-proposal-reason": "true", "{proposal.reason}" }
                button {
                    r#type: "button",
                    class: "min-h-11 self-start border border-primary px-3 text-[11px] font-bold text-primary",
                    "data-ai-proposal-apply": "true",
                    disabled,
                    onclick: move |_| on_apply.call(proposal.clone()),
                    "{text(Key::RecordProposalApply)}"
                }
                button {
                    r#type: "button",
                    class: "min-h-11 self-start border border-line px-3 text-[11px]",
                    "data-ai-proposal-edit": "true",
                    onclick: move |_| editing.set(true),
                    "{text(Key::RecordProposalEdit)}"
                }
                if editing() {
                    ProposalEditor {
                        id: id.clone(),
                        proposal: proposal.clone(),
                        state,
                        editing,
                    }
                }
                button {
                    r#type: "button",
                    class: "min-h-11 self-start border border-line px-3 text-[11px]",
                    "data-ai-proposal-redo": "true",
                    onclick: move |_| {
                        spawn(request_redo(id.clone(), state, message));
                    },
                    "{text(Key::AiRedo)}"
                }
                if let Some(value) = (message)() {
                    p { class: "text-[11px] text-primary", "data-ai-redo-message": "true", "{value}" }
                }
            }
        },
    }
}

#[component]
fn ProposalEditor(
    id: String,
    proposal: SetProposal,
    mut state: Signal<PanelState>,
    mut editing: Signal<bool>,
) -> Element {
    let mut weight = use_signal(|| {
        proposal
            .weight_kg
            .map(|value| value.to_string())
            .unwrap_or_default()
    });
    let mut reps = use_signal(|| {
        proposal
            .reps
            .map(|value| value.to_string())
            .unwrap_or_default()
    });
    let mut duration = use_signal(|| {
        proposal
            .duration_seconds
            .map(|value| value.to_string())
            .unwrap_or_default()
    });
    let mut sets = use_signal(|| proposal.sets.to_string());
    let mut reason = use_signal(|| proposal.reason.clone());
    let message = use_signal(|| Option::<String>::None);
    let is_time = proposal.duration_seconds.is_some();
    let original = proposal.clone();
    rsx! {
        div { class: "flex flex-col gap-1 border border-line p-2", "data-ai-proposal-editor": "true",
            label { class: "flex min-h-11 items-center gap-2 text-[11px]",
                span { "{text(Key::RecordExerciseWeight)}" }
                input {
                    class: "min-h-11 flex-1 border border-line bg-bg px-2",
                    r#type: "number",
                    step: "0.5",
                    "data-ai-proposal-weight": "true",
                    value: "{weight()}",
                    oninput: move |event| weight.set(event.value()),
                }
            }
            if is_time {
                label { class: "flex min-h-11 items-center gap-2 text-[11px]",
                    span { "{text(Key::RecordExerciseDuration)}" }
                    input {
                        class: "min-h-11 flex-1 border border-line bg-bg px-2",
                        r#type: "number",
                        "data-ai-proposal-duration": "true",
                        value: "{duration()}",
                        oninput: move |event| duration.set(event.value()),
                    }
                }
            } else {
                label { class: "flex min-h-11 items-center gap-2 text-[11px]",
                    span { "{text(Key::RecordExerciseReps)}" }
                    input {
                        class: "min-h-11 flex-1 border border-line bg-bg px-2",
                        r#type: "number",
                        "data-ai-proposal-reps": "true",
                        value: "{reps()}",
                        oninput: move |event| reps.set(event.value()),
                    }
                }
            }
            label { class: "flex min-h-11 items-center gap-2 text-[11px]",
                span { "{text(Key::RecordProposalSets)}" }
                input {
                    class: "min-h-11 flex-1 border border-line bg-bg px-2",
                    r#type: "number",
                    "data-ai-proposal-sets-edit": "true",
                    value: "{sets()}",
                    oninput: move |event| sets.set(event.value()),
                }
            }
            label { class: "flex flex-col gap-1 text-[11px]",
                span { "{text(Key::RecordProposalReason)}" }
                textarea {
                    class: "min-h-16 border border-line bg-bg p-2",
                    "data-ai-proposal-reason-edit": "true",
                    value: "{reason()}",
                    oninput: move |event| reason.set(event.value()),
                }
            }
            div { class: "flex gap-2",
                button {
                    r#type: "button",
                    class: "min-h-11 border border-primary px-3 text-[11px]",
                    "data-ai-proposal-save": "true",
                    onclick: {
                        let id = id.clone();
                        let original = original.clone();
                        move |_| {
                            let mut updated = original.clone();
                            updated.weight_kg = weight().parse().ok();
                            updated.reps = reps().parse().ok();
                            updated.duration_seconds = duration().parse().ok();
                            updated.sets = sets().parse().unwrap_or(original.sets);
                            updated.reason = reason();
                            spawn(save_proposal(id.clone(), updated, state, editing, message));
                        }
                    },
                    "{text(Key::RecordProposalSave)}"
                }
                button {
                    r#type: "button",
                    class: "min-h-11 border border-line px-3 text-[11px]",
                    onclick: move |_| editing.set(false),
                    "{text(Key::RecordProposalCancel)}"
                }
            }
            if let Some(value) = (message)() {
                p { class: "text-[11px] text-primary", "data-ai-proposal-save-message": "true", "{value}" }
            }
        }
    }
}

async fn save_proposal(
    id: String,
    proposal: SetProposal,
    mut state: Signal<PanelState>,
    mut editing: Signal<bool>,
    mut message: Signal<Option<String>>,
) {
    match ai_api::update_proposal(&id, &proposal).await {
        Ok(updated) => {
            state.set(PanelState::Ready {
                id,
                proposal: updated,
                status: "generated",
            });
            editing.set(false);
            message.set(None);
        }
        Err(_) => message.set(Some(text(Key::RecordProposalSaveFailed).to_owned())),
    }
}

#[component]
fn ProposalFigures(proposal: SetProposal) -> Element {
    let weight = proposal
        .weight_kg
        .map(|weight| format!("{weight} kg"))
        .unwrap_or_default();
    let reps = proposal
        .reps
        .map(|reps| format!("{reps} 回"))
        .unwrap_or_default();
    let duration = proposal
        .duration_seconds
        .map(warrior_core::duration::format)
        .unwrap_or_default();
    let amount = if weight.is_empty() {
        duration
    } else {
        format!("{weight} x {reps}")
    };
    rsx! {
        dl {
            class: "grid grid-cols-2 gap-1 text-[11px]",
            "data-ai-proposal-figures": "true",
            div {
                dt { class: "font-bold text-text-sub", "{text(Key::RecordProposalSets)}" }
                dd { "data-ai-proposal-sets": "true", "{proposal.sets}" }
            }
            div {
                dt { class: "font-bold text-text-sub", "{text(Key::RecordExerciseWeight)}" }
                dd { "data-ai-proposal-amount": "true", "{amount}" }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PanelState {
    Hidden,
    Loading,
    Failed,
    Ready {
        id: String,
        proposal: SetProposal,
        status: &'static str,
    },
}

async fn load(exercise_id: String, mut state: Signal<PanelState>) {
    let Ok(entries) = ai_api::proposals(Some(&exercise_id)).await else {
        state.set(PanelState::Failed);
        return;
    };
    let Some(entry) = entries.into_iter().next() else {
        state.set(PanelState::Hidden);
        return;
    };
    if let Some(proposal) = entry.proposal {
        state.set(PanelState::Ready {
            id: entry.id,
            proposal,
            status: "generated",
        });
        return;
    }
    if entry.status == "failed" {
        state.set(PanelState::Failed);
    } else {
        state.set(PanelState::Loading);
    }
}

async fn request_redo(
    job_id: String,
    mut state: Signal<PanelState>,
    mut message: Signal<Option<String>>,
) {
    match ai_api::redo(&job_id).await {
        Ok(result) if result.status == "minimal" => {
            message.set(Some(text(Key::AiRedoMinimal).to_owned()));
        }
        Ok(_) => {
            message.set(None);
            state.set(PanelState::Loading);
            for _ in 0..60 {
                crate::screens::record::sleep_ms(1_000).await;
                let Ok(entries) = ai_api::proposals(None).await else {
                    continue;
                };
                if let Some(entry) = entries.into_iter().find(|entry| entry.id == job_id) {
                    if let Some(proposal) = entry.proposal {
                        state.set(PanelState::Ready {
                            id: entry.id,
                            proposal,
                            status: "generated",
                        });
                        return;
                    }
                    if entry.status == "failed" {
                        state.set(PanelState::Failed);
                        return;
                    }
                }
            }
        }
        Err(_) => message.set(Some(text(Key::AiRedoFailed).to_owned())),
    }
}
