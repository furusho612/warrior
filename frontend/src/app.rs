//! アプリの起動。

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::pwa::PwaBanner;
#[cfg(target_arch = "wasm32")]
use crate::routes::Route;
#[cfg(target_arch = "wasm32")]
use crate::sync;

/// 画面のルーターを起動する。
#[cfg(target_arch = "wasm32")]
#[component]
pub fn App() -> Element {
    use_effect(move || {
        spawn(async move {
            sync::start().await;
        });
    });

    use_effect(move || {
        let revision = sync::outbox_revision();
        if revision == 0 {
            return;
        }
        spawn(async move {
            sync::flush().await;
        });
    });

    rsx! {
        PwaBanner {}
        dioxus_router::Router::<Route> {}
    }
}
