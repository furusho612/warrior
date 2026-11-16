//! AI の評価の一覧と、日の評価の表示 (REQ-AI-001、REQ-AI-004、REQ-AI-005、REQ-AI-013)。

use dioxus::prelude::*;
use warrior_core::ai::output::{Evaluation, EvaluationItem};

use crate::ai as ai_api;
use crate::ai_notice::AiPrivacyNotice;
use crate::text::{text, Key};

/// 評価の一覧の画面。
#[component]
pub fn EvaluationsScreen() -> Element {
    let entries = use_signal(Vec::<ai_api::EvaluationEntry>::new);
    let error = use_signal(|| Option::<String>::None);
    use_effect(move || {
        spawn(load(entries, error));
    });
    let list = (entries)();
    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-route": "/evaluations",
            h1 { class: "text-lg font-bold", "{text(Key::EvaluationsTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::EvaluationsDescription)}" }
            AiPrivacyNotice {}
            if let Some(message) = (error)() {
                p { class: "text-[13px] text-primary", "{message}" }
            } else if list.is_empty() {
                p { class: "text-[13px] text-text-sub", "data-evaluations-empty": "true", "{text(Key::EvaluationsEmpty)}" }
            } else {
                div { class: "flex flex-col gap-3", "data-evaluation-list": "true",
                    for entry in list {
                        EvaluationCard { entry }
                    }
                }
            }
        }
    }
}

/// 評価の 1 件。日の評価の画面でも使う。
#[component]
pub fn EvaluationCard(entry: ai_api::EvaluationEntry) -> Element {
    let period = entry
        .period_start
        .as_deref()
        .map(str::to_owned)
        .or_else(|| entry.training_id.as_ref().map(|_| "当日".to_owned()))
        .unwrap_or_default();
    let message = use_signal(|| Option::<String>::None);
    rsx! {
        article {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-ai-evaluation": "true",
            "data-ai-evaluation-kind": entry.kind,
            "data-ai-evaluation-status": entry.status,
            header { class: "flex items-center justify-between gap-2",
                h2 { class: "text-sm font-bold", "{kind_label(&entry.kind)}" }
                span { class: "text-[11px] text-text-sub", "{period}" }
            }
            EvaluationBody { entry: entry.clone() }
            if entry.status == "generated" {
                button {
                    r#type: "button",
                    class: "min-h-11 self-start border border-line px-3 text-[11px]",
                    "data-ai-redo": "true",
                    onclick: move |_| {
                        spawn(request_redo(entry.id.clone(), message));
                    },
                    "{text(Key::AiRedo)}"
                }
            }
            if let Some(value) = (message)() {
                p { class: "text-[11px] text-primary", "data-ai-redo-message": "true", "{value}" }
            }
        }
    }
}

async fn request_redo(job_id: String, mut message: Signal<Option<String>>) {
    match ai_api::redo(&job_id).await {
        Ok(result) if result.status == "minimal" => {
            message.set(Some(text(Key::AiRedoMinimal).to_owned()))
        }
        Ok(_) => message.set(None),
        Err(_) => message.set(Some(text(Key::AiRedoFailed).to_owned())),
    }
}

#[component]
fn EvaluationBody(entry: ai_api::EvaluationEntry) -> Element {
    if entry.status == "generated" {
        if let Some(evaluation) = entry.evaluation {
            return rsx! { EvaluationSections { evaluation } };
        }
        return rsx! {
            p { class: "text-[12px] text-text-sub", "data-ai-evaluation-unavailable": "true", "{text(Key::EvaluationUnavailable)}" }
        };
    }
    if entry.status == "failed" {
        return rsx! {
            p { class: "text-[12px] text-primary", "data-ai-evaluation-failed": "true", "{text(Key::EvaluationFailed)}" }
        };
    }
    rsx! {
        p { class: "text-[12px] text-text-sub", "data-ai-evaluation-pending": "true", "{text(Key::EvaluationPending)}" }
    }
}

#[component]
fn EvaluationSections(evaluation: Evaluation) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2", "data-ai-evaluation-sections": "true",
            EvaluationSection { label: text(Key::EvaluationGood).to_owned(), items: evaluation.good, field: "good" }
            EvaluationSection { label: text(Key::EvaluationImprovements).to_owned(), items: evaluation.improvements, field: "improvements" }
            EvaluationSection { label: text(Key::EvaluationNextActions).to_owned(), items: evaluation.next_actions, field: "next-actions" }
        }
    }
}

#[component]
fn EvaluationSection(label: String, items: Vec<EvaluationItem>, field: &'static str) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { "data-ai-evaluation-section": field,
            h3 { class: "text-[12px] font-bold", "{label}" }
            ul { class: "flex list-disc flex-col gap-1 pl-4 text-[12px] leading-5",
                for item in items {
                    li {
                        span { "{item.text}" }
                        span { class: "text-text-sub", " ({item.evidence})" }
                    }
                }
            }
        }
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "daily_evaluation" => text(Key::EvaluationDaily),
        "weekly_evaluation" => text(Key::EvaluationWeekly),
        "monthly_evaluation" => text(Key::EvaluationMonthly),
        _ => text(Key::EvaluationsTitle),
    }
}

async fn load(
    mut entries: Signal<Vec<ai_api::EvaluationEntry>>,
    mut error: Signal<Option<String>>,
) {
    match ai_api::evaluations(None).await {
        Ok(list) => {
            error.set(None);
            entries.set(list);
        }
        Err(_) => error.set(Some(text(Key::EvaluationFailed).to_owned())),
    }
}

/// 日の評価を、生成が終わるまで条件を待って表示する。
#[component]
pub fn DailyEvaluationPanel(date: String) -> Element {
    let entry = use_signal(|| Option::<ai_api::EvaluationEntry>::None);
    let error = use_signal(|| Option::<String>::None);
    let load_date = date.clone();
    use_effect(move || {
        spawn(poll(load_date.clone(), entry, error));
    });
    rsx! {
        div { class: "flex flex-col gap-2", "data-daily-evaluation": "true",
            if let Some(message) = (error)() {
                p { class: "text-[12px] text-primary", "{message}" }
            } else if let Some(entry) = (entry)() {
                EvaluationCard { entry }
            } else {
                p { class: "text-[12px] text-text-sub", "data-ai-evaluation-pending": "true", "{text(Key::EvaluationPending)}" }
            }
        }
    }
}

async fn poll(
    date: String,
    mut entry: Signal<Option<ai_api::EvaluationEntry>>,
    mut error: Signal<Option<String>>,
) {
    for _ in 0..60 {
        match ai_api::evaluations(Some(&date)).await {
            Ok(list) => {
                if let Some(found) = list.into_iter().next() {
                    let terminal = found.status == "generated" || found.status == "failed";
                    entry.set(Some(found));
                    if terminal {
                        error.set(None);
                        return;
                    }
                }
            }
            Err(_) => {
                error.set(Some(text(Key::EvaluationFailed).to_owned()));
                return;
            }
        }
        crate::screens::record::sleep_ms(1_000).await;
    }
    error.set(Some(text(Key::EvaluationFailed).to_owned()));
}
