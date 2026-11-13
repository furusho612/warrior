//! PWA の Service Worker の登録、更新の案内、iOS のホーム画面への追加の案内。

use dioxus::prelude::*;

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{MediaQueryList, ServiceWorkerContainer, ServiceWorkerRegistration};

use crate::text::{text, Key};

/// Service Worker を登録する。
pub async fn register(mut update_available: Signal<bool>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let container = window.navigator().service_worker();
    let has_controller = ServiceWorkerContainer::controller(&container).is_some();
    let registered = JsFuture::from(ServiceWorkerContainer::register(&container, "/sw.js")).await;
    let Ok(registered) = registered else {
        return;
    };
    let registration: ServiceWorkerRegistration = registered.unchecked_into();
    if has_controller && ServiceWorkerRegistration::waiting(&registration).is_some() {
        update_available.set(true);
    }
    let registration_for_event = registration.clone();
    let closure = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
        move |_event: web_sys::Event| {
            if let Some(installing) = ServiceWorkerRegistration::installing(&registration_for_event)
            {
                let registration_for_state = registration_for_event.clone();
                let state_closure =
                    wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
                        move |_event: web_sys::Event| {
                            if has_controller
                                && ServiceWorkerRegistration::waiting(&registration_for_state)
                                    .is_some()
                            {
                                update_available.set(true);
                            }
                        },
                    );
                let _ = installing.add_event_listener_with_callback(
                    "statechange",
                    state_closure.as_ref().unchecked_ref(),
                );
                state_closure.forget();
            }
        },
    );
    let _ = registration
        .add_event_listener_with_callback("updatefound", closure.as_ref().unchecked_ref());
    closure.forget();
}

/// 待機中の Service Worker を有効にし、画面を再読み込みする。
pub async fn activate_waiting() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let container = window.navigator().service_worker();
    let Ok(ready) = ServiceWorkerContainer::ready(&container) else {
        return;
    };
    let ready = JsFuture::from(ready).await;
    if let Ok(ready) = ready {
        let registration: ServiceWorkerRegistration = ready.unchecked_into();
        if let Some(waiting) = ServiceWorkerRegistration::waiting(&registration) {
            let _ = waiting.post_message(&JsValue::from_str("skip-waiting"));
        }
    }
    let _ = window.location().reload();
}

/// iOS でホーム画面に追加されていないかを返す。
pub fn ios_install_hint() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let user_agent = window.navigator().user_agent().unwrap_or_default();
    let is_ios =
        user_agent.contains("iPhone") || user_agent.contains("iPad") || user_agent.contains("iPod");
    if !is_ios {
        return false;
    }
    let standalone = window
        .match_media("(display-mode: standalone)")
        .ok()
        .flatten()
        .map(|query| MediaQueryList::matches(&query))
        .unwrap_or(false);
    !standalone
}

/// PWA の更新と iOS の追加の案内。
#[component]
pub fn PwaBanner() -> Element {
    let update_available = use_signal(|| false);
    let mut install_hint = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            register(update_available).await;
        });
        install_hint.set(ios_install_hint());
    });

    rsx! {
        if update_available() {
            div {
                class: "fixed inset-x-0 top-0 z-50 flex min-h-11 items-center justify-between gap-2 border-b border-line bg-surface px-4 text-[13px] text-text",
                span { "{text(Key::PwaUpdateAvailable)}" }
                button {
                    class: "min-h-11 border border-primary px-3 text-primary",
                    onclick: move |_| {
                        spawn(async move { activate_waiting().await; });
                    },
                    "{text(Key::PwaReload)}"
                }
            }
        }
        if install_hint() {
            p { class: "fixed inset-x-0 bottom-20 z-40 border-t border-line bg-surface px-4 py-2 text-center text-[11px] leading-4 text-text-sub",
                "{text(Key::IosInstallHint)}"
            }
        }
    }
}
