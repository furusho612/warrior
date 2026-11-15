//! 終了したトレーニングの一覧と、編集と削除の入口。
//!
//! 一覧は端末に保存した終了済みのトレーニングから作り、日付の新しい順に並べる。
//! 編集は記録画面を開いて行い、削除は端末内の行と送信待ちの列を通す (ADR-002)。
//! 種目ごとの履歴とグラフは後続の画面が担う。

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::memo::display_text;
use warrior_core::sync::{
    validate_operation, Operation, OperationKind, SessionDeletePayload, SessionRecord,
};
use warrior_core::training_session::SessionSource;

use crate::ids::new_uuid;
use crate::routes::Route;
use crate::screens::summary::format_duration;
use crate::storage::{self, DeleteOutcome, StorageError};
use crate::sync;
use crate::text::{text, Key};

#[component]
pub fn HistoryScreen() -> Element {
    let sessions = use_signal(Vec::<SessionRecord>::new);
    let loaded = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let busy = use_signal(|| false);
    let mut confirming = use_signal(|| Option::<String>::None);

    use_effect(move || {
        spawn(load(sessions, loaded, error, busy));
    });

    let rows = sessions();
    let row_count = rows.len();
    let loaded_now = loaded();
    let error_now = error();
    let busy_now = busy();
    let confirming_now = confirming();

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-route": "/history",
            "data-history-count": "{row_count}",
            h1 { class: "text-lg font-bold", "{text(Key::HistoryTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::HistoryDescription)}" }
            if let Some(message) = error_now {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    disabled: busy_now,
                    onclick: move |_| {
                        spawn(load(sessions, loaded, error, busy));
                    },
                    "{text(Key::HistoryRetry)}"
                }
            } else if !loaded_now {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordLoading)}" }
            } else if rows.is_empty() {
                p {
                    class: "text-[13px] text-text-sub",
                    "data-history-empty": "true",
                    "{text(Key::HistoryEmpty)}"
                }
            } else {
                ul { class: "flex flex-col gap-1",
                    for session in rows {
                        HistoryRow {
                            key: "{session.id}",
                            session: session.clone(),
                            busy: busy_now,
                            confirming: confirming_now.as_deref() == Some(session.id.as_str()),
                            on_delete: EventHandler::new({
                                move |id: String| {
                                    spawn(delete_training(id, sessions, error, busy));
                                }
                            }),
                            on_confirm: EventHandler::new({
                                move |id: String| confirming.set(Some(id))
                            }),
                            on_cancel: EventHandler::new(move |_: ()| confirming.set(None)),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HistoryRow(
    session: SessionRecord,
    busy: bool,
    confirming: bool,
    on_delete: EventHandler<String>,
    on_confirm: EventHandler<String>,
    on_cancel: EventHandler<()>,
) -> Element {
    let training_id = session.id.clone();
    let date = session.date.clone();
    let duration = session.duration_seconds.unwrap_or(0);
    let set_count = session.set_count.unwrap_or(0);
    let memo = session.memo.clone();
    let delete_id = session.id.clone();
    let confirm_id = training_id.clone();

    rsx! {
        li {
            class: "flex flex-col gap-1 border-b border-line py-1",
            "data-history-item": "{training_id}",
            "data-history-date": "{date}",
            "data-history-duration-seconds": "{duration}",
            div { class: "flex min-h-11 items-center gap-2",
                span { class: "min-w-0 flex-1 truncate text-[13px] font-bold", "{date}" }
                span {
                    class: "shrink-0 text-[11px] text-text-sub",
                    "data-history-set-count": "{set_count}",
                    "{format_duration(duration)} / {set_count} {text(Key::HistorySetCount)}"
                }
                Link {
                    to: Route::RecordSession { training_id: training_id.clone() },
                    class: "flex min-h-11 shrink-0 items-center border border-line px-2 text-[11px] text-primary",
                    "data-history-open": "true",
                    "{text(Key::HistoryOpen)}"
                }
                button {
                    class: "min-h-11 shrink-0 border border-line px-2 text-[11px] text-primary",
                    "data-history-delete": "true",
                    disabled: busy,
                    onclick: move |_| on_confirm.call(confirm_id.clone()),
                    "{text(Key::HistoryDelete)}"
                }
            }
            if !memo.is_empty() {
                p { class: "truncate text-[11px] text-text-sub", "{display_text(&memo)}" }
            }
            if confirming {
                div {
                    class: "flex flex-col gap-1 border border-line bg-surface p-2",
                    "data-history-confirm": "{delete_id}",
                    p { class: "text-[11px]", "{text(Key::HistoryDeleteConfirm)}" }
                    div { class: "flex gap-2",
                        button {
                            class: "min-h-11 border border-primary bg-primary px-2 text-[11px] font-bold text-on-primary",
                            "data-history-delete-accept": "true",
                            disabled: busy,
                            onclick: move |_| on_delete.call(delete_id.clone()),
                            "{text(Key::HistoryDeleteAccept)}"
                        }
                        button {
                            class: "min-h-11 border border-line px-2 text-[11px]",
                            "data-history-delete-cancel": "true",
                            onclick: move |_| on_cancel.call(()),
                            "{text(Key::HistoryDeleteCancel)}"
                        }
                    }
                }
            }
        }
    }
}

async fn load(
    mut sessions: Signal<Vec<SessionRecord>>,
    mut loaded: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut busy: Signal<bool>,
) {
    busy.set(true);
    let result = match storage::current().await {
        Ok(storage) => storage.finished_sessions().await,
        Err(storage_error) => Err(storage_error),
    };
    busy.set(false);
    loaded.set(true);
    match result {
        Ok(rows) => {
            sessions.set(rows);
            error.set(None);
        }
        Err(_) => error.set(Some(text(Key::HistoryLoadFailed).to_owned())),
    }
}

async fn delete_training(
    training_id: String,
    mut sessions: Signal<Vec<SessionRecord>>,
    mut error: Signal<Option<String>>,
    mut busy: Signal<bool>,
) {
    busy.set(true);
    let result = remove_training(&training_id).await;
    busy.set(false);
    match result {
        Ok(outcome) => {
            error.set(None);
            sessions.write().retain(|session| session.id != training_id);
            if outcome == DeleteOutcome::Queued {
                sync::flush_in_background();
            }
        }
        Err(_) => error.set(Some(text(Key::HistoryDeleteFailed).to_owned())),
    }
}

/// トレーニングの削除の操作を組み立て、端末内の行と送信待ちの列を通す。
async fn remove_training(training_id: &str) -> Result<DeleteOutcome, StorageError> {
    let operation = delete_operation(training_id)?;
    let storage = storage::current().await?;
    storage.delete_session_tree(training_id, &operation).await
}

fn delete_operation(training_id: &str) -> Result<Operation, StorageError> {
    let payload = SessionDeletePayload {
        training_id: training_id.to_owned(),
    };
    pending_operation(
        training_id,
        OperationKind::TrainingSessionDelete,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// 日付を指定した追加の記録を組み立てる。開始時刻を持たず、指定した所要時間を持つ。
pub(crate) fn backfill_session(
    id: String,
    date: String,
    duration_seconds: i64,
    now: String,
) -> SessionRecord {
    SessionRecord {
        id,
        date,
        started_at: None,
        memo: String::new(),
        confirmed_memo: String::new(),
        confirmed_sequence: 0,
        source_kind: SessionSource::Empty,
        routine_id: None,
        program_day_id: None,
        custom_program_day_id: None,
        finished_at: Some(now.clone()),
        duration_seconds: Some(duration_seconds),
        total_weight_kg: Some(0.0),
        exercise_count: Some(0),
        set_count: Some(0),
        personal_bests: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    }
}

/// 日付を指定した追加の操作を組み立てる。
pub(crate) fn backfill_operation(session: &SessionRecord) -> Result<Operation, StorageError> {
    pending_operation(
        &session.id,
        OperationKind::TrainingSessionBackfill,
        serde_json::to_value(warrior_core::sync::SessionBackfillPayload {
            id: session.id.clone(),
            date: session.date.clone(),
            duration_seconds: session.duration_seconds.unwrap_or(0),
            memo: session.memo.clone(),
        })
        .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// 送信待ちの列へ積む前の操作を組み立てる。実際の順序は保存時に決める。
fn pending_operation(
    training_id: &str,
    kind: OperationKind,
    payload: serde_json::Value,
) -> Result<Operation, StorageError> {
    let operation = Operation {
        operation_id: new_uuid()?,
        training_id: training_id.to_owned(),
        // 保存時に実際の順序へ置き換える。検証は 1 以上を要求する。
        sequence: 1,
        kind,
        parent_operation_id: None,
        payload,
    };
    validate_operation(&operation)
        .map_err(|error| StorageError::InvalidOperation(error.code().to_owned()))?;
    Ok(operation)
}

#[cfg(test)]
mod tests {
    //! 履歴の削除の操作と、日付を指定した追加の記録の例示テスト。
    //!
    //! 対応する要求: `REQ-REC-006`。

    use warrior_core::sync::{validate_operation, OperationKind};

    use super::{backfill_operation, backfill_session, delete_operation};
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn a_backfilled_record_has_a_date_a_duration_and_no_start_time() {
        let session = backfill_session(
            "3f2504e0-4f89-41d3-9a0c-0305e82c6601".to_owned(),
            "2026-11-12".to_owned(),
            3_600,
            "2026-11-12T00:00:00Z".to_owned(),
        );
        assert_eq!(session.date, "2026-11-12");
        assert_eq!(session.started_at, None);
        assert_eq!(session.duration_seconds, Some(3_600));
        assert_eq!(session.finished_at.as_deref(), Some("2026-11-12T00:00:00Z"));
        let operation = backfill_operation(&session).expect("a valid operation");
        assert_eq!(operation.kind, OperationKind::TrainingSessionBackfill);
        validate_operation(&operation).expect("the operation is valid");
        let value = serde_json::to_value(&operation).expect("the operation is JSON");
        assert!(value["payload"].get("started_at").is_none());
        assert_eq!(value["payload"]["duration_seconds"], 3_600);
    }

    #[wasm_bindgen_test]
    fn a_delete_operation_is_valid() {
        let operation =
            delete_operation("3f2504e0-4f89-41d3-9a0c-0305e82c6601").expect("a valid operation");
        assert_eq!(operation.kind, OperationKind::TrainingSessionDelete);
        validate_operation(&operation).expect("the operation is valid");
    }
}
