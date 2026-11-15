//! アカウントの削除の後に、端末に保存したデータを消す。
//!
//! 設定と Cache API のキャッシュを消し、Service Worker の登録を解除してから、
//! IndexedDB と送信待ちの列を削除する (REQ-NFR-010)。
//! 登録を解除すると、次の起動で Service Worker がアプリシェルをキャッシュし直す。
//! セッション Cookie、端末トークンの Cookie、再認証のトークンの Cookie は、
//! サーバーの削除の応答が失効させる。

use js_sys::Array;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::Storage as WebStorage;

use super::{clear_account, StorageError};

/// アカウントの削除の後に、設定、キャッシュ、IndexedDB、送信待ちの列を削除する。
pub async fn clear_after_deletion() -> Result<(), StorageError> {
    clear_web_storage()?;
    unregister_service_workers().await?;
    clear_caches().await?;
    clear_account().await
}

fn clear_web_storage() -> Result<(), StorageError> {
    let window = web_sys::window()
        .ok_or_else(|| StorageError::Unavailable("the window is missing".to_owned()))?;
    // ブラウザが保存を提供しない場合は、消す設定が無い。
    clear_storage(window.local_storage().ok().flatten())?;
    clear_storage(window.session_storage().ok().flatten())
}

fn clear_storage(storage: Option<WebStorage>) -> Result<(), StorageError> {
    let Some(storage) = storage else {
        return Ok(());
    };
    storage
        .clear()
        .map_err(|_| StorageError::Request("the settings could not be cleared".to_owned()))
}

async fn unregister_service_workers() -> Result<(), StorageError> {
    let Some(window) = web_sys::window() else {
        return Ok(());
    };
    let container = window.navigator().service_worker();
    let registrations = JsFuture::from(container.get_registrations())
        .await
        .map_err(|_| {
            StorageError::Request("the service worker registrations could not be read".to_owned())
        })?;
    let registrations = registrations.dyn_into::<Array>().map_err(|_| {
        StorageError::Request("the service worker registrations are not a list".to_owned())
    })?;
    for registration in registrations.iter() {
        let registration = registration
            .dyn_into::<web_sys::ServiceWorkerRegistration>()
            .map_err(|_| {
                StorageError::Request("the service worker registration is not readable".to_owned())
            })?;
        let unregister = registration.unregister().map_err(|_| {
            StorageError::Request("the service worker could not be unregistered".to_owned())
        })?;
        JsFuture::from(unregister).await.map_err(|_| {
            StorageError::Request("the service worker could not be unregistered".to_owned())
        })?;
    }
    Ok(())
}

async fn clear_caches() -> Result<(), StorageError> {
    let window = web_sys::window()
        .ok_or_else(|| StorageError::Unavailable("the window is missing".to_owned()))?;
    let Ok(caches) = window.caches() else {
        // Cache API を提供しないブラウザには、消すキャッシュが無い。
        return Ok(());
    };
    let keys = JsFuture::from(caches.keys())
        .await
        .map_err(|_| StorageError::Request("the cache names could not be read".to_owned()))?
        .dyn_into::<Array>()
        .map_err(|_| StorageError::Request("the cache names are not a list".to_owned()))?;
    for key in keys.iter() {
        let Some(name) = key.as_string() else {
            continue;
        };
        JsFuture::from(caches.delete(&name))
            .await
            .map_err(|_| StorageError::Request("the cache could not be deleted".to_owned()))?;
    }
    Ok(())
}
