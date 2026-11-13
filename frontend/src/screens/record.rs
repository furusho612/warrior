//! 記録の開始、記録中のメモの端末内への保存、トレーニングの終了。
//!
//! セット表、種目レール、レストタイマー、セット詳細の枠を置く。セットの入力と
//! レストタイマーの中身は後続の画面が入れる。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use dioxus_router::navigator;
use warrior_core::datetime;
use warrior_core::ids::uuid_v4_from_bytes;
use warrior_core::sync::{
    failed_count, unsaved_count, Operation, OperationKind, OutboxEntry, OutboxState,
    SessionCreatePayload, SessionRecord, SessionUpdatePayload, MEMO_MAX_CHARS,
};
use warrior_core::training_session::{SessionSource, TrainingSessionSummary};

use crate::routes::{navigate, Route};
use crate::storage::{self, DiscardOutcome, StorageError};
use crate::sync::{self, SyncStatus};
use crate::text::{text, Key};
use crate::training_session as training_api;

#[component]
pub fn RecordScreen() -> Element {
    let error = use_signal(|| Option::<String>::None);
    let starting = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            let Ok(storage) = storage::current().await else {
                return;
            };
            if let Ok(Some(session)) = storage.active_session().await {
                let _ = navigator().push(Route::RecordSession {
                    training_id: session.id,
                });
            }
        });
    });

    rsx! {
        section { class: "flex min-h-full flex-col gap-3 p-5",
            h1 { class: "text-lg font-bold", "{text(Key::RecordTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::RecordDescription)}" }
            button {
                class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                "data-start-record": "true",
                disabled: starting(),
                onclick: move |_| {
                    spawn(start_empty_session(starting, error));
                },
                "{text(Key::RecordStart)}"
            }
            if let Some(message) = error() {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        spawn(start_empty_session(starting, error));
                    },
                    "{text(Key::RecordRetry)}"
                }
            }
        }
    }
}

pub(crate) async fn start_empty_session(
    mut starting: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    starting.set(true);
    let result = create_session().await;
    starting.set(false);
    match result {
        Ok(session) => {
            error.set(None);
            let _ = navigator().push(Route::RecordSession {
                training_id: session.id,
            });
        }
        Err(_) => error.set(Some(text(Key::RecordStartFailed).to_owned())),
    }
}

/// 画面が読む記録の状態。
#[derive(Clone, PartialEq, Default)]
struct RecordState {
    status: SyncStatus,
    confirmed: String,
    failed: Vec<OutboxEntry>,
}

/// 画面が読む同期の状態の signal。
#[derive(Clone, Copy, PartialEq)]
struct StateSignals {
    status: Signal<SyncStatus>,
    failed: Signal<Vec<OutboxEntry>>,
    confirmed: Signal<String>,
}

/// 記録画面の入力と保存が共有する signal。
#[derive(Clone, Copy, PartialEq)]
struct RecordSignals {
    session: Signal<Option<SessionRecord>>,
    memo: Signal<String>,
    persisted_memo: Signal<String>,
    memo_revision: Signal<u64>,
    save_error: Signal<Option<String>>,
}

/// 画面が状態を読み直す間隔 (ミリ秒)。
///
/// 同期の完了を Dioxus の runtime の外から signal へ書くと panic するため、画面の側から
/// IndexedDB を読んで状態を反映する。
const STATE_POLL_INTERVAL_MS: i32 = 200;

#[component]
pub fn RecordSessionScreen(training_id: String) -> Element {
    let session = use_signal(|| Option::<SessionRecord>::None);
    let memo = use_signal(String::new);
    let persisted_memo = use_signal(String::new);
    let memo_revision = use_signal(|| 0u64);
    let load_error = use_signal(|| Option::<String>::None);
    let save_error = use_signal(|| Option::<String>::None);
    let status = use_signal(SyncStatus::default);
    let failed = use_signal(Vec::<OutboxEntry>::new);
    let confirmed = use_signal(String::new);
    let signals = RecordSignals {
        session,
        memo,
        persisted_memo,
        memo_revision,
        save_error,
    };
    let state_signals = StateSignals {
        status,
        failed,
        confirmed,
    };
    let route_id = training_id.clone();

    use_effect(move || {
        let id = route_id.clone();
        spawn(async move {
            load(id, signals, state_signals, load_error).await;
        });
    });

    let poll_id = training_id.clone();
    use_future(move || {
        let id = poll_id.clone();
        async move {
            loop {
                sleep_ms(STATE_POLL_INTERVAL_MS).await;
                let state = read_record_state(&id).await;
                apply_state(state_signals, state);
            }
        }
    });

    let persisted = (signals.persisted_memo)();
    let failed_entries = (state_signals.failed)();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-route": "/record/:training_id",
            "data-session-id": training_id,
            "data-persisted-memo": "{persisted}",
            RecordHeader { signals, state_signals }
            MemoEditor { signals, state_signals }
            RecordFinish { signals }
            SetTable {}
            SetDrawer {}
            RecordLoadError {
                training_id: training_id.clone(),
                signals,
                state_signals,
                error: load_error,
            }
            FailedOperations {
                failed: failed_entries,
                signals,
                state_signals,
            }
        }
    }
}

#[component]
fn SetTable() -> Element {
    rsx! {
        div { class: "flex flex-col gap-1",
            h2 { class: "text-sm font-bold", "{text(Key::RecordSets)}" }
            div {
                class: "h-[222px] overflow-y-auto border border-line bg-surface",
                "data-set-table": "true",
                "data-set-rows": "5",
                "data-set-row-height": "44",
                for index in 0..5 {
                    div {
                        class: "h-11 border-b border-line",
                        "data-set-row": "true",
                        "data-set-row-index": "{index}",
                    }
                }
            }
        }
    }
}

#[component]
fn SetDrawer() -> Element {
    rsx! {
        aside {
            class: "hidden border border-line bg-surface p-3",
            "data-set-drawer": "true",
            h2 { class: "text-sm font-bold", "{text(Key::RecordSetDetail)}" }
            p { class: "text-[11px] text-text-sub", "{text(Key::RecordSetDetailEmpty)}" }
        }
    }
}

#[component]
fn RecordFinish(signals: RecordSignals) -> Element {
    let finishing = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let enabled = (signals.session)().is_some();
    let busy = finishing();

    rsx! {
        button {
            class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
            "data-finish": "true",
            disabled: busy || !enabled,
            onclick: move |_| {
                spawn(finish_training(signals, finishing, error));
            },
            if busy {
                "{text(Key::RecordFinishing)}"
            } else {
                "{text(Key::RecordFinish)}"
            }
        }
        if let Some(message) = error() {
            p { class: "text-[13px] text-primary", "{message}" }
            button {
                class: "min-h-11 border border-line px-3 text-[13px]",
                onclick: move |_| {
                    spawn(finish_training(signals, finishing, error));
                },
                "{text(Key::RecordRetry)}"
            }
        }
    }
}

async fn finish_training(
    mut signals: RecordSignals,
    mut finishing: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    let Some(record) = (signals.session)() else {
        return;
    };
    finishing.set(true);
    error.set(None);
    // 送信待ちが残ったまま終了すると、サーバーの記録とサマリーが食い違う。
    sync::flush().await;
    match pending_operations(&record.id).await {
        Ok(true) => {
            finishing.set(false);
            error.set(Some(text(Key::RecordPendingOperations).to_owned()));
            return;
        }
        Ok(false) => {}
        Err(_) => {
            finishing.set(false);
            error.set(Some(text(Key::RecordFinishFailed).to_owned()));
            return;
        }
    }
    let result = training_api::finish(&record.id).await;
    finishing.set(false);
    let summary = match result {
        Ok(summary) => summary,
        Err(_) => {
            error.set(Some(text(Key::RecordFinishFailed).to_owned()));
            return;
        }
    };
    let completed = completed_session(record, now_iso(), summary);
    let saved = match storage::current().await {
        Ok(storage) => storage.save_session_summary(&completed).await,
        Err(error) => Err(error),
    };
    match saved {
        Ok(()) => {
            error.set(None);
            signals.session.set(Some(completed.clone()));
            navigate(&format!("/summary/{}", completed.id));
        }
        Err(_) => error.set(Some(text(Key::RecordSaveFailed).to_owned())),
    }
}

fn completed_session(
    record: SessionRecord,
    finished_at: String,
    summary: TrainingSessionSummary,
) -> SessionRecord {
    SessionRecord {
        finished_at: Some(finished_at.clone()),
        duration_seconds: Some(summary.duration_seconds),
        total_weight_kg: Some(summary.total_weight_kg),
        exercise_count: Some(summary.exercise_count),
        set_count: Some(summary.set_count),
        personal_bests: summary.personal_bests,
        updated_at: finished_at,
        ..record
    }
}

#[component]
fn RecordHeader(signals: RecordSignals, state_signals: StateSignals) -> Element {
    let (status_value, status_text) = sync_status_text(
        (state_signals.status)(),
        &(signals.memo)(),
        &(state_signals.confirmed)(),
    );
    rsx! {
        h1 { class: "text-lg font-bold", "{text(Key::RecordTitle)}" }
        p {
            class: "text-[13px] text-text-sub",
            "data-sync-status": status_value,
            "{status_text}"
        }
    }
}

#[component]
fn RecordLoadError(
    training_id: String,
    signals: RecordSignals,
    state_signals: StateSignals,
    error: Signal<Option<String>>,
) -> Element {
    if let Some(message) = error() {
        return rsx! {
            p { class: "text-[13px] text-primary", "{message}" }
            button {
                class: "min-h-11 border border-line px-3 text-[13px]",
                onclick: move |_| {
                    let id = training_id.clone();
                    spawn(async move {
                        load(id, signals, state_signals, error).await;
                    });
                },
                "{text(Key::RecordRetry)}"
            }
        };
    }
    if (signals.session)().is_none() {
        return rsx! { p { class: "text-[13px] text-text-sub", "{text(Key::RecordLoading)}" } };
    }
    rsx! {}
}

#[derive(Clone)]
struct SaveState {
    /// 保存中の表示。
    saving: Signal<bool>,
    /// 保存タスクが走っているか。
    in_flight: Rc<Cell<bool>>,
    /// 保存中に新しい入力があったか。
    dirty: Rc<Cell<bool>>,
    /// 入力中のメモ。保存の開始時の値を読む。
    draft: Rc<RefCell<String>>,
}

#[component]
fn MemoEditor(signals: RecordSignals, state_signals: StateSignals) -> Element {
    let saving = use_signal(|| false);
    let in_flight = use_hook(|| Rc::new(Cell::new(false)));
    let dirty = use_hook(|| Rc::new(Cell::new(false)));
    let draft = use_hook(|| Rc::new(RefCell::new((signals.memo)())));
    let state = SaveState {
        saving,
        in_flight,
        dirty,
        draft,
    };
    let input_state = state.clone();
    let retry_state = state.clone();
    let reset_state = state.clone();

    use_effect(move || {
        let _ = (signals.memo_revision)();
        *reset_state.draft.borrow_mut() = (*signals.memo.peek()).clone();
    });

    let revision = (signals.memo_revision)();
    let memo = (signals.memo)();
    let disabled = (signals.session)().is_none();
    let saving_now = (state.saving)();
    let save_error_message = (signals.save_error)();

    rsx! {
        div {
            key: "{revision}",
            class: "flex flex-col gap-3",
            label { class: "text-[13px] font-bold", r#for: "record-memo", "{text(Key::RecordMemoLabel)}" }
            textarea {
                id: "record-memo",
                class: "min-h-24 border border-line bg-surface p-3 text-[13px] leading-5",
                "data-memo": "true",
                initial_value: "{memo}",
                disabled,
                maxlength: "{MEMO_MAX_CHARS}",
                placeholder: "{text(Key::RecordMemoPlaceholder)}",
                oninput: move |event| {
                    *input_state.draft.borrow_mut() = event.value();
                    input_state.dirty.set(true);
                    if input_state.in_flight.replace(true) {
                        return;
                    }
                    spawn(save_loop(signals, input_state.clone()));
                },
            }
            if saving_now {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordSaving)}" }
            }
            if let Some(message) = save_error_message {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        if retry_state.in_flight.get() {
                            return;
                        }
                        retry_state.in_flight.set(true);
                        retry_state.dirty.set(true);
                        spawn(save_loop(signals, retry_state.clone()));
                    },
                    "{text(Key::RecordRetry)}"
                }
            }
        }
    }
}

#[component]
fn FailedOperations(
    failed: Vec<OutboxEntry>,
    signals: RecordSignals,
    state_signals: StateSignals,
) -> Element {
    let mut signals = signals;

    if failed.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "flex flex-col gap-2 border border-line bg-surface p-3",
            h2 { class: "text-sm font-bold", "{text(Key::RecordFailedOperations)}" }
            for entry in failed {
                div {
                    class: "flex min-h-11 flex-wrap items-center justify-between gap-2 border border-line px-3",
                    span { class: "text-[13px]", "{operation_label(entry.operation.kind)}" }
                    div { class: "flex gap-2",
                        button {
                            class: "min-h-11 border border-primary px-3 text-[13px] text-primary",
                            onclick: {
                                let operation_id = entry.operation.operation_id.clone();
                                let training_id = entry.operation.training_id.clone();
                                move |_| {
                                    let operation_id = operation_id.clone();
                                    let training_id = training_id.clone();
                                    spawn(async move {
                                        sync::retry_operation(&operation_id).await;
                                        refresh_state(&training_id, state_signals).await;
                                    });
                                }
                            },
                            "{text(Key::RecordRetry)}"
                        }
                        button {
                            class: "min-h-11 border border-line px-3 text-[13px]",
                            onclick: {
                                let operation_id = entry.operation.operation_id.clone();
                                let training_id = entry.operation.training_id.clone();
                                move |_| {
                                    let operation_id = operation_id.clone();
                                    let training_id = training_id.clone();
                                    spawn(async move {
                                        match sync::discard_operation(&operation_id).await {
                                            Some(DiscardOutcome::SessionDeleted) => {
                                                navigate("/record");
                                            }
                                            Some(DiscardOutcome::MemoReverted(updated)) => {
                                                signals.memo.set(updated.memo.clone());
                                                signals.persisted_memo.set(updated.memo.clone());
                                                signals
                                                    .memo_revision
                                                    .set((signals.memo_revision)() + 1);
                                                signals.session.set(Some(*updated));
                                                refresh_state(&training_id, state_signals).await;
                                            }
                                            _ => {}
                                        }
                                    });
                                }
                            },
                            "{text(Key::RecordDiscard)}"
                        }
                    }
                }
            }
        }
    }
}

async fn load(
    training_id: String,
    mut signals: RecordSignals,
    state_signals: StateSignals,
    mut error: Signal<Option<String>>,
) {
    let Ok(storage) = storage::current().await else {
        error.set(Some(text(Key::RecordLoadFailed).to_owned()));
        return;
    };
    match storage.session(&training_id).await {
        Ok(Some(record)) => {
            signals.memo.set(record.memo.clone());
            signals.persisted_memo.set(record.memo.clone());
            signals.memo_revision.set((signals.memo_revision)() + 1);
            signals.session.set(Some(record));
            error.set(None);
        }
        Ok(None) => error.set(Some(text(Key::RecordLoadFailed).to_owned())),
        Err(_) => error.set(Some(text(Key::RecordLoadFailed).to_owned())),
    }
    refresh_state(&training_id, state_signals).await;
}

async fn refresh_state(training_id: &str, mut state_signals: StateSignals) {
    sync::flush().await;
    let state = read_record_state(training_id).await;
    state_signals.status.set(state.status);
    state_signals.failed.set(state.failed);
    state_signals.confirmed.set(state.confirmed);
}

fn apply_state(mut state_signals: StateSignals, state: RecordState) {
    if *state_signals.status.peek() != state.status {
        state_signals.status.set(state.status);
    }
    if *state_signals.failed.peek() != state.failed {
        state_signals.failed.set(state.failed);
    }
    if *state_signals.confirmed.peek() != state.confirmed {
        state_signals.confirmed.set(state.confirmed);
    }
}

/// 次の状態の読み直しまで待つ。
///
/// 状態そのものは `read_record_state` が IndexedDB から読む。この待機は画面の読み直しの
/// 間隔を空けるためのものであり、テストのように条件の成立を時間で待つものではない。
async fn sleep_ms(milliseconds: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

async fn read_record_state(training_id: &str) -> RecordState {
    let Ok(storage) = storage::current().await else {
        return RecordState::default();
    };
    let entries = storage.outbox_entries().await.unwrap_or_default();
    let confirmed = storage
        .session(training_id)
        .await
        .ok()
        .flatten()
        .map(|session| session.confirmed_memo)
        .unwrap_or_default();
    RecordState {
        status: SyncStatus {
            unsaved: unsaved_count(&entries),
            failed: failed_count(&entries),
        },
        confirmed,
        failed: entries
            .into_iter()
            .filter(|entry| entry.state == OutboxState::Failed)
            .collect(),
    }
}

async fn save_loop(mut signals: RecordSignals, mut state: SaveState) {
    state.saving.set(true);
    loop {
        state.dirty.set(false);
        let Some(record) = (signals.session)() else {
            state.saving.set(false);
            state.in_flight.set(false);
            return;
        };
        let draft_text = state.draft.borrow().clone();
        match save_draft(record, &draft_text).await {
            Ok(updated) => {
                if *state.draft.borrow() == draft_text {
                    signals.persisted_memo.set(draft_text.clone());
                }
                signals.memo.set(draft_text);
                signals.session.set(Some(updated));
                signals.save_error.set(None);
                // 送信の往復を待たずに次の入力を保存する。画面の状態は 200ms の読み直しが更新する。
                sync::flush_in_background();
            }
            Err(_) => {
                signals
                    .save_error
                    .set(Some(text(Key::RecordSaveFailed).to_owned()));
                break;
            }
        }
        if !state.dirty.get() {
            break;
        }
    }
    state.saving.set(false);
    state.in_flight.set(false);
}

async fn pending_operations(training_id: &str) -> Result<bool, StorageError> {
    let storage = storage::current().await?;
    let entries = storage.outbox_entries().await?;
    Ok(entries
        .iter()
        .any(|entry| entry.operation.training_id == training_id))
}

async fn create_session() -> Result<SessionRecord, StorageError> {
    let now = now_iso();
    let date = datetime::japan_date_from_utc(&now)
        .ok_or_else(|| StorageError::InvalidOperation("the current time is invalid".to_owned()))?;
    let id = new_uuid()?;
    let session = SessionRecord {
        id: id.clone(),
        date: date.clone(),
        started_at: Some(now.clone()),
        memo: String::new(),
        confirmed_memo: String::new(),
        confirmed_sequence: 0,
        source_kind: SessionSource::Empty,
        routine_id: None,
        program_day_id: None,
        custom_program_day_id: None,
        finished_at: None,
        duration_seconds: None,
        total_weight_kg: None,
        exercise_count: None,
        set_count: None,
        personal_bests: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    let payload = SessionCreatePayload {
        id: id.clone(),
        date,
        started_at: session
            .started_at
            .clone()
            .expect("a created session has a start time"),
        memo: String::new(),
        source_kind: SessionSource::Empty,
        routine_id: None,
        program_day_id: None,
        custom_program_day_id: None,
    };
    let operation = Operation {
        operation_id: new_uuid()?,
        training_id: id,
        sequence: 0,
        kind: OperationKind::TrainingSessionCreate,
        parent_operation_id: None,
        payload: serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    };
    let storage = storage::current().await?;
    storage.save_session_create(&session, &operation).await?;
    Ok(session)
}

async fn save_draft(mut record: SessionRecord, memo: &str) -> Result<SessionRecord, StorageError> {
    record.memo = memo.to_owned();
    record.updated_at = now_iso();
    let started_at = record.started_at.clone().ok_or_else(|| {
        StorageError::InvalidOperation("the session has no start time".to_owned())
    })?;
    let payload = SessionUpdatePayload {
        date: record.date.clone(),
        started_at,
        memo: record.memo.clone(),
    };
    let operation = Operation {
        operation_id: new_uuid()?,
        training_id: record.id.clone(),
        sequence: 0,
        kind: OperationKind::TrainingSessionUpdate,
        parent_operation_id: None,
        payload: serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    };
    let storage = storage::current().await?;
    storage.save_session_update(&record, &operation).await?;
    Ok(record)
}

fn sync_status_text(status: SyncStatus, draft: &str, confirmed: &str) -> (&'static str, String) {
    let draft_unsaved = draft != confirmed;
    let unsaved = status.unsaved > 0 || draft_unsaved;
    let value = if status.failed > 0 {
        "failed"
    } else if unsaved {
        "unsaved"
    } else {
        "saved"
    };
    let mut parts = Vec::new();
    if unsaved {
        if status.unsaved > 0 {
            parts.push(format!("{}: {}", text(Key::SyncUnsaved), status.unsaved));
        } else {
            parts.push(text(Key::SyncUnsaved).to_owned());
        }
    }
    if status.failed > 0 {
        parts.push(format!("{}: {}", text(Key::SyncFailed), status.failed));
    }
    if parts.is_empty() {
        parts.push(text(Key::SyncSaved).to_owned());
    }
    (value, parts.join(" "))
}

fn operation_label(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::TrainingSessionCreate => text(Key::RecordOperationCreate),
        OperationKind::TrainingSessionUpdate => text(Key::RecordOperationUpdate),
    }
}

fn new_uuid() -> Result<String, StorageError> {
    let window = web_sys::window()
        .ok_or_else(|| StorageError::Unavailable("the window is missing".to_owned()))?;
    let crypto = window
        .crypto()
        .map_err(|error| StorageError::Unavailable(format!("{error:?}")))?;
    let mut bytes = [0u8; 16];
    crypto
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|error| StorageError::Unavailable(format!("{error:?}")))?;
    Ok(uuid_v4_from_bytes(bytes))
}

fn now_iso() -> String {
    let seconds = (js_sys::Date::now() / 1000.0) as i64;
    datetime::format_utc(seconds)
}
