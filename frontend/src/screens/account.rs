//! マイページのパスキーの一覧と操作。

use dioxus::prelude::*;
use serde::Deserialize;

use crate::passkey;
use crate::text::{text, Key};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct PasskeyItem {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct PasskeyList {
    passkeys: Vec<PasskeyItem>,
}

#[derive(Debug, Deserialize)]
struct ApiResult {
    status: u16,
    body: Option<serde_json::Value>,
}

#[component]
pub fn AccountScreen() -> Element {
    let passkeys = use_signal(Vec::<PasskeyItem>::new);
    let message = use_signal(String::new);
    let needs_reauth = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            refresh(passkeys, message, needs_reauth).await;
        });
    });

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-4 p-5",
            "data-route": "/mypage",
            h1 { class: "text-lg font-bold", "{text(Key::MyPageTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::MyPageDescription)}" }
            div { class: "flex flex-col gap-2 border border-line bg-surface p-3",
                h2 { class: "text-sm font-bold", "{text(Key::PasskeyTitle)}" }
                p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::PasskeyDescription)}" }
                if passkeys().is_empty() {
                    p { class: "text-[13px] text-text-sub", "{text(Key::PasskeyEmpty)}" }
                } else {
                    ul { class: "flex flex-col gap-2",
                        for item in passkeys() {
                            li {
                                class: "flex min-h-11 items-center justify-between gap-2 border border-line px-3",
                                span { class: "truncate text-[13px]", "{item.name}" }
                                button {
                                    class: "min-h-11 border border-line px-3 text-[13px] text-primary",
                                    onclick: {
                                        let id = item.id.clone();
                                        move |_| {
                                            let id = id.clone();
                                            spawn(async move {
                                                remove(id, passkeys, message, needs_reauth).await;
                                            });
                                        }
                                    },
                                    "{text(Key::PasskeyDelete)}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-wrap gap-2",
                    button {
                        class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                        onclick: move |_| {
                            spawn(async move {
                                add(passkeys, message, needs_reauth).await;
                            });
                        },
                        "{text(Key::PasskeyAdd)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| {
                            spawn(async move {
                                reauthenticate(message, needs_reauth).await;
                            });
                        },
                        "{text(Key::PasskeyPasskeyReauth)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| {
                            if let Some(window) = web_sys::window() {
                                let _ = window
                                    .location()
                                    .set_href("/api/auth/reauthenticate/google/start");
                            }
                        },
                        "{text(Key::PasskeyGoogleReauth)}"
                    }
                }
                if needs_reauth() {
                    p { class: "text-[13px] text-primary", "{text(Key::PasskeyNeedsReauth)}" }
                }
                if !message().is_empty() {
                    p { class: "text-[13px] text-text-sub", "{message()}" }
                }
            }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::PasskeyRecovery)}" }
        }
    }
}

async fn refresh(
    mut passkeys: Signal<Vec<PasskeyItem>>,
    mut message: Signal<String>,
    mut needs_reauth: Signal<bool>,
) {
    match passkey::call("list", "").await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                if let Some(body) = result.body {
                    if let Ok(list) = serde_json::from_value::<PasskeyList>(body) {
                        passkeys.set(list.passkeys);
                    }
                }
                needs_reauth.set(false);
            }
            Ok(result) if result.status == 401 => {
                message.set(text(Key::LoginError).to_owned());
            }
            Ok(result) if result.status == 403 => {
                needs_reauth.set(true);
            }
            _ => message.set(text(Key::PasskeyFailed).to_owned()),
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}

async fn add(
    passkeys: Signal<Vec<PasskeyItem>>,
    mut message: Signal<String>,
    mut needs_reauth: Signal<bool>,
) {
    match passkey::call("register", text(Key::PasskeyTitle)).await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                message.set(text(Key::PasskeyRegistered).to_owned());
                refresh(passkeys, message, needs_reauth).await;
            }
            Ok(result) if result.status == 403 => {
                needs_reauth.set(true);
            }
            _ => message.set(text(Key::PasskeyFailed).to_owned()),
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}

async fn remove(
    id: String,
    passkeys: Signal<Vec<PasskeyItem>>,
    mut message: Signal<String>,
    mut needs_reauth: Signal<bool>,
) {
    match passkey::call("remove", &id).await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                message.set(text(Key::PasskeyDeleted).to_owned());
                refresh(passkeys, message, needs_reauth).await;
            }
            Ok(result) if result.status == 403 => {
                needs_reauth.set(true);
            }
            _ => message.set(text(Key::PasskeyFailed).to_owned()),
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}

async fn reauthenticate(mut message: Signal<String>, mut needs_reauth: Signal<bool>) {
    match passkey::call("reauthenticate", "").await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => needs_reauth.set(false),
            _ => {
                needs_reauth.set(true);
                message.set(text(Key::PasskeyFailed).to_owned());
            }
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}
