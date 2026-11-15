//! Service Worker のキャッシュとプッシュ通知 (REQ-TMR-008、ADR-020)。
//!
//! イベントの接続はローダーの JavaScript が `service_worker_start` を呼ぶ。
//! キャッシュの版と対象の判定は `warrior_core::pwa` の純粋な関数を使う。
//! プッシュの本文は固定の文言を使い、記録の内容を含めない (ADR-008)。

use js_sys::{Array, Promise};
use warrior_core::push::{NOTIFICATION_BODY, NOTIFICATION_TITLE};
use warrior_core::pwa::{cache_name, is_cacheable_path};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use web_sys::{
    Cache, CacheStorage, Client, ClientQueryOptions, ClientType, Clients, Event, ExtendableEvent,
    FetchEvent, NotificationOptions, Request, Response, ServiceWorkerGlobalScope, WindowClient,
};

/// Service Worker のキャッシュの版。wasm と JS のビルドごとに更新する。
const CACHE_VERSION: &str = env!("WARRIOR_SW_VERSION");

/// インストール時にキャッシュする静的なファイル。
const INSTALL_ASSETS: &[&str] = &[
    "/",
    "/index.html",
    "/tailwind.css",
    "/generated/tokens.css",
    "/manifest.webmanifest",
    "/icons/icon-192.svg",
    "/icons/icon-512.svg",
    "/sw/warrior_sw_bg.wasm",
];

/// Service Worker の起動時にイベントを接続する。
#[wasm_bindgen]
pub fn service_worker_start() {
    let scope: ServiceWorkerGlobalScope = js_sys::global().unchecked_into();
    connect_install(&scope);
    connect_activate(&scope);
    connect_fetch(&scope);
    connect_message(&scope);
    connect_push(&scope);
    connect_notification_click(&scope);
}

#[wasm_bindgen]
pub fn service_worker_install() -> Promise {
    future_to_promise(async move {
        install().await?;
        Ok(JsValue::UNDEFINED)
    })
}

fn connect_install(scope: &ServiceWorkerGlobalScope) {
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Ok(event) = event.dyn_into::<ExtendableEvent>() else {
            return;
        };
        let promise = future_to_promise(async move {
            if let Err(error) = install().await {
                worker_log(&format!("install failed: {error:?}"));
            }
            Ok(JsValue::UNDEFINED)
        });
        let _ = event.wait_until(&promise);
    });
    let _ = scope.add_event_listener_with_callback("install", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn connect_activate(scope: &ServiceWorkerGlobalScope) {
    let scope_for_activate = scope.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Ok(event) = event.dyn_into::<ExtendableEvent>() else {
            return;
        };
        let scope = scope_for_activate.clone();
        let promise = future_to_promise(async move {
            if let Err(error) = activate(&scope).await {
                worker_log(&format!("activate failed: {error:?}"));
            }
            Ok(JsValue::UNDEFINED)
        });
        let _ = event.wait_until(&promise);
    });
    let _ = scope.add_event_listener_with_callback("activate", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn connect_fetch(scope: &ServiceWorkerGlobalScope) {
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let event: FetchEvent = event.unchecked_into();
        let request = event.request();
        if !is_cacheable_path(&request.url()) {
            return;
        }
        let promise = future_to_promise(async move {
            match fetch_and_cache(&request).await {
                Ok(response) => Ok(response.into()),
                Err(error) => {
                    worker_log(&format!("fetch failed: {error:?}"));
                    Err(error)
                }
            }
        });
        let _ = event.respond_with(&promise);
    });
    let _ = scope.add_event_listener_with_callback("fetch", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn connect_message(scope: &ServiceWorkerGlobalScope) {
    let scope_for_message = scope.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Some(message) = event.dyn_ref::<web_sys::MessageEvent>() else {
            return;
        };
        if message.data().as_string().as_deref() == Some("skip-waiting") {
            let _ = scope_for_message.skip_waiting();
        }
    });
    let _ = scope.add_event_listener_with_callback("message", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn connect_push(scope: &ServiceWorkerGlobalScope) {
    let scope_for_push = scope.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Ok(event) = event.dyn_into::<ExtendableEvent>() else {
            return;
        };
        let scope = scope_for_push.clone();
        let promise = future_to_promise(async move {
            if let Err(error) = show_rest_finished(&scope).await {
                worker_log(&format!("push failed: {error:?}"));
            }
            Ok(JsValue::UNDEFINED)
        });
        if let Err(error) = event.wait_until(&promise) {
            worker_log(&format!("push wait_until failed: {error:?}"));
        }
    });
    let _ = scope.add_event_listener_with_callback("push", closure.as_ref().unchecked_ref());
    closure.forget();
}

fn connect_notification_click(scope: &ServiceWorkerGlobalScope) {
    let scope_for_click = scope.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Ok(event) = event.dyn_into::<ExtendableEvent>() else {
            return;
        };
        let scope = scope_for_click.clone();
        let promise = future_to_promise(async move {
            if let Err(error) = focus_or_open(&scope).await {
                worker_log(&format!("notificationclick failed: {error:?}"));
            }
            Ok(JsValue::UNDEFINED)
        });
        if let Err(error) = event.wait_until(&promise) {
            worker_log(&format!("notificationclick wait_until failed: {error:?}"));
        }
    });
    let _ = scope
        .add_event_listener_with_callback("notificationclick", closure.as_ref().unchecked_ref());
    closure.forget();
}

async fn show_rest_finished(scope: &ServiceWorkerGlobalScope) -> Result<(), JsValue> {
    let options = NotificationOptions::new();
    options.set_body(NOTIFICATION_BODY);
    let registration = scope.registration();
    let shown = registration.show_notification_with_options(NOTIFICATION_TITLE, &options)?;
    JsFuture::from(shown).await?;
    Ok(())
}

async fn focus_or_open(scope: &ServiceWorkerGlobalScope) -> Result<(), JsValue> {
    let clients: Clients = scope.clients();
    let options = ClientQueryOptions::new();
    options.set_type(ClientType::Window);
    options.set_include_uncontrolled(true);
    let windows = JsFuture::from(clients.match_all_with_options(&options))
        .await?
        .dyn_into::<Array>()?;
    let origin = scope.origin();
    for value in windows.iter() {
        let client: Client = value.unchecked_into();
        if !client.url().starts_with(&origin) {
            continue;
        }
        let window: WindowClient = client.unchecked_into();
        // フォーカスに失敗しても、ほかの画面か新しい画面で開けるか試す。
        if let Ok(promise) = window.focus() {
            if JsFuture::from(promise).await.is_ok() {
                return Ok(());
            }
        }
    }
    JsFuture::from(clients.open_window("/")).await?;
    Ok(())
}

async fn install() -> Result<(), JsValue> {
    let cache = open_cache().await?;
    let assets = Array::new();
    for asset in INSTALL_ASSETS {
        assets.push(&JsValue::from_str(asset));
    }
    JsFuture::from(cache.add_all_with_str_sequence(&assets)).await?;
    Ok(())
}

async fn activate(scope: &ServiceWorkerGlobalScope) -> Result<(), JsValue> {
    let storage = scope.caches()?;
    let names = JsFuture::from(storage.keys()).await?;
    let names = names.dyn_into::<Array>()?;
    let current = cache_name(CACHE_VERSION);
    for value in names.iter() {
        let Some(name) = value.as_string() else {
            continue;
        };
        if name.starts_with("warrior-static-") && name != current {
            JsFuture::from(storage.delete(&name)).await?;
        }
    }
    JsFuture::from(scope.clients().claim()).await?;
    Ok(())
}

async fn fetch_and_cache(request: &Request) -> Result<Response, JsValue> {
    let cache = open_cache().await?;
    let key = request.url();
    if let Ok(cached) = JsFuture::from(cache.match_with_str(&key))
        .await?
        .dyn_into::<Response>()
    {
        return Ok(cached);
    }
    let global: ServiceWorkerGlobalScope = js_sys::global().unchecked_into();
    let response = JsFuture::from(global.fetch_with_request(request)).await?;
    let response = response.dyn_into::<Response>()?;
    if response.ok() {
        let _ = JsFuture::from(cache.add_with_str(&key)).await;
    }
    Ok(response)
}

async fn open_cache() -> Result<Cache, JsValue> {
    let global: ServiceWorkerGlobalScope = js_sys::global().unchecked_into();
    let storage: CacheStorage = global.caches()?;
    JsFuture::from(storage.open(&cache_name(CACHE_VERSION)))
        .await?
        .dyn_into::<Cache>()
}

fn worker_log(message: &str) {
    let global = js_sys::global();
    if let Ok(log) = js_sys::Reflect::get(&global, &JsValue::from_str("console")) {
        if let Ok(log) = log.dyn_into::<js_sys::Object>() {
            if let Ok(error) = js_sys::Reflect::get(&log, &JsValue::from_str("error")) {
                if let Ok(error) = error.dyn_into::<js_sys::Function>() {
                    let _ = error.call1(&log, &JsValue::from_str(message));
                }
            }
        }
    }
}
