//! ホーム。トレーニングの開始と、記録中のトレーニングの復元。

use dioxus::prelude::*;
use dioxus_router::navigator;

use crate::routes::{navigate, Route};
use crate::screens::record::create_session;
use crate::screens::ScreenFrame;
use crate::storage;
use crate::text::{text, Key};

#[component]
pub fn HomeScreen() -> Element {
    let mut checked = use_signal(|| false);
    let starting = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);

    use_effect(move || {
        spawn(async move {
            if let Ok(storage) = storage::current().await {
                if let Ok(Some(session)) = storage.active_session().await {
                    navigate(&format!("/record/{}", session.id));
                    return;
                }
            }
            checked.set(true);
        });
    });

    let checked_now = checked();
    let busy = starting();

    rsx! {
        ScreenFrame {
            title: text(Key::HomeTitle).to_owned(),
            description: text(Key::HomeDescription).to_owned(),
            action: rsx! {
                button {
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-start": "true",
                    "data-active-session-checked": if checked_now { "true" } else { "false" },
                    disabled: !checked_now || busy,
                    onclick: move |_| {
                        spawn(start_session(starting, error));
                    },
                    "{text(Key::HomeStart)}"
                }
                if let Some(message) = error() {
                    p { class: "text-[13px] text-primary", "{message}" }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| {
                            spawn(start_session(starting, error));
                        },
                        "{text(Key::RecordRetry)}"
                    }
                }
            },
        }
    }
}

async fn start_session(mut starting: Signal<bool>, mut error: Signal<Option<String>>) {
    starting.set(true);
    let result = create_session().await;
    starting.set(false);
    match result {
        Ok(session) => {
            error.set(None);
            let _ = navigator().push(Route::RecordSession {
                training_id: session.id,
            });
        }
        Err(_) => error.set(Some(text(Key::HomeStartFailed).to_owned())),
    }
}
