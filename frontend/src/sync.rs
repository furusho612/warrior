//! 送信待ちの列をサーバーへ送る。
//!
//! 起動時と online イベントで送り、2xx で未保存の表示を消す (ADR-002)。
//! 固定の sleep は使わず、通信の失敗は次の online イベントか起動で再試行する。

use std::cell::Cell;

use warrior_core::sync::{next_sendable, Operation};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use crate::storage::{self, DiscardOutcome, StorageError};

/// 画面に出す同期の状態。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStatus {
    /// 未保存の操作の件数。Pending、Sending、Blocked を数える。
    pub unsaved: usize,
    /// 失敗した操作の件数。
    pub failed: usize,
}

thread_local! {
    static FLUSHING: Cell<bool> = const { Cell::new(false) };
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// 起動時の永続ストレージの要求、online の購読、最初の送信を行う。
pub async fn start() {
    let _ = storage::request_persistence().await;
    listen_online();
    flush().await;
}

/// 失敗した操作を再試行する。
pub async fn retry_operation(operation_id: &str) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let _ = storage.retry(operation_id).await;
}

/// 失敗した操作とその子孫を破棄する。
pub async fn discard_operation(operation_id: &str) -> Option<DiscardOutcome> {
    let storage = storage::current().await.ok()?;
    let outcome = storage.discard(operation_id).await.ok()?;
    Some(outcome)
}

/// アカウントの削除で端末内の記録と送信待ちの列を削除する。
pub async fn clear_account() -> Result<(), StorageError> {
    storage::clear_account().await?;
    Ok(())
}

fn listen_online() {
    if LISTENING.with(|listening| listening.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        LISTENING.with(|listening| listening.set(false));
        return;
    };
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
        wasm_bindgen_futures::spawn_local(async move {
            flush().await;
        });
    });
    if window
        .add_event_listener_with_callback("online", closure.as_ref().unchecked_ref())
        .is_err()
    {
        LISTENING.with(|listening| listening.set(false));
        return;
    }
    closure.forget();
}

/// 送信待ちの列を先頭から送る。同じ flush を同時に走らせない。
pub async fn flush() {
    if FLUSHING.with(Cell::get) {
        return;
    }
    FLUSHING.with(|flushing| flushing.set(true));
    let _guard = FlushGuard;
    flush_inner().await;
}

struct FlushGuard;

impl Drop for FlushGuard {
    fn drop(&mut self) {
        FLUSHING.with(|flushing| flushing.set(false));
    }
}

async fn flush_inner() {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let generation = storage::generation();
    if storage.recover_interrupted().await.is_err() {
        return;
    }
    if storage::generation() != generation {
        return;
    }

    loop {
        if storage::generation() != generation {
            return;
        }
        let Ok(entries) = storage.outbox_entries().await else {
            return;
        };
        let Some(entry) = next_sendable(&entries).cloned() else {
            return;
        };
        let operation_id = entry.operation.operation_id.clone();
        if storage.mark_sending(&operation_id).await.is_err() {
            return;
        }
        if storage::generation() != generation {
            return;
        }

        let outcome = send(&entry.operation).await;
        if storage::generation() != generation {
            return;
        }

        match outcome {
            SendOutcome::Success => {
                if storage.mark_success(&operation_id).await.is_err() {
                    return;
                }
            }
            SendOutcome::Permanent => {
                if storage.mark_permanent_failure(&operation_id).await.is_err() {
                    return;
                }
            }
            SendOutcome::Unauthorized => {
                let _ = storage.return_to_pending(&operation_id).await;
                return;
            }
            SendOutcome::Transient => {
                let _ = storage.mark_transient_failure(&operation_id).await;
                return;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendOutcome {
    Success,
    Permanent,
    Unauthorized,
    Transient,
}

async fn send(operation: &Operation) -> SendOutcome {
    let Ok(body) = serde_json::to_string(operation) else {
        return SendOutcome::Permanent;
    };
    let Some(window) = web_sys::window() else {
        return SendOutcome::Transient;
    };
    let headers = match Headers::new() {
        Ok(headers) => headers,
        Err(_) => return SendOutcome::Transient,
    };
    if headers.set("Content-Type", "application/json").is_err() {
        return SendOutcome::Transient;
    }
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body));
    let request = match Request::new_with_str_and_init("/api/sync/operations", &init) {
        Ok(request) => request,
        Err(_) => return SendOutcome::Transient,
    };
    let response = JsFuture::from(window.fetch_with_request(&request)).await;
    let Ok(response) = response else {
        return SendOutcome::Transient;
    };
    let Ok(response) = response.dyn_into::<Response>() else {
        return SendOutcome::Transient;
    };
    classify_status(response.status())
}

fn classify_status(status: u16) -> SendOutcome {
    match status {
        200..=299 => SendOutcome::Success,
        400 | 404 => SendOutcome::Permanent,
        401 => SendOutcome::Unauthorized,
        408 | 425 | 429 | 500..=599 => SendOutcome::Transient,
        _ => SendOutcome::Permanent,
    }
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;
    use wasm_bindgen_test::wasm_bindgen_test_configure;

    use super::{classify_status, SendOutcome};

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn request_timeout_and_too_early_are_transient() {
        assert_eq!(classify_status(408), SendOutcome::Transient);
        assert_eq!(classify_status(425), SendOutcome::Transient);
        assert_eq!(classify_status(429), SendOutcome::Transient);
        assert_eq!(classify_status(500), SendOutcome::Transient);
        assert_eq!(classify_status(400), SendOutcome::Permanent);
        assert_eq!(classify_status(401), SendOutcome::Unauthorized);
        assert_eq!(classify_status(200), SendOutcome::Success);
    }
}
