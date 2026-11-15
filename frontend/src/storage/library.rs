//! 種目の変更の端末内の送信待ちの列。
//!
//! 独自種目の追加、編集、削除と、マスターの種目の編集、削除、初期化を、サーバーへ
//! 送る前に IndexedDB へ保存する。通信が復帰したら順に送り、受理した時点で消す。
//! 操作 ID はサーバーが同じ変更の再送を重複させないための鍵になる。

use serde::{Deserialize, Serialize};
use warrior_core::exercise::ExerciseInput;
use warrior_core::sync::OutboxState;
use web_sys::IdbTransactionMode;

use super::indexeddb::{self, from_js, to_js};
use super::{abort, outbox, Storage, StorageError, Store};

/// 種目の変更の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExerciseChangeKind {
    /// 独自種目の追加。
    Create,
    /// 独自種目またはマスターの種目の編集。
    Update,
    /// 独自種目またはマスターの種目の削除。
    Delete,
    /// マスターの種目の初期化。
    Reset,
}

impl ExerciseChangeKind {
    /// サーバーへ送る変更の名前。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Reset => "reset",
        }
    }
}

/// 送信待ちの種目の変更の 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExerciseOutboxEntry {
    /// 操作の UUID v4。
    pub operation_id: String,
    /// 端末が積んだ順序。
    pub sequence: u64,
    /// 変更の種別。
    pub kind: ExerciseChangeKind,
    /// 対象の種目の ID。追加では None。
    pub exercise_id: Option<String>,
    /// 追加と編集の内容。
    pub input: Option<ExerciseInput>,
    /// 送信の状態。
    pub state: OutboxState,
    /// 試行回数。
    pub attempts: u32,
}

impl Storage {
    /// 種目の変更を送信待ちの列へ積む。
    pub async fn enqueue_exercise_change(
        &self,
        kind: ExerciseChangeKind,
        exercise_id: Option<String>,
        input: Option<ExerciseInput>,
    ) -> Result<ExerciseOutboxEntry, StorageError> {
        validate_change(kind, exercise_id.as_deref(), input.as_ref())?;
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::ExerciseOutbox.name(), Store::Meta.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let store = transaction.store(Store::ExerciseOutbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let sequence = outbox::next_sequence(&meta, "exercise_library").await?;
        let entry = ExerciseOutboxEntry {
            operation_id: crate::ids::new_uuid()?,
            sequence,
            kind,
            exercise_id,
            input,
            state: OutboxState::Pending,
            attempts: 0,
        };
        if let Err(error) = indexeddb::add(&store, &to_js(&entry)?).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await?;
        Ok(entry)
    }

    /// 送信待ちの種目の変更を順序の昇順で返す。
    pub async fn exercise_changes(&self) -> Result<Vec<ExerciseOutboxEntry>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::ExerciseOutbox.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(Store::ExerciseOutbox.name())?;
        let mut entries = indexeddb::get_all(&store)
            .await?
            .iter()
            .map(from_js::<ExerciseOutboxEntry>)
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by(|left, right| {
            left.sequence
                .cmp(&right.sequence)
                .then_with(|| left.operation_id.cmp(&right.operation_id))
        });
        transaction.finish().await?;
        Ok(entries)
    }

    /// 種目の変更を送信中にする。
    pub async fn mark_exercise_change_sending(
        &self,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.transition_exercise_changes(|entries| {
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.operation_id == operation_id)
            {
                entry.state = OutboxState::Sending;
            }
        })
        .await
    }

    /// 種目の変更を成功として列から削除する。
    pub async fn mark_exercise_change_success(
        &self,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.transition_exercise_changes(|entries| {
            entries.retain(|entry| entry.operation_id != operation_id);
        })
        .await
    }

    /// 種目の変更を一時的な失敗として Pending に戻す。
    pub async fn mark_exercise_change_transient(
        &self,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.transition_exercise_changes(|entries| {
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.operation_id == operation_id)
            {
                entry.state = OutboxState::Pending;
                entry.attempts = entry.attempts.saturating_add(1);
            }
        })
        .await
    }

    /// 種目の変更を恒久的な失敗として Failed にする。
    pub async fn mark_exercise_change_permanent(
        &self,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        self.transition_exercise_changes(|entries| {
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.operation_id == operation_id)
            {
                entry.state = OutboxState::Failed;
            }
        })
        .await
    }

    async fn transition_exercise_changes(
        &self,
        apply: impl FnOnce(&mut Vec<ExerciseOutboxEntry>),
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::ExerciseOutbox.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let store = transaction.store(Store::ExerciseOutbox.name())?;
        let before = indexeddb::get_all(&store)
            .await?
            .iter()
            .map(from_js::<ExerciseOutboxEntry>)
            .collect::<Result<Vec<_>, _>>()?;
        let mut after = before.clone();
        apply(&mut after);
        for old in &before {
            if !after
                .iter()
                .any(|entry| entry.operation_id == old.operation_id)
            {
                if let Err(error) = indexeddb::delete_entry(
                    &store,
                    &wasm_bindgen::JsValue::from_str(&old.operation_id),
                )
                .await
                {
                    return abort(transaction, error).await;
                }
            }
        }
        for entry in &after {
            if !before.iter().any(|old| old == entry) {
                if let Err(error) = indexeddb::put(&store, &to_js(entry)?).await {
                    return abort(transaction, error).await;
                }
            }
        }
        transaction.finish().await
    }
}

fn validate_change(
    kind: ExerciseChangeKind,
    exercise_id: Option<&str>,
    input: Option<&ExerciseInput>,
) -> Result<(), StorageError> {
    match kind {
        ExerciseChangeKind::Create => {
            if exercise_id.is_some() || input.is_none() {
                return Err(StorageError::InvalidOperation(
                    "a create needs an input and no exercise id".to_owned(),
                ));
            }
        }
        ExerciseChangeKind::Update => {
            if exercise_id.is_none() || input.is_none() {
                return Err(StorageError::InvalidOperation(
                    "an update needs an exercise id and an input".to_owned(),
                ));
            }
        }
        ExerciseChangeKind::Delete | ExerciseChangeKind::Reset => {
            if exercise_id.is_none() || input.is_some() {
                return Err(StorageError::InvalidOperation(
                    "a delete or reset needs only an exercise id".to_owned(),
                ));
            }
        }
    }
    Ok(())
}
