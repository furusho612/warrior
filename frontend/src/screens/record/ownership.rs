//! 閲覧のみの端末の表示、編集権の引き継ぎ、サーバーに保存済みの記録の読み直し。
//!
//! 403 で編集権を失った端末は閲覧のみを表示し、利用者が明示的に操作したときだけ
//! 引き継ぐ (REQ-NFR-015)。引き継ぎと読み直しは、応答の記録で IndexedDB の
//! セッション木と送信待ちの列を置き換え、画面を読み込み直す (ADR-002、ADR-003)。
//! 読み直しは閲覧のみの端末だけが使う。

use dioxus::prelude::*;
use warrior_core::training_record::TrainingRecord;

use super::{OwnershipSignals, RecordSignals};
use crate::routes::navigate;
use crate::storage::{self, ReplacePurpose};
use crate::text::{text, Key};
use crate::training_session as training_api;
use crate::training_session::TrainingApiError;

/// 閲覧のみの端末に、編集権の引き継ぎとサーバーからの読み直しを出す。
///
/// 編集権を持つ端末は、どちらの操作も必要としないため表示しない。
#[component]
pub(super) fn RecordOwnership(
    training_id: String,
    signals: RecordSignals,
    ownership: OwnershipSignals,
) -> Element {
    let mut ownership = ownership;
    if !(signals.view_only)() {
        return rsx! {};
    }
    let busy = (ownership.busy)();
    let confirming = (ownership.confirming)();
    let take_id = training_id.clone();
    let reload_id = training_id.clone();
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-view-only": "true",
            p { class: "text-[13px] text-text-sub", "{text(Key::RecordViewOnly)}" }
            button {
                class: "min-h-11 border border-primary px-3 text-[13px] font-bold text-primary",
                "data-takeover": "true",
                disabled: busy,
                onclick: move |_| ownership.confirming.set(true),
                "{text(Key::RecordTakeover)}"
            }
            if confirming {
                p { class: "text-[13px] text-primary", "{text(Key::RecordTakeoverConfirm)}" }
                button {
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-takeover-confirm": "true",
                    disabled: busy,
                    onclick: move |_| {
                        spawn(takeover_record(take_id.clone(), signals, ownership));
                    },
                    "{text(Key::RecordTakeoverAccept)}"
                }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    disabled: busy,
                    onclick: move |_| ownership.confirming.set(false),
                    "{text(Key::RecordTakeoverCancel)}"
                }
            }
            button {
                class: "min-h-11 border border-line px-3 text-[13px]",
                "data-reload-server": "true",
                disabled: busy,
                onclick: move |_| {
                    spawn(reload_from_server(reload_id.clone(), signals, ownership));
                },
                "{text(Key::RecordReloadFromServer)}"
            }
            if let Some(message) = (ownership.error)() {
                p { class: "text-[13px] text-primary", "{message}" }
            }
        }
    }
}

/// 編集権をこの端末へ引き継ぎ、応答の記録で端末の記録を置き換える。
///
/// 引き継ぎの後は画面を読み込み直し、記録画面を新しい記録で開き直す。
async fn takeover_record(
    training_id: String,
    signals: RecordSignals,
    mut ownership: OwnershipSignals,
) {
    ownership.busy.set(true);
    ownership.error.set(None);
    let result = training_api::takeover(&training_id).await;
    ownership.busy.set(false);
    let record = match result {
        Ok(record) => record,
        Err(TrainingApiError::Conflict) => {
            ownership
                .error
                .set(Some(text(Key::RecordTakeoverFinished).to_owned()));
            return;
        }
        Err(_) => {
            ownership
                .error
                .set(Some(text(Key::RecordTakeoverFailed).to_owned()));
            return;
        }
    };
    store_and_reload(&training_id, &record, signals, ownership).await;
}

/// サーバーに保存済みの記録を読み直し、端末の記録を置き換える。
async fn reload_from_server(
    training_id: String,
    signals: RecordSignals,
    mut ownership: OwnershipSignals,
) {
    ownership.busy.set(true);
    ownership.error.set(None);
    let reloaded = reload_record(&training_id, signals).await;
    ownership.busy.set(false);
    if !reloaded {
        ownership
            .error
            .set(Some(text(Key::RecordReloadFailed).to_owned()));
        return;
    }
    navigate(&format!("/record/{training_id}"));
}

/// サーバーに保存済みの記録を端末へ置き換え、画面の記録を反映する。
pub(super) async fn reload_record(training_id: &str, signals: RecordSignals) -> bool {
    let Ok(record) = training_api::record(training_id).await else {
        return false;
    };
    if replace_record(&record, ReplacePurpose::Reload)
        .await
        .is_err()
    {
        return false;
    }
    apply_record(&record, signals);
    true
}

async fn store_and_reload(
    training_id: &str,
    record: &TrainingRecord,
    mut signals: RecordSignals,
    mut ownership: OwnershipSignals,
) {
    if replace_record(record, ReplacePurpose::Takeover)
        .await
        .is_err()
    {
        ownership
            .error
            .set(Some(text(Key::RecordTakeoverFailed).to_owned()));
        return;
    }
    apply_record(record, signals);
    signals.view_only.set(false);
    ownership.confirming.set(false);
    navigate(&format!("/record/{training_id}"));
}

/// 応答の記録を画面の signal へ反映する。
fn apply_record(record: &TrainingRecord, mut signals: RecordSignals) {
    signals.memo.set(record.session.memo.clone());
    signals.persisted_memo.set(record.session.memo.clone());
    signals.memo_revision.set((signals.memo_revision)() + 1);
    signals.session.set(Some(record.session.clone()));
    signals.tree_revision.set((signals.tree_revision)() + 1);
}

/// 端末の記録を、サーバーに保存済みの記録で置き換える。
async fn replace_record(
    record: &TrainingRecord,
    purpose: ReplacePurpose,
) -> Result<(), storage::StorageError> {
    let storage = storage::current().await?;
    storage.replace_session_tree(record, purpose).await
}
