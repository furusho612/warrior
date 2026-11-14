//! 失敗した送信待ちの操作の表示と破棄。
//!
//! 破棄は操作の種類ごとに関数へ分ける。種目の削除と並べ替えは、対象の種目と
//! その種目を含む組をサーバーの記録から戻し、ほかの送信待ちの操作と入力には触れない。
//! 組の作成、削除、並べ替えは、対象の組だけをサーバーの記録から戻す。

use dioxus::prelude::*;
use warrior_core::sync::{OperationKind, OutboxEntry, SessionRecord};

use super::{
    operation_label, refresh_state, restore_discarded_exercise, restore_discarded_set,
    RecordSignals, StateSignals,
};
use crate::routes::navigate;
use crate::storage::{self, DiscardOutcome};
use crate::sync;
use crate::text::{text, Key};
use crate::training_session as training_api;

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
                                let operation_id = entry.operation.operation_id.clone();
                                let training_id = entry.operation.training_id.clone();
                                let operation_kind = entry.operation.kind;
                                let training_exercise_id = entry
                                    .operation
                                    .payload
                                    .get("training_exercise_id")
                                    .and_then(|value| value.as_str())
                                    .map(str::to_owned);
                                let set_id = entry
                                    .operation
                                    .payload
                                    .get("set_id")
                                    .and_then(|value| value.as_str())
                                    .map(str::to_owned);
                                let superset_id = entry
                                    .operation
                                    .payload
                                    .get("superset_id")
                                    .and_then(|value| value.as_str())
                                    .map(str::to_owned);
                                move |_| {
                                    spawn(discard(
                                        DiscardTarget {
                                            operation_id: operation_id.clone(),
                                            training_id: training_id.clone(),
                                            operation_kind,
                                            training_exercise_id: training_exercise_id.clone(),
                                            set_id: set_id.clone(),
                                            superset_id: superset_id.clone(),
                                        },
                                        signals,
                                        state_signals,
                                    ));
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

/// 破棄する操作の対象。
struct DiscardTarget {
    operation_id: String,
    training_id: String,
    operation_kind: OperationKind,
    training_exercise_id: Option<String>,
    set_id: Option<String>,
    superset_id: Option<String>,
}

/// 破棄の結果を、操作の種類ごとの処理へ振り分ける。
async fn discard(target: DiscardTarget, mut signals: RecordSignals, state_signals: StateSignals) {
    let DiscardTarget {
        operation_id,
        training_id,
        operation_kind,
        training_exercise_id,
        set_id,
        superset_id,
    } = target;
    match sync::discard_operation(&operation_id).await {
        Some(DiscardOutcome::SessionDeleted) => discard_session_deleted(),
        Some(DiscardOutcome::MemoReverted(updated)) => {
            discard_memo_reverted(updated, &training_id, signals, state_signals).await;
        }
        Some(DiscardOutcome::ExerciseDeleted) => {
            discard_exercise_deleted(&training_id, signals, state_signals).await;
        }
        Some(DiscardOutcome::SetDeleted) => {
            discard_set_deleted(&training_id, signals, state_signals).await;
        }
        Some(DiscardOutcome::OperationsRemoved)
            if matches!(
                operation_kind,
                OperationKind::TrainingExerciseDelete | OperationKind::TrainingExerciseReorder
            ) =>
        {
            discard_exercise_operation(
                &training_id,
                training_exercise_id.as_deref(),
                operation_kind,
                &mut signals,
                state_signals,
            )
            .await;
        }
        Some(DiscardOutcome::OperationsRemoved)
            if matches!(
                operation_kind,
                OperationKind::SetUpdate
                    | OperationKind::SetDelete
                    | OperationKind::SetReorder
                    | OperationKind::SetComplete
            ) =>
        {
            discard_set_operation(
                &training_id,
                set_id.as_deref(),
                operation_kind,
                &mut signals,
                state_signals,
            )
            .await;
        }
        Some(DiscardOutcome::OperationsRemoved)
            if matches!(
                operation_kind,
                OperationKind::SupersetCreate
                    | OperationKind::SupersetDelete
                    | OperationKind::SupersetReorder
            ) =>
        {
            discard_superset_operation(
                &training_id,
                superset_id.as_deref(),
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
    set_id: Option<&str>,
    operation_kind: OperationKind,
    signals: &mut RecordSignals,
    state_signals: StateSignals,
) {
    if restore_discarded_set(training_id, set_id, operation_kind).await {
        signals.tree_revision.set((signals.tree_revision)() + 1);
    } else {
        signals
            .save_error
            .set(Some(text(Key::RecordSetRestoreFailed).to_owned()));
    }
    refresh_state(training_id, state_signals).await;
}

async fn discard_superset_operation(
    training_id: &str,
    superset_id: Option<&str>,
    signals: &mut RecordSignals,
    state_signals: StateSignals,
) {
    let restored = match superset_id {
        Some(superset_id) => {
            match (
                training_api::record(training_id).await,
                storage::current().await,
            ) {
                (Ok(record), Ok(storage)) => {
                    storage.restore_superset(&record, superset_id).await.is_ok()
                }
                _ => false,
            }
        }
        None => false,
    };
    if restored {
        signals
            .exercise_revision
            .set((signals.exercise_revision)() + 1);
    } else {
        signals
            .save_error
            .set(Some(text(Key::RecordSupersetRestoreFailed).to_owned()));
    }
    refresh_state(training_id, state_signals).await;
}

async fn discard_exercise_operation(
    training_id: &str,
    training_exercise_id: Option<&str>,
    operation_kind: OperationKind,
    signals: &mut RecordSignals,
    state_signals: StateSignals,
) {
    if restore_discarded_exercise(training_id, training_exercise_id, operation_kind).await {
        signals
            .exercise_revision
            .set((signals.exercise_revision)() + 1);
    } else {
        signals
            .save_error
            .set(Some(text(Key::RecordExerciseRestoreFailed).to_owned()));
    }
    refresh_state(training_id, state_signals).await;
}
