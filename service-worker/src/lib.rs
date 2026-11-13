//! Service Worker のキャッシュ。
//!
//! イベントの接続はローダーの JavaScript が `service_worker_start` を呼ぶ。
//! キャッシュの版と対象の判定は `warrior_core::pwa` の純粋な関数を使う。

use js_sys::{Array, Promise};
use warrior_core::pwa::{cache_name, is_cacheable_path};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
use web_sys::{
    Cache, CacheStorage, Event, ExtendableEvent, FetchEvent, Request, Response,
    ServiceWorkerGlobalScope,
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
