//! 記録画面のルーティンの開始の入口。
//!
//! 端末に保存したルーティンを並べ、選んだルーティンからトレーニングを開始する。
//! 開始は送信待ちの列を送ってからサーバーにトレーニングを作らせ、記録画面へ移る。

use dioxus::prelude::*;
use dioxus_router::{navigator, Link};
use warrior_core::routine::Routine;

use crate::routes::Route;
use crate::routine as routine_api;
use crate::storage;
use crate::text::{text, Key};

/// 記録画面のルーティンの一覧と開始のボタン。
#[component]
pub(super) fn RoutineStart() -> Element {
    let routines = use_signal(Vec::<Routine>::new);
    let starting = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);

    use_effect(move || {
        spawn(load(routines));
    });

    let list = (routines)();
    let busy = (starting)();
    let message = (error)();
    rsx! {
        section {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-routine-start-list": "true",
            div { class: "flex items-center justify-between gap-2",
                h2 { class: "text-sm font-bold", "{text(Key::RecordStartRoutine)}" }
                Link {
                    to: Route::Routines {},
                    class: "flex min-h-11 items-center text-[13px] text-primary",
                    "data-routines-link": "true",
                    "{text(Key::RecordRoutinesLink)}"
                }
            }
            if let Some(message) = message {
                p { class: "text-[13px] text-primary", "data-routine-start-error": "true", "{message}" }
            }
            if list.is_empty() {
                p { class: "text-[13px] text-text-sub", "{text(Key::RoutinesEmpty)}" }
            }
            for routine in list {
                button {
                    class: "flex min-h-11 items-center justify-between gap-2 border border-line px-3 text-left text-[13px]",
                    "data-start-routine": "true",
                    "data-routine-id": "{routine.id}",
                    disabled: busy,
                    onclick: {
                        let routine_id = routine.id.clone();
                        move |_| {
                            spawn(start(routine_id.clone(), starting, error));
                        }
                    },
                    span { class: "font-bold", "{routine.name}" }
                    span { class: "text-[11px] text-text-sub", "{routine.exercises.len()}" }
                }
            }
        }
    }
}

async fn load(mut routines: Signal<Vec<Routine>>) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let local = storage.routines().await.unwrap_or_default();
    if !local.is_empty() {
        routines.set(local);
        return;
    }
    if let Ok(list) = routine_api::list().await {
        let _ = storage.replace_routines(&list.routines).await;
        routines.set(storage.routines().await.unwrap_or_default());
    }
}

async fn start(routine_id: String, mut starting: Signal<bool>, mut error: Signal<Option<String>>) {
    if starting() {
        return;
    }
    starting.set(true);
    let result = routine_api::start_and_store(&routine_id).await;
    starting.set(false);
    match result {
        Ok(training_id) => {
            error.set(None);
            let _ = navigator().push(Route::RecordSession { training_id });
        }
        Err(_) => error.set(Some(text(Key::RoutinesStartFailed).to_owned())),
    }
}
