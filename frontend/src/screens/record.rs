//! 記録の開始、記録中のメモの端末内への保存、種目の追加と前回の記録、トレーニングの終了。
//!
//! 種目の一覧と前回の記録、セットの入力、レストタイマー、セット詳細の枠を置く。
//! レストタイマーの中身は後続の画面が入れる。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use dioxus_router::{navigator, Link};
use warrior_core::datetime;
use warrior_core::sync::{
    failed_count, unsaved_count, Operation, OperationKind, OutboxEntry, OutboxState,
    SessionCreatePayload, SessionMemoPayload, SessionRecord, MEMO_MAX_CHARS,
};
use warrior_core::training_record::TrainingRecord;
use warrior_core::training_session::{PersonalBestItem, SessionSource, TrainingSessionSummary};

use crate::ids::{new_uuid, now_iso};
use crate::routes::{navigate, Route};
use crate::storage::{self, ReplacePurpose, RestoreOutcome, StorageError};
use crate::sync::{self, SyncStatus};
use crate::text::{text, Key};
use crate::training_session as training_api;
use crate::training_session::TrainingApiError;

mod errors;
mod exercise_view;
mod exercises;
mod failed_operations;
mod memo_save;
mod one_rep_max;
mod ownership;
mod personal_bests;
mod set_drawer;
mod set_events;
mod set_inputs;
mod set_table;
mod sets;
mod stopwatch;
mod superset_panel;

use exercises::ExerciseSection;
use failed_operations::FailedOperations;
use memo_save::{run_save_loop, MemoSaveState};
use ownership::RecordOwnership;
use personal_bests::RecordPersonalBests;
use set_events::{SetCompleted, SetCompletedEvents};

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
    view_only: bool,
    personal_bests: Vec<PersonalBestItem>,
}

/// 画面が読む同期の状態の signal。
#[derive(Clone, Copy, PartialEq)]
struct StateSignals {
    status: Signal<SyncStatus>,
    failed: Signal<Vec<OutboxEntry>>,
    confirmed: Signal<String>,
    personal_bests: Signal<Vec<PersonalBestItem>>,
}

/// 記録画面の入力と保存が共有する signal。
#[derive(Clone, Copy, PartialEq)]
struct RecordSignals {
    session: Signal<Option<SessionRecord>>,
    memo: Signal<String>,
    persisted_memo: Signal<String>,
    memo_revision: Signal<u64>,
    tree_revision: Signal<u64>,
    exercise_revision: Signal<u64>,
    save_error: Signal<Option<String>>,
    view_only: Signal<bool>,
}

/// 閲覧のみの端末の引き継ぎと読み直しの signal。
#[derive(Clone, Copy, PartialEq)]
struct OwnershipSignals {
    confirming: Signal<bool>,
    busy: Signal<bool>,
    error: Signal<Option<String>>,
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
    let tree_revision = use_signal(|| 0u64);
    let exercise_revision = use_signal(|| 0u64);
    let load_error = use_signal(|| Option::<String>::None);
    let save_error = use_signal(|| Option::<String>::None);
    let view_only = use_signal(|| false);
    let profile_prompt = use_signal(|| false);
    let profile_checked = use_signal(|| false);
    let status = use_signal(SyncStatus::default);
    let failed = use_signal(Vec::<OutboxEntry>::new);
    let confirmed = use_signal(String::new);
    let confirming = use_signal(|| false);
    let ownership_busy = use_signal(|| false);
    let ownership_error = use_signal(|| Option::<String>::None);
    let completed_events = use_signal(Vec::<SetCompleted>::new);
    let personal_bests = use_signal(Vec::<PersonalBestItem>::new);
    let completed_event_signal = SetCompletedEvents(completed_events);
    use_context_provider(|| completed_event_signal);
    let signals = RecordSignals {
        session,
        memo,
        persisted_memo,
        memo_revision,
        tree_revision,
        exercise_revision,
        save_error,
        view_only,
    };
    let ownership = OwnershipSignals {
        confirming,
        busy: ownership_busy,
        error: ownership_error,
    };
    let state_signals = StateSignals {
        status,
        failed,
        confirmed,
        personal_bests,
    };
    let route_id = training_id.clone();

    use_effect(move || {
        let id = route_id.clone();
        spawn(async move {
            load(id, signals, state_signals, load_error).await;
        });
    });

    use_effect(move || {
        let mut profile_prompt = profile_prompt;
        let mut profile_checked = profile_checked;
        spawn(async move {
            if let Ok(profile) = crate::profile::fetch_and_cache().await {
                profile_prompt.set(profile.is_none_or(|profile| profile.body_profile().is_none()));
                profile_checked.set(true);
            }
        });
    });

    let poll_id = training_id.clone();
    use_future(move || {
        let id = poll_id.clone();
        async move {
            loop {
                sleep_ms(STATE_POLL_INTERVAL_MS).await;
                let state = read_record_state(&id).await;
                apply_state(state_signals, signals, state);
            }
        }
    });

    let persisted = (signals.persisted_memo)();
    let failed_entries = (state_signals.failed)();
    let completed_event_count = completed_event_signal.count();
    let rest_timer_event_count = completed_event_signal.rest_timer_count();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-route": "/record/:training_id",
            "data-session-id": training_id,
            "data-persisted-memo": "{persisted}",
            "data-set-events": "{completed_event_count}",
            "data-rest-timer-events": "{rest_timer_event_count}",
            "data-profile-checked": if profile_checked() { "true" } else { "false" },
            RecordHeader { signals, state_signals }
            RecordOwnership {
                training_id: training_id.clone(),
                signals,
                ownership,
            }
            MemoEditor { signals, state_signals }
            RecordFinish { signals }
            RecordPersonalBests { items: (state_signals.personal_bests)() }
            if profile_prompt() {
                ProfilePrompt {}
            }
            ExerciseSection {
                training_id: training_id.clone(),
                signals,
            }
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
fn ProfilePrompt() -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-profile-prompt": "true",
            p { class: "text-[13px] leading-5", "{text(Key::ProfilePrompt)}" }
            Link {
                to: Route::MyPage {},
                class: "flex min-h-11 items-center border border-line px-3 text-[13px] text-primary",
                "{text(Key::ProfilePromptAction)}"
            }
        }
    }
}

#[component]
fn RecordFinish(signals: RecordSignals) -> Element {
    let finishing = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let session = (signals.session)();
    let finished = session
        .as_ref()
        .is_some_and(|session| session.finished_at.is_some());
    let enabled = session.is_some() && !(signals.view_only)() && !finished;
    let busy = finishing();
    let summary_id = session.map(|session| session.id).unwrap_or_default();

    rsx! {
        if finished {
            p {
                class: "text-[13px] text-text-sub",
                "data-record-finished": "true",
                "{text(Key::RecordEditingFinished)}"
            }
            Link {
                to: Route::Summary { training_id: summary_id },
                class: "flex min-h-11 items-center border border-line px-3 text-[13px] text-primary",
                "data-record-summary": "true",
                "{text(Key::RecordSummary)}"
            }
        } else {
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
        }
        if let Some(message) = error() {
            p { class: "text-[13px] text-primary", "{message}" }
            if !(signals.view_only)() {
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
    match finish_blocker(&record.id).await {
        Ok(Some(FinishBlocker::Pending)) => {
            finishing.set(false);
            error.set(Some(text(Key::RecordPendingOperations).to_owned()));
            return;
        }
        Ok(Some(FinishBlocker::Failed)) => {
            finishing.set(false);
            error.set(Some(text(Key::RecordFailedOperationsRemain).to_owned()));
            return;
        }
        Ok(None) => {}
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
        Err(TrainingApiError::Forbidden) => {
            finish_forbidden(&record.id, signals, error).await;
            return;
        }
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

/// 終了の 403 で、この端末を閲覧のみへ切り替える。
///
/// 同期の 403 と同じく編集権を失った記録を残し、以後の送信を止める。
async fn finish_forbidden(
    training_id: &str,
    mut signals: RecordSignals,
    mut error: Signal<Option<String>>,
) {
    if sync::mark_training_forbidden(training_id).await {
        signals.view_only.set(true);
        error.set(Some(text(Key::RecordViewOnly).to_owned()));
        return;
    }
    error.set(Some(text(Key::RecordFinishFailed).to_owned()));
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

#[component]
fn MemoEditor(signals: RecordSignals, state_signals: StateSignals) -> Element {
    let saving = use_signal(|| false);
    let initial = (signals.memo)();
    let state = use_hook(|| MemoSaveState {
        saving,
        in_flight: Rc::new(Cell::new(false)),
        dirty: Rc::new(Cell::new(false)),
        draft: Rc::new(RefCell::new(initial)),
    });
    let input_state = state.clone();
    let retry_state = state.clone();
    let reset_state = state.clone();

    use_effect(move || {
        let _ = (signals.memo_revision)();
        *reset_state.draft.borrow_mut() = (*signals.memo.peek()).clone();
    });

    let revision = (signals.memo_revision)();
    let memo = (signals.memo)();
    let disabled = (signals.session)().is_none() || (signals.view_only)();
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
                    if !input_state.request() {
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
                        retry_state.dirty.set(true);
                        retry_state.in_flight.set(true);
                        spawn(save_loop(signals, retry_state.clone()));
                    },
                    "{text(Key::RecordRetry)}"
                }
            }
        }
    }
}

async fn restore_discarded_exercise(
    record: &TrainingRecord,
    training_exercise_id: Option<&str>,
    kind: OperationKind,
    after_sequence: u64,
) -> Result<RestoreOutcome, ()> {
    let storage = storage::current().await.map_err(|_| ())?;
    match kind {
        OperationKind::TrainingExerciseDelete => {
            let Some(training_exercise_id) = training_exercise_id else {
                return Err(());
            };
            storage
                .restore_training_exercise_with_outcome(
                    record,
                    training_exercise_id,
                    after_sequence,
                )
                .await
                .map_err(|_| ())
        }
        OperationKind::TrainingExerciseReorder => storage
            .restore_training_exercise_positions_with_outcome(record, after_sequence)
            .await
            .map_err(|_| ()),
        _ => Err(()),
    }
}

async fn restore_discarded_set(
    record: &TrainingRecord,
    set_id: Option<&str>,
    training_exercise_id: Option<&str>,
    kind: OperationKind,
    after_sequence: u64,
) -> Result<RestoreOutcome, ()> {
    let storage = storage::current().await.map_err(|_| ())?;
    match kind {
        OperationKind::SetUpdate | OperationKind::SetComplete | OperationKind::SetDelete => {
            let Some(set_id) = set_id else {
                return Err(());
            };
            storage
                .restore_set_with_outcome(record, set_id, after_sequence)
                .await
                .map_err(|_| ())
        }
        OperationKind::SetReorder => storage
            .restore_set_positions_with_outcome(record, training_exercise_id, after_sequence)
            .await
            .map_err(|_| ()),
        _ => Err(()),
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
            signals.tree_revision.set((signals.tree_revision)() + 1);
            error.set(None);
        }
        Ok(None) => {
            load_from_server(&training_id, &storage, signals, state_signals, error).await;
            return;
        }
        Err(_) => error.set(Some(text(Key::RecordLoadFailed).to_owned())),
    }
    refresh_state(&training_id, state_signals).await;
}

/// 端末に記録がないとき、サーバーに保存済みの記録で端末の記録を作る。
///
/// 同じアカウントでログインした別の端末は、この経路で記録を閲覧する (REQ-NFR-006)。
async fn load_from_server(
    training_id: &str,
    storage: &storage::Storage,
    mut signals: RecordSignals,
    state_signals: StateSignals,
    mut error: Signal<Option<String>>,
) {
    let record = match training_api::record(training_id).await {
        Ok(record) => record,
        Err(_) => {
            error.set(Some(text(Key::RecordLoadFailed).to_owned()));
            return;
        }
    };
    if storage
        .replace_session_tree(&record, ReplacePurpose::Reload)
        .await
        .is_err()
    {
        error.set(Some(text(Key::RecordLoadFailed).to_owned()));
        return;
    }
    signals.memo.set(record.session.memo.clone());
    signals.persisted_memo.set(record.session.memo.clone());
    signals.memo_revision.set((signals.memo_revision)() + 1);
    signals.session.set(Some(record.session));
    signals.tree_revision.set((signals.tree_revision)() + 1);
    error.set(None);
    refresh_state(training_id, state_signals).await;
}

async fn refresh_state(training_id: &str, mut state_signals: StateSignals) {
    sync::flush().await;
    let state = read_record_state(training_id).await;
    state_signals.status.set(state.status);
    state_signals.failed.set(state.failed);
    state_signals.confirmed.set(state.confirmed);
}

fn apply_state(mut state_signals: StateSignals, mut signals: RecordSignals, state: RecordState) {
    if *state_signals.status.peek() != state.status {
        state_signals.status.set(state.status);
    }
    if *state_signals.failed.peek() != state.failed {
        state_signals.failed.set(state.failed);
    }
    if *state_signals.confirmed.peek() != state.confirmed {
        state_signals.confirmed.set(state.confirmed);
    }
    if *state_signals.personal_bests.peek() != state.personal_bests {
        state_signals.personal_bests.set(state.personal_bests);
    }
    if *signals.view_only.peek() != state.view_only {
        signals.view_only.set(state.view_only);
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
    let session = storage.session(training_id).await.ok().flatten();
    let confirmed = session
        .as_ref()
        .map(|session| session.confirmed_memo.clone())
        .unwrap_or_default();
    let personal_bests = session
        .map(|session| session.personal_bests)
        .unwrap_or_default();
    let view_only = storage
        .forbidden_trainings()
        .await
        .map(|trainings| trainings.iter().any(|id| id == training_id))
        .unwrap_or(false);
    RecordState {
        status: SyncStatus {
            unsaved: unsaved_count(&entries),
            failed: failed_count(&entries),
        },
        confirmed,
        personal_bests,
        failed: entries
            .into_iter()
            .filter(|entry| entry.state == OutboxState::Failed)
            .collect(),
        view_only,
    }
}

async fn save_loop(signals: RecordSignals, state: MemoSaveState) {
    let save_state = state.clone();
    run_save_loop(state, move |draft_text| {
        let mut signals = signals;
        let save_state = save_state.clone();
        async move {
            let Some(record) = (signals.session)() else {
                return false;
            };
            match save_draft(record, &draft_text).await {
                Ok(updated) => {
                    if *save_state.draft.borrow() == draft_text {
                        signals.persisted_memo.set(draft_text.clone());
                    }
                    signals.memo.set(draft_text);
                    signals.session.set(Some(updated));
                    signals.save_error.set(None);
                    // 送信の往復を待たずに次の入力を保存する。画面の状態は 200ms の読み直しが更新する。
                    sync::flush_in_background();
                    true
                }
                Err(_) => {
                    signals
                        .save_error
                        .set(Some(text(Key::RecordSaveFailed).to_owned()));
                    false
                }
            }
        }
    })
    .await;
}

/// 終了を止める送信待ちの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinishBlocker {
    /// 送信中の操作が残っている。
    Pending,
    /// 恒久的に失敗した操作が残っている。
    Failed,
}

async fn finish_blocker(training_id: &str) -> Result<Option<FinishBlocker>, StorageError> {
    let storage = storage::current().await?;
    let entries = storage.outbox_entries().await?;
    Ok(blocker(&entries, training_id))
}

fn blocker(entries: &[OutboxEntry], training_id: &str) -> Option<FinishBlocker> {
    let mut blocker = None;
    for entry in entries
        .iter()
        .filter(|entry| entry.operation.training_id == training_id)
    {
        if entry.state == OutboxState::Failed {
            return Some(FinishBlocker::Failed);
        }
        blocker = Some(FinishBlocker::Pending);
    }
    blocker
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
    warrior_core::memo::validate(memo)
        .map_err(|_| StorageError::InvalidOperation("memo_too_long".to_owned()))?;
    record.memo = memo.to_owned();
    record.updated_at = now_iso();
    let payload = SessionMemoPayload {
        training_id: record.id.clone(),
        memo: record.memo.clone(),
    };
    let operation = Operation {
        operation_id: new_uuid()?,
        training_id: record.id.clone(),
        sequence: 0,
        kind: OperationKind::TrainingSessionMemo,
        parent_operation_id: None,
        payload: serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    };
    let storage = storage::current().await?;
    storage.save_session_memo(&record, &operation).await?;
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
        OperationKind::TrainingSessionMemo => text(Key::RecordOperationSessionMemo),
        OperationKind::TrainingSessionBackfill => text(Key::RecordOperationBackfill),
        OperationKind::TrainingSessionDelete => text(Key::RecordOperationDelete),
        OperationKind::TrainingExerciseCreate => text(Key::RecordOperationExerciseCreate),
        OperationKind::TrainingExerciseDelete => text(Key::RecordOperationExerciseDelete),
        OperationKind::TrainingExerciseReorder => text(Key::RecordOperationExerciseReorder),
        OperationKind::TrainingExerciseMemo => text(Key::RecordOperationExerciseMemo),
        OperationKind::SetCreate => text(Key::RecordOperationSetCreate),
        OperationKind::SetUpdate => text(Key::RecordOperationSetUpdate),
        OperationKind::SetDelete => text(Key::RecordOperationSetDelete),
        OperationKind::SetReorder => text(Key::RecordOperationSetReorder),
        OperationKind::SetComplete => text(Key::RecordOperationSetComplete),
        OperationKind::SupersetCreate => text(Key::RecordOperationSupersetCreate),
        OperationKind::SupersetDelete => text(Key::RecordOperationSupersetDelete),
        OperationKind::SupersetReorder => text(Key::RecordOperationSupersetReorder),
    }
}

#[cfg(test)]
mod tests {
    use warrior_core::sync::{Operation, OperationKind, OutboxEntry, OutboxState};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{blocker, FinishBlocker};

    #[wasm_bindgen_test]
    fn blocker_separates_pending_and_failed_operations() {
        assert_eq!(blocker(&[], "training-1"), None);
        assert_eq!(
            blocker(&[entry("training-1", OutboxState::Pending)], "training-1"),
            Some(FinishBlocker::Pending)
        );
        assert_eq!(
            blocker(&[entry("training-2", OutboxState::Failed)], "training-1"),
            None
        );
        assert_eq!(
            blocker(
                &[
                    entry("training-1", OutboxState::Pending),
                    entry("training-1", OutboxState::Failed),
                ],
                "training-1"
            ),
            Some(FinishBlocker::Failed),
            "a failed operation must take precedence over a pending operation"
        );
    }

    fn entry(training_id: &str, state: OutboxState) -> OutboxEntry {
        OutboxEntry {
            operation: Operation {
                operation_id: format!("operation-{training_id}-{state:?}"),
                training_id: training_id.to_owned(),
                sequence: 1,
                kind: OperationKind::TrainingSessionUpdate,
                parent_operation_id: None,
                payload: serde_json::json!({}),
            },
            state,
            attempts: 0,
            attempted: false,
        }
    }
}
