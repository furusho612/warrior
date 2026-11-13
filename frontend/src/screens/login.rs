use dioxus::prelude::*;

use crate::passkey;
use crate::text::{text, Key};

#[component]
pub fn LoginScreen() -> Element {
    let has_error = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .is_some_and(|path| path.contains("/login/error"));
    let mut email = use_signal(String::new);
    let passkey_failed = use_signal(|| false);

    rsx! {
        main {
            class: "mx-auto flex min-h-dvh w-full max-w-[390px] flex-col items-center justify-center gap-6 bg-bg px-6 text-text",
            "data-route": "/login",
            h1 { class: "font-condensed text-[32px] font-bold tracking-[0.16em]", "{text(Key::AppName)}" }
            p { class: "text-center text-[13px] leading-5 text-text-sub", "{text(Key::LoginDescription)}" }
            if has_error || passkey_failed() {
                p { class: "text-center text-[13px] leading-5 text-primary", "{text(Key::LoginError)}" }
            }
            button {
                class: "min-h-11 w-full border border-primary bg-primary px-4 text-[13px] font-bold text-on-primary",
                onclick: move |_| {
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/api/auth/google/start");
                    }
                },
                "{text(Key::LoginButton)}"
            }
            div { class: "flex w-full flex-col gap-2 border-t border-line pt-5",
                h2 { class: "text-sm font-bold", "{text(Key::PasskeyLoginTitle)}" }
                label { class: "text-[13px] text-text-sub", "{text(Key::PasskeyEmailLabel)}" }
                input {
                    class: "min-h-11 border border-line bg-surface px-3 text-[13px]",
                    r#type: "email",
                    autocomplete: "username",
                    placeholder: text(Key::PasskeyEmailPlaceholder),
                    value: "{email}",
                    oninput: move |event| email.set(event.value())
                }
                button {
                    class: "min-h-11 w-full border border-line px-4 text-[13px]",
                    onclick: move |_| {
                        let value = email();
                        spawn(async move { login_with_passkey(&value, passkey_failed).await; });
                    },
                    "{text(Key::PasskeyLoginButton)}"
                }
                button {
                    class: "min-h-11 w-full border border-line px-4 text-[13px]",
                    onclick: move |_| {
                        spawn(async move { login_with_passkey("", passkey_failed).await; });
                    },
                    "{text(Key::PasskeyDiscoverableButton)}"
                }
            }
        }
    }
}

async fn login_with_passkey(email: &str, mut passkey_failed: Signal<bool>) {
    let call = passkey::call("login", email).await;
    let succeeded = call
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|value| value.get("status").and_then(|status| status.as_u64()))
        == Some(200);
    if succeeded {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href("/");
        }
    } else {
        passkey_failed.set(true);
    }
}
