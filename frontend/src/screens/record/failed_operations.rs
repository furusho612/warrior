//! 失敗した送信待ちの操作の表示と破棄。
//!
//! 破棄は操作の種類ごとに関数へ分ける。種目の削除と並べ替えは、対象の種目だけを
//! サーバーの記録から戻し、ほかの送信待ちの操作と入力には触れない。

use dioxus::prelude::*;
use warrior_core::sync::{OperationKind, OutboxEntry, SessionRecord};
use warrior_core::training_record::TrainingRecord;

use super::{
    operation_label, refresh_state, restore_discarded_exercise, restore_discarded_set,
    RecordSignals, StateSignals,
};
use crate::routes::navigate;
use crate::storage::{DiscardOutcome, RestoreOutcome};
use crate::sync;
use crate::text::{text, Key};
use crate::training_session as training_api;

/// 破棄する操作の指定。
#[derive(Clone)]
struct DiscardRequest {
    operation_id: String,
    training_id: String,
    operation_kind: OperationKind,
    training_exercise_id: Option<String>,
    set_id: Option<String>,
    sequence: u64,
}

/// 失敗した操作の一覧と、再試行と破棄の操作。
#[component]
pub(super) fn FailedOperations(
    failed: Vec<OutboxEntry>,
    signals: RecordSignals,
    state_signals: StateSignals,
) -> Element {
    let signals = signals;

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
                                let request = DiscardRequest {
                                    operation_id: entry.operation.operation_id.clone(),
                                    training_id: entry.operation.training_id.clone(),
                                    operation_kind: entry.operation.kind,
                                    training_exercise_id: entry
                                        .operation
                                        .payload
                                        .get("training_exercise_id")
                                        .and_then(|value| value.as_str())
                                        .map(str::to_owned),
                                    set_id: entry
                                        .operation
                                        .payload
                                        .get("set_id")
                                        .and_then(|value| value.as_str())
                                        .map(str::to_owned),
                                    sequence: entry.operation.sequence,
                                };
                                move |_| {
                                    spawn(discard(request.clone(), signals, state_signals));
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

/// 破棄の結果を、操作の種類ごとの処理へ振り分ける。
async fn discard(request: DiscardRequest, mut signals: RecordSignals, state_signals: StateSignals) {
    let record = match restore_record(&request.training_id, request.operation_kind).await {
        Ok(record) => record,
        Err(key) => {
            signals.save_error.set(Some(text(key).to_owned()));
            return;
        }
    };
    match sync::discard_operation(&request.operation_id).await {
        Some(DiscardOutcome::SessionDeleted) => discard_session_deleted(),
        Some(DiscardOutcome::MemoReverted(updated)) => {
            discard_memo_reverted(updated, &request.training_id, signals, state_signals).await;
        }
        Some(DiscardOutcome::ExerciseDeleted) => {
            discard_exercise_deleted(&request.training_id, signals, state_signals).await;
        }
        Some(DiscardOutcome::SetDeleted) => {
            discard_set_deleted(&request.training_id, signals, state_signals).await;
        }
        Some(DiscardOutcome::OperationsRemoved)
            if matches!(
                request.operation_kind,
                OperationKind::TrainingExerciseDelete | OperationKind::TrainingExerciseReorder
            ) =>
        {
            discard_exercise_operation(
                &request.training_id,
                record.as_ref(),
                &request,
                &mut signals,
                state_signals,
            )
            .await;
        }
        Some(DiscardOutcome::OperationsRemoved)
            if matches!(
                request.operation_kind,
                OperationKind::SetUpdate
                    | OperationKind::SetDelete
                    | OperationKind::SetReorder
                    | OperationKind::SetComplete
            ) =>
        {
            discard_set_operation(
                &request.training_id,
                record.as_ref(),
                &request,
                &mut signals,
                state_signals,
            )
            .await;
        }
        _ => {}
    }
}

fn discard_session_deleted() {
    navigate("/record");
}

async fn discard_memo_reverted(
    updated: Box<SessionRecord>,
    training_id: &str,
    mut signals: RecordSignals,
    state_signals: StateSignals,
) {
    signals.memo.set(updated.memo.clone());
    signals.persisted_memo.set(updated.memo.clone());
    signals.memo_revision.set((signals.memo_revision)() + 1);
    signals.session.set(Some(*updated));
    refresh_state(training_id, state_signals).await;
}

async fn discard_exercise_deleted(
    training_id: &str,
    mut signals: RecordSignals,
    state_signals: StateSignals,
) {
    signals
        .exercise_revision
        .set((signals.exercise_revision)() + 1);
    refresh_state(training_id, state_signals).await;
}

async fn discard_set_deleted(
    training_id: &str,
    mut signals: RecordSignals,
    state_signals: StateSignals,
) {
    signals.tree_revision.set((signals.tree_revision)() + 1);
    refresh_state(training_id, state_signals).await;
}

async fn discard_set_operation(
    training_id: &str,
    record: Option<&TrainingRecord>,
    request: &DiscardRequest,
    signals: &mut RecordSignals,
    state_signals: StateSignals,
) {
    let Some(record) = record else {
        signals
            .save_error
            .set(Some(text(Key::RecordSetRestoreFailed).to_owned()));
        refresh_state(training_id, state_signals).await;
        return;
    };
    match restore_discarded_set(
        record,
        request.set_id.as_deref(),
        request.training_exercise_id.as_deref(),
        request.operation_kind,
        request.sequence,
    )
    .await
    {
        Ok(RestoreOutcome::Restored) => {
            signals.tree_revision.set((signals.tree_revision)() + 1);
        }
        Ok(RestoreOutcome::Deferred) => {
            signals
                .save_error
                .set(Some(text(Key::RecordRestoreDeferred).to_owned()));
        }
        Err(()) => {
            signals
                .save_error
                .set(Some(text(Key::RecordSetRestoreFailed).to_owned()));
        }
    }
    refresh_state(training_id, state_signals).await;
}

async fn discard_exercise_operation(
    training_id: &str,
    record: Option<&TrainingRecord>,
    request: &DiscardRequest,
    signals: &mut RecordSignals,
    state_signals: StateSignals,
) {
    let Some(record) = record else {
        signals
            .save_error
            .set(Some(text(Key::RecordExerciseRestoreFailed).to_owned()));
        refresh_state(training_id, state_signals).await;
        return;
    };
    match restore_discarded_exercise(
        record,
        request.training_exercise_id.as_deref(),
        request.operation_kind,
        request.sequence,
    )
    .await
    {
        Ok(RestoreOutcome::Restored) => {
            signals
                .exercise_revision
                .set((signals.exercise_revision)() + 1);
        }
        Ok(RestoreOutcome::Deferred) => {
            signals
                .save_error
                .set(Some(text(Key::RecordRestoreDeferred).to_owned()));
        }
        Err(()) => {
            signals
                .save_error
                .set(Some(text(Key::RecordExerciseRestoreFailed).to_owned()));
        }
    }
    refresh_state(training_id, state_signals).await;
}

/// 復元に使うサーバーの記録を取る。復元が要らない操作は None を返す。
async fn restore_record(
    training_id: &str,
    operation_kind: OperationKind,
) -> Result<Option<TrainingRecord>, Key> {
    if !needs_server_record(operation_kind) {
        return Ok(None);
    }
    training_api::record(training_id)
        .await
        .map(Some)
        .map_err(|_| restore_error_key(operation_kind))
}

fn needs_server_record(operation_kind: OperationKind) -> bool {
    matches!(
        operation_kind,
        OperationKind::TrainingExerciseDelete
            | OperationKind::TrainingExerciseReorder
            | OperationKind::SetUpdate
            | OperationKind::SetDelete
            | OperationKind::SetReorder
            | OperationKind::SetComplete
    )
}

fn restore_error_key(operation_kind: OperationKind) -> Key {
    match operation_kind {
        OperationKind::TrainingExerciseDelete | OperationKind::TrainingExerciseReorder => {
            Key::RecordExerciseRestoreFailed
        }
        _ => Key::RecordSetRestoreFailed,
    }
}
