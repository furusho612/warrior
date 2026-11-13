use dioxus::prelude::*;

use crate::text::{text, Key};

#[component]
pub fn LoginScreen() -> Element {
    rsx! {
        main {
            class: "mx-auto flex min-h-dvh w-full max-w-[390px] flex-col items-center justify-center gap-6 bg-bg px-6 text-text",
            "data-route": "/login",
            h1 { class: "font-condensed text-[32px] font-bold tracking-[0.16em]", "{text(Key::AppName)}" }
            p { class: "text-center text-[13px] leading-5 text-text-sub", "{text(Key::LoginDescription)}" }
            button {
                class: "min-h-11 w-full border border-primary bg-primary px-4 text-[13px] font-bold text-on-primary",
                "{text(Key::LoginButton)}"
            }
        }
    }
}
