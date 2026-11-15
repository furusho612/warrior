//! 送信待ちの列をサーバーへ送る。
//!
//! 起動時と online イベントで送り、2xx で未保存の表示を消す (ADR-002)。
//! セットの完了の応答が返す自己ベストは、記録中のトレーニングの端末内の行へ反映する。
//! 種目の変更も同じ送信待ちの考え方で送り、操作 ID で再送の重複を防ぐ。
//! 送信の再試行は固定の sleep で待たず、次の online イベントか起動で行う。
//! テストの待機も条件で行い、固定の sleep を書かない (ADR-024)。

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::task::{Poll, Waker};

use warrior_core::sync::{next_sendable, Operation, OperationResponse, OutboxState};
use warrior_core::training_session::PersonalBestItem;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

use crate::http::{self, HttpError};
use crate::storage::{self, DiscardOutcome, ExerciseChangeKind, ExerciseOutboxEntry, StorageError};

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
    static FLUSH_WAITERS: RefCell<Vec<Waker>> = const { RefCell::new(Vec::new()) };
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// 起動時の永続ストレージの要求、online の購読、最初の送信を行う。
pub async fn start() {
    let _ = storage::request_persistence().await;
    listen_online();
    flush().await;
    let _ = flush_exercise_changes().await;
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

/// 編集権を失ったトレーニングを記録し、以後の送信を止める。
///
/// 記録できたときだけ true を返す。画面は true のとき閲覧のみへ切り替える。
pub async fn mark_training_forbidden(training_id: &str) -> bool {
    let Ok(storage) = storage::current().await else {
        return false;
    };
    storage.mark_training_forbidden(training_id).await.is_ok()
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
        flush_in_background();
        wasm_bindgen_futures::spawn_local(async {
            let _ = flush_exercise_changes().await;
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

/// 送信待ちの列を、呼び出し元の処理を待たせずに送る。
///
/// 入力の保存の後に呼ぶ。送信の往復は Dioxus の runtime の外で進め、次の入力を止めない。
pub fn flush_in_background() {
    wasm_bindgen_futures::spawn_local(async {
        flush().await;
    });
}

/// 送信待ちの列を先頭から送る。
///
/// 実行中の flush があるときは、それが終わるのを待ってから送る。待たずに戻ると、
/// あとから積んだ操作が送られないまま未保存として残る。
pub async fn flush() {
    loop {
        wait_until_idle().await;
        // 待っている間に別の flush が始まることがある。もう一度確かめる。
        if FLUSHING.with(Cell::get) {
            continue;
        }
        FLUSHING.with(|flushing| flushing.set(true));
        let _guard = FlushGuard;
        flush_inner().await;
        return;
    }
}

/// 実行中の flush が終わるまで待つ。
async fn wait_until_idle() {
    poll_fn(|context| {
        if !FLUSHING.with(Cell::get) {
            return Poll::Ready(());
        }
        FLUSH_WAITERS.with(|waiters| {
            let mut waiters = waiters.borrow_mut();
            if !waiters.iter().any(|waker| waker.will_wake(context.waker())) {
                waiters.push(context.waker().clone());
            }
        });
        Poll::Pending
    })
    .await;
}

struct FlushGuard;

impl Drop for FlushGuard {
    fn drop(&mut self) {
        FLUSHING.with(|flushing| flushing.set(false));
        let waiters = FLUSH_WAITERS.with(|waiters| std::mem::take(&mut *waiters.borrow_mut()));
        for waker in waiters {
            waker.wake();
        }
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
        let forbidden = storage.forbidden_trainings().await.unwrap_or_default();
        let Ok(entries) = storage.outbox_entries().await else {
            return;
        };
        // 編集権を失ったトレーニングは、引き継ぐか読み直すまで送らない。
        let sendable = entries
            .into_iter()
            .filter(|entry| !forbidden.contains(&entry.operation.training_id))
            .collect::<Vec<_>>();
        let Some(entry) = next_sendable(&sendable).cloned() else {
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
        let (outcome, personal_bests) = outcome;

        match outcome {
            SendOutcome::Success => {
                if !personal_bests.is_empty()
                    && storage
                        .merge_personal_bests(&entry.operation.training_id, &personal_bests)
                        .await
                        .is_err()
                {
                    return;
                }
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
            SendOutcome::Forbidden => {
                if !mark_training_forbidden(&entry.operation.training_id).await {
                    return;
                }
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
    /// 記録中のトレーニングの編集権を持たない。
    Forbidden,
    Transient,
}

async fn send(operation: &Operation) -> (SendOutcome, Vec<PersonalBestItem>) {
    let Ok(body) = serde_json::to_string(operation) else {
        return (SendOutcome::Permanent, Vec::new());
    };
    let Some(window) = web_sys::window() else {
        return (SendOutcome::Transient, Vec::new());
    };
    let headers = match Headers::new() {
        Ok(headers) => headers,
        Err(_) => return (SendOutcome::Transient, Vec::new()),
    };
    if headers.set("Content-Type", "application/json").is_err() {
        return (SendOutcome::Transient, Vec::new());
    }
    let init = RequestInit::new();
    init.set_method("POST");
    init.set_headers(&headers);
    init.set_body(&JsValue::from_str(&body));
    let request = match Request::new_with_str_and_init("/api/sync/operations", &init) {
        Ok(request) => request,
        Err(_) => return (SendOutcome::Transient, Vec::new()),
    };
    let response = JsFuture::from(window.fetch_with_request(&request)).await;
    let Ok(response) = response else {
        return (SendOutcome::Transient, Vec::new());
    };
    let Ok(response) = response.dyn_into::<Response>() else {
        return (SendOutcome::Transient, Vec::new());
    };
    let outcome = classify_status(response.status());
    if outcome != SendOutcome::Success {
        return (outcome, Vec::new());
    }
    let personal_bests = match response.text() {
        Ok(promise) => match JsFuture::from(promise).await {
            Ok(value) => value.as_string(),
            Err(_) => None,
        },
        Err(_) => None,
    }
    .and_then(|text| serde_json::from_str::<OperationResponse>(&text).ok())
    .map(|response| response.personal_bests)
    .unwrap_or_default();
    (SendOutcome::Success, personal_bests)
}

fn classify_status(status: u16) -> SendOutcome {
    match status {
        200..=299 => SendOutcome::Success,
        400 | 404 => SendOutcome::Permanent,
        401 => SendOutcome::Unauthorized,
        403 => SendOutcome::Forbidden,
        408 | 425 | 429 | 500..=599 => SendOutcome::Transient,
        _ => SendOutcome::Permanent,
    }
}

/// 種目の変更の送信待ちをサーバーへ送る。
///
/// 送信できる変更が無くなったら true を返す。通信の失敗と 5xx は Pending に戻し、
/// 次の online イベントか起動で再送する。400 と 404 は Failed にして後続を止めない。
pub async fn flush_exercise_changes() -> bool {
    loop {
        wait_until_idle().await;
        // 待っている間に別の flush が始まることがある。もう一度確かめる。
        if FLUSHING.with(Cell::get) {
            continue;
        }
        FLUSHING.with(|flushing| flushing.set(true));
        let _guard = FlushGuard;
        return flush_exercise_inner().await;
    }
}

async fn flush_exercise_inner() -> bool {
    let Ok(storage) = storage::current().await else {
        return false;
    };
    let generation = storage::generation();
    loop {
        let Ok(entries) = storage.exercise_changes().await else {
            return false;
        };
        let Some(entry) = entries
            .iter()
            .find(|entry| entry.state == OutboxState::Pending)
            .cloned()
        else {
            return !entries
                .iter()
                .any(|entry| entry.state == OutboxState::Sending);
        };
        if storage
            .mark_exercise_change_sending(&entry.operation_id)
            .await
            .is_err()
        {
            return false;
        }
        match send_exercise_change(&entry).await {
            _ if storage::generation() != generation => return false,
            Ok(status) if (200..=299).contains(&status) => {
                if storage
                    .mark_exercise_change_success(&entry.operation_id)
                    .await
                    .is_err()
                {
                    return false;
                }
            }
            Ok(401) => {
                let _ = storage
                    .mark_exercise_change_transient(&entry.operation_id)
                    .await;
                return false;
            }
            Ok(408 | 425 | 429 | 500..=599) => {
                let _ = storage
                    .mark_exercise_change_transient(&entry.operation_id)
                    .await;
                return false;
            }
            Ok(400 | 404) => {
                if storage
                    .mark_exercise_change_permanent(&entry.operation_id)
                    .await
                    .is_err()
                {
                    return false;
                }
            }
            Ok(_) => {
                if storage
                    .mark_exercise_change_permanent(&entry.operation_id)
                    .await
                    .is_err()
                {
                    return false;
                }
            }
            Err(HttpError::Unavailable) => {
                let _ = storage
                    .mark_exercise_change_transient(&entry.operation_id)
                    .await;
                return false;
            }
            Err(_) => {
                if storage
                    .mark_exercise_change_permanent(&entry.operation_id)
                    .await
                    .is_err()
                {
                    return false;
                }
            }
        }
    }
}

async fn send_exercise_change(entry: &ExerciseOutboxEntry) -> Result<u16, HttpError> {
    match entry.kind {
        ExerciseChangeKind::Create => {
            let input = entry.input.as_ref().ok_or(HttpError::InvalidRequest)?;
            http::request_status_with_operation(
                "POST",
                "/api/exercises",
                &entry.operation_id,
                input,
            )
            .await
        }
        ExerciseChangeKind::Update => {
            let id = entry
                .exercise_id
                .as_deref()
                .ok_or(HttpError::InvalidRequest)?;
            let input = entry.input.as_ref().ok_or(HttpError::InvalidRequest)?;
            http::request_status_with_operation(
                "PUT",
                &format!("/api/exercises/{id}"),
                &entry.operation_id,
                input,
            )
            .await
        }
        ExerciseChangeKind::Delete => {
            let id = entry
                .exercise_id
                .as_deref()
                .ok_or(HttpError::InvalidRequest)?;
            http::request_status_for_operation(
                "DELETE",
                &format!("/api/exercises/{id}"),
                &entry.operation_id,
            )
            .await
        }
        ExerciseChangeKind::Reset => {
            let id = entry
                .exercise_id
                .as_deref()
                .ok_or(HttpError::InvalidRequest)?;
            http::request_status_for_operation(
                "POST",
                &format!("/api/exercises/{id}/reset"),
                &entry.operation_id,
            )
            .await
        }
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
        assert_eq!(classify_status(404), SendOutcome::Permanent);
        assert_eq!(classify_status(401), SendOutcome::Unauthorized);
        assert_eq!(classify_status(403), SendOutcome::Forbidden);
        assert_eq!(classify_status(200), SendOutcome::Success);
    }
}
