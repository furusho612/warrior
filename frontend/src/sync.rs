//! 送信待ちの列をサーバーへ送る。
//!
//! 起動時と online イベントで送り、2xx で未保存の表示を消す (ADR-002)。
//! 固定の sleep は使わず、通信の失敗は次の online イベントか起動で再試行する。

use std::cell::Cell;

use dioxus::prelude::*;
use warrior_core::sync::{
    failed_count, next_sendable, unsaved_count, Operation, OutboxEntry, OutboxState,
};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use crate::storage::{self, StorageError};

/// 画面に出す同期の状態。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStatus {
    /// 未保存の操作の件数。Pending、Sending、Blocked を数える。
    pub unsaved: usize,
    /// 失敗した操作の件数。
    pub failed: usize,
}

/// 画面が読む同期の状態。
pub static SYNC_STATUS: GlobalSignal<SyncStatus> = Signal::global(SyncStatus::default);
static SYNC_OUTBOX: GlobalSignal<Vec<OutboxEntry>> = Signal::global(Vec::new);

thread_local! {
    static FLUSHING: Cell<bool> = const { Cell::new(false) };
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// 起動時の永続ストレージの要求、online の購読、最初の送信を行う。
pub async fn start() {
    let _ = storage::request_persistence().await;
    listen_online();
    refresh_status().await;
    flush().await;
}

/// 現在の同期の状態。
pub fn status() -> SyncStatus {
    SYNC_STATUS()
}

/// 失敗した操作の一覧。
pub fn failed_operations() -> Vec<OutboxEntry> {
    SYNC_OUTBOX()
        .iter()
        .filter(|entry| entry.state == OutboxState::Failed)
        .cloned()
        .collect()
}

/// 端末内の保存の後に状態を読み直し、送信を始める。
pub async fn after_local_save() {
    refresh_status().await;
    flush().await;
}

/// 失敗した操作を再試行する。
pub async fn retry_operation(operation_id: &str) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let _ = storage.retry(operation_id).await;
    refresh_status().await;
    flush().await;
}

/// 失敗した操作とその子孫を破棄する。
pub async fn discard_operation(operation_id: &str) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let _ = storage.discard(operation_id).await;
    refresh_status().await;
}

/// アカウントの削除で端末内の記録と送信待ちの列を削除する。
pub async fn clear_account() -> Result<(), StorageError> {
    storage::clear_account().await?;
    set_outbox(Vec::new());
    Ok(())
}

/// 送信待ちの列を読み直し、状態を画面へ反映する。
pub async fn refresh_status() {
    let Ok(storage) = storage::current().await else {
        return;
    };
    if let Ok(entries) = storage.outbox_entries().await {
        set_outbox(entries);
    }
}

fn set_outbox(entries: Vec<OutboxEntry>) {
    *SYNC_STATUS.write() = SyncStatus {
        unsaved: unsaved_count(&entries),
        failed: failed_count(&entries),
    };
    *SYNC_OUTBOX.write() = entries;
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
        spawn(async move {
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
        set_outbox(entries.clone());
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
                refresh_status().await;
                return;
            }
            SendOutcome::Transient => {
                let _ = storage.mark_transient_failure(&operation_id).await;
                refresh_status().await;
                return;
            }
        }
    }
}

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
    match response.status() {
        200..=299 => SendOutcome::Success,
        400 | 404 => SendOutcome::Permanent,
        401 => SendOutcome::Unauthorized,
        429 | 500..=599 => SendOutcome::Transient,
        _ => SendOutcome::Permanent,
    }
}
