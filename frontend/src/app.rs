//! アプリの起動。

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::pwa::PwaBanner;
#[cfg(target_arch = "wasm32")]
use crate::routes::Route;

/// 画面のルーターを起動する。
#[cfg(target_arch = "wasm32")]
#[component]
pub fn App() -> Element {
    rsx! {
        PwaBanner {}
        dioxus_router::Router::<Route> {}
    }
}
