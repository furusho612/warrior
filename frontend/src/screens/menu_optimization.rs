//! メニューの最適化の変更案と、変更ごとの承認 (REQ-AI-010、REQ-AI-011、REQ-AI-012)。

use dioxus::prelude::*;
use warrior_core::ai::output::{MenuChange, MenuChangeKind};

use crate::ai as ai_api;
use crate::text::{text, Key};

/// メニューの変更案の一覧。
#[component]
pub fn MenuOptimizationPanel() -> Element {
    let view = use_signal(|| Option::<ai_api::MenuView>::None);
    let error = use_signal(|| Option::<String>::None);
    let status = use_signal(|| Option::<String>::None);
    let reload = use_signal(|| 0u32);
    let reload_effect = reload;
    use_effect(move || {
        let _ = (reload_effect)();
        spawn(load(view, error));
    });
    let changes = (view)().map(|view| view.changes).unwrap_or_default();
    let status_text = (status)();
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-menu-optimization": "true",
            h2 { class: "text-sm font-bold", "{text(Key::MenuOptimizationTitle)}" }
            if let Some(message) = (error)() {
                p { class: "text-[12px] text-primary", "{message}" }
            }
            if let Some(message) = status_text {
                p { class: "text-[12px] text-primary", "data-menu-optimization-status": "true", "{message}" }
            }
            if changes.is_empty() {
                p { class: "text-[12px] text-text-sub", "data-menu-optimization-empty": "true", "{text(Key::MenuOptimizationEmpty)}" }
            } else {
                ul { class: "flex flex-col gap-2", "data-menu-optimization-changes": "true",
                    for (index, change) in changes.iter().cloned().enumerate() {
                        MenuChangeRow {
                            index,
                            change,
                            on_approve: move |change: MenuChange| {
                                spawn(approve(change, view, status, reload));
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MenuChangeRow(
    index: usize,
    change: MenuChange,
    on_approve: EventHandler<MenuChange>,
) -> Element {
    let amount = change
        .sets
        .map(|sets| format!("{sets} セット"))
        .unwrap_or_default();
    rsx! {
        li {
            class: "flex flex-col gap-1 border border-line p-2",
            "data-menu-change": "{index}",
            "data-menu-change-kind": kind_name(change.kind),
            p { class: "text-[12px] font-bold", "{kind_label(change.kind)}" }
            p { class: "text-[11px] text-text-sub", "{change.exercise_id} {amount}" }
            p { class: "text-[11px] leading-5", "{change.reason}" }
            button {
                r#type: "button",
                class: "min-h-11 self-start border border-primary px-3 text-[11px] font-bold text-primary",
                "data-menu-change-approve": "{index}",
                onclick: move |_| on_approve.call(change.clone()),
                "{text(Key::MenuOptimizationApprove)}"
            }
        }
    }
}

async fn load(mut view: Signal<Option<ai_api::MenuView>>, mut error: Signal<Option<String>>) {
    match ai_api::menu().await {
        Ok(value) => {
            error.set(None);
            view.set(Some(value));
        }
        Err(_) => error.set(Some(text(Key::EvaluationFailed).to_owned())),
    }
}

async fn approve(
    change: MenuChange,
    mut view: Signal<Option<ai_api::MenuView>>,
    mut status: Signal<Option<String>>,
    mut reload: Signal<u32>,
) {
    match ai_api::apply_menu(std::slice::from_ref(&change)).await {
        Ok(_) => {
            status.set(Some(text(Key::MenuOptimizationApplied).to_owned()));
            if let Some(current) = view.write().as_mut() {
                current.changes.retain(|item| {
                    item.exercise_id != change.exercise_id || item.kind != change.kind
                });
            }
            reload += 1;
        }
        Err(_) => status.set(Some(text(Key::MenuOptimizationFailed).to_owned())),
    }
}

fn kind_label(kind: MenuChangeKind) -> &'static str {
    match kind {
        MenuChangeKind::AddExercise => text(Key::MenuOptimizationAdd),
        MenuChangeKind::ReplaceExercise => text(Key::MenuOptimizationReplace),
        MenuChangeKind::SetIncrease => text(Key::MenuOptimizationIncrease),
        MenuChangeKind::SetDecrease => text(Key::MenuOptimizationDecrease),
        MenuChangeKind::PartBalance => text(Key::MenuOptimizationBalance),
    }
}

fn kind_name(kind: MenuChangeKind) -> &'static str {
    match kind {
        MenuChangeKind::AddExercise => "add_exercise",
        MenuChangeKind::ReplaceExercise => "replace_exercise",
        MenuChangeKind::SetIncrease => "set_increase",
        MenuChangeKind::SetDecrease => "set_decrease",
        MenuChangeKind::PartBalance => "part_balance",
    }
}
