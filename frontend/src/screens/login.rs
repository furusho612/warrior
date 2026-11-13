use dioxus::prelude::*;

use crate::text::{text, Key};

#[component]
pub fn LoginScreen() -> Element {
    let has_error = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .is_some_and(|path| path.contains("/login/error"));
    rsx! {
        main {
            class: "mx-auto flex min-h-dvh w-full max-w-[390px] flex-col items-center justify-center gap-6 bg-bg px-6 text-text",
            "data-route": "/login",
            h1 { class: "font-condensed text-[32px] font-bold tracking-[0.16em]", "{text(Key::AppName)}" }
            p { class: "text-center text-[13px] leading-5 text-text-sub", "{text(Key::LoginDescription)}" }
            if has_error {
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
        }
    }
}
