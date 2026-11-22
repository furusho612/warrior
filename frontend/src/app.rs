//! アプリの起動。

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::pwa::PwaBanner;
#[cfg(target_arch = "wasm32")]
use crate::record_rail::RecordRailState;
#[cfg(target_arch = "wasm32")]
use crate::rest_timer::RestTimerState;
#[cfg(target_arch = "wasm32")]
use crate::routes::Route;
#[cfg(target_arch = "wasm32")]
use crate::settings;
#[cfg(target_arch = "wasm32")]
use crate::sync;
#[cfg(target_arch = "wasm32")]
use crate::theme::{self, ThemePreference};

/// 画面のルーターを起動する。
#[cfg(target_arch = "wasm32")]
#[component]
pub fn App() -> Element {
    use_context_provider(|| Signal::new(RecordRailState::default()));
    let rest_timer = RestTimerState {
        timer: use_signal(|| None),
        wake_lock_enabled: use_signal(|| true),
        notification_enabled: use_signal(|| false),
    };
    use_context_provider(|| rest_timer);
    let theme_mode = use_signal(theme::cached_display_mode);
    let theme_preference = use_context_provider(|| ThemePreference { mode: theme_mode });
    use_hook(theme::watch_system_preference);
    let mut theme_mode = theme_preference.mode;
    use_effect(move || {
        spawn(async move {
            settings::flush().await;
            let settings = settings::load().await;
            theme_mode.set(settings.display_mode);
            theme::apply_display_mode(settings.display_mode);
            sync::start().await;
        });
    });
    use_effect(move || {
        if let Some(root) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.document_element())
        {
            let _ = root.set_attribute("data-app-ready", "true");
        }
    });

    rsx! {
        PwaBanner {}
        dioxus_router::Router::<Route> {}
    }
}
