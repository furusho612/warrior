//! 記録の端末内の保存と、送信待ちの列。
//!
//! 記録の実体と送信待ちの操作を 1 つの IndexedDB のトランザクションで書き、
//! サーバーへの送信を入力の完了条件にしない (ADR-002)。
//! スキーマは版で管理し、移行に失敗してもデータベースを削除しない。

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use wasm_bindgen::JsValue;
use web_sys::{IdbDatabase, IdbTransactionMode};

use warrior_core::sync::{
    discard as discard_transition, mark_permanent_failure as permanent_failure_transition,
    mark_sending as sending_transition, mark_success as success_transition,
    mark_transient_failure as transient_failure_transition, retry as retry_transition,
    OperationKind, OutboxEntry, OutboxState, SessionRecord,
};

pub mod indexeddb;
pub mod outbox;

pub use indexeddb::request_persistence;

/// 既定のデータベースの名前。
pub const DATABASE_NAME: &str = "warrior";
/// スキーマの版。
pub const SCHEMA_VERSION: u32 = 1;

/// 端末内の保存の誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// ブラウザが IndexedDB を提供しない。
    Unavailable(String),
    /// データベースを開けない。
    Open(String),
    /// リクエストが失敗した。
    Request(String),
    /// トランザクションが失敗した。
    Transaction(String),
    /// JSON との変換に失敗した。
    Serialize(String),
    /// 保存する値が操作の形に合わない。
    InvalidOperation(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Unavailable(message)
            | Self::Open(message)
            | Self::Request(message)
            | Self::Transaction(message)
            | Self::Serialize(message)
            | Self::InvalidOperation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for StorageError {}

/// IndexedDB のオブジェクトストア。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    /// トレーニング。
    TrainingSessions,
    /// トレーニングに追加した種目。
    TrainingExercises,
    /// セット。
    Sets,
    /// ドロップセットの段。
    SetSegments,
    /// スーパーセットの組。
    Supersets,
    /// スーパーセットに含める種目。
    SupersetMembers,
    /// 送信待ちの操作。
    Outbox,
    /// 同期のメタ。
    Meta,
}

impl Store {
    /// オブジェクトストアの名前。
    pub fn name(self) -> &'static str {
        match self {
            Self::TrainingSessions => "training_sessions",
            Self::TrainingExercises => "training_exercises",
            Self::Sets => "sets",
            Self::SetSegments => "set_segments",
            Self::Supersets => "supersets",
            Self::SupersetMembers => "superset_members",
            Self::Outbox => "outbox",
            Self::Meta => "meta",
        }
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Storage>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

struct Inner {
    name: String,
    version: u32,
    database: RefCell<Option<IdbDatabase>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(database) = self.database.borrow_mut().take() {
            database.close();
        }
    }
}

/// 端末内の保存の入口。
#[derive(Clone)]
pub struct Storage {
    inner: Rc<Inner>,
}

impl Storage {
    /// 既定の名前と版で開く。
    pub async fn open() -> Result<Self, StorageError> {
        Self::open_with(DATABASE_NAME, SCHEMA_VERSION).await
    }

    /// 名前と版を指定して開く。ブラウザのテストは一意な名前を使う。
    pub async fn open_with(name: &str, version: u32) -> Result<Self, StorageError> {
        let database = indexeddb::open(name, version).await?;
        Ok(Self {
            inner: Rc::new(Inner {
                name: name.to_owned(),
                version,
                database: RefCell::new(Some(database)),
            }),
        })
    }

    async fn database(&self) -> Result<IdbDatabase, StorageError> {
        if let Some(database) = self.inner.database.borrow().clone() {
            return Ok(database);
        }
        let database = indexeddb::open(&self.inner.name, self.inner.version).await?;
        *self.inner.database.borrow_mut() = Some(database.clone());
        Ok(database)
    }

    /// 1 つのトレーニングを読む。
    pub async fn session(&self, id: &str) -> Result<Option<SessionRecord>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::TrainingSessions.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(Store::TrainingSessions.name())?;
        let value = indexeddb::get(&store, &JsValue::from_str(id)).await?;
        let session = value
            .as_ref()
            .map(indexeddb::from_js::<SessionRecord>)
            .transpose()?;
        transaction.finish().await?;
        Ok(session)
    }

    /// 記録中のトレーニングを返す。開始時刻が最も新しいものを選ぶ。
    pub async fn active_session(&self) -> Result<Option<SessionRecord>, StorageError> {
        let sessions = self.all_records(Store::TrainingSessions).await?;
        let mut sessions = sessions
            .iter()
            .map(|value| serde_json::from_value::<SessionRecord>(value.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::Serialize(error.to_string()))?;
        sessions.retain(|session| session.started_at.is_some());
        sessions.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.updated_at.cmp(&right.updated_at))
        });
        Ok(sessions.pop())
    }

    /// 送信待ちの列を順序の昇順で返す。
    pub async fn outbox_entries(&self) -> Result<Vec<OutboxEntry>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Outbox.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(Store::Outbox.name())?;
        let entries = outbox::read(&store).await?;
        transaction.finish().await?;
        Ok(entries)
    }

    /// トレーニングの作成とその操作を 1 つのトランザクションで保存する。
    pub async fn save_session_create(
        &self,
        session: &SessionRecord,
        operation: &warrior_core::sync::Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingSessionCreate
            || operation.training_id != session.id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not a session creation".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingSessions.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let sessions = transaction.store(Store::TrainingSessions.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;

        let sequence = outbox::next_sequence(&meta, &operation.training_id).await?;
        let mut stored_operation = operation.clone();
        stored_operation.sequence = sequence;
        let entry = OutboxEntry {
            operation: stored_operation,
            state: OutboxState::Pending,
            attempts: 0,
        };

        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(session)?).await {
            transaction.abort();
            let _ = transaction.finish().await;
            return Err(error);
        }
        if let Err(error) = indexeddb::add(&outbox_store, &indexeddb::to_js(&entry)?).await {
            transaction.abort();
            let _ = transaction.finish().await;
            return Err(error);
        }
        transaction.finish().await
    }

    /// トレーニングの更新とその操作を 1 つのトランザクションで保存する。
    ///
    /// 同じトレーニングの Pending の更新があれば、その操作 ID と順序を再利用する。
    pub async fn save_session_update(
        &self,
        session: &SessionRecord,
        operation: &warrior_core::sync::Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingSessionUpdate
            || operation.training_id != session.id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not a session update".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingSessions.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let sessions = transaction.store(Store::TrainingSessions.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;

        let entries = outbox::read(&outbox_store).await?;
        let pending = outbox::pending_update(&entries, &operation.training_id);
        let (operation_id, sequence, attempts, reuse) = match pending {
            Some(entry) => (
                entry.operation.operation_id.clone(),
                entry.operation.sequence,
                entry.attempts,
                true,
            ),
            None => (
                operation.operation_id.clone(),
                outbox::next_sequence(&meta, &operation.training_id).await?,
                0,
                false,
            ),
        };
        let mut stored_operation = operation.clone();
        stored_operation.operation_id = operation_id;
        stored_operation.sequence = sequence;
        let entry = OutboxEntry {
            operation: stored_operation,
            state: OutboxState::Pending,
            attempts,
        };

        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(session)?).await {
            transaction.abort();
            let _ = transaction.finish().await;
            return Err(error);
        }
        let write = if reuse {
            indexeddb::put(&outbox_store, &indexeddb::to_js(&entry)?).await
        } else {
            indexeddb::add(&outbox_store, &indexeddb::to_js(&entry)?).await
        };
        if let Err(error) = write {
            transaction.abort();
            let _ = transaction.finish().await;
            return Err(error);
        }
        transaction.finish().await
    }

    /// 任意のストアに 1 件を書く。
    pub async fn put_record(
        &self,
        store_name: Store,
        record: &serde_json::Value,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[store_name.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let store = transaction.store(store_name.name())?;
        indexeddb::put(&store, &indexeddb::to_js(record)?).await?;
        transaction.finish().await
    }

    /// 任意のストアのすべての行を読む。
    pub async fn all_records(
        &self,
        store_name: Store,
    ) -> Result<Vec<serde_json::Value>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[store_name.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(store_name.name())?;
        let values = indexeddb::get_all(&store).await?;
        let records = values
            .iter()
            .map(indexeddb::from_js::<serde_json::Value>)
            .collect::<Result<Vec<_>, _>>()?;
        transaction.finish().await?;
        Ok(records)
    }

    /// 送信待ちの列に遷移を適用し、変わった行だけを書く。
    async fn transition_outbox(
        &self,
        apply: impl FnOnce(&mut Vec<OutboxEntry>),
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Outbox.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let store = transaction.store(Store::Outbox.name())?;
        let entries = outbox::read(&store).await?;
        let mut updated = entries.clone();
        apply(&mut updated);

        let removed = entries
            .iter()
            .filter(|old| {
                !updated
                    .iter()
                    .any(|entry| entry.operation.operation_id == old.operation.operation_id)
            })
            .map(|entry| entry.operation.operation_id.clone())
            .collect::<Vec<_>>();
        for operation_id in removed {
            indexeddb::delete_entry(&store, &JsValue::from_str(&operation_id)).await?;
        }
        for entry in &updated {
            if !entries.iter().any(|old| old == entry) {
                indexeddb::put(&store, &indexeddb::to_js(entry)?).await?;
            }
        }
        transaction.finish().await
    }

    /// 操作を送信中にする。
    pub async fn mark_sending(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            sending_transition(entries, operation_id);
        })
        .await
    }

    /// 操作を成功として列から削除し、子を unblock する。
    pub async fn mark_success(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            success_transition(entries, operation_id);
        })
        .await
    }

    /// 一時的な失敗として Pending に戻す。
    pub async fn mark_transient_failure(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            transient_failure_transition(entries, operation_id);
        })
        .await
    }

    /// 恒久的な失敗として Failed にする。
    pub async fn mark_permanent_failure(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            permanent_failure_transition(entries, operation_id);
        })
        .await
    }

    /// Failed の操作を Pending に戻す。
    pub async fn retry(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            retry_transition(entries, operation_id);
        })
        .await
    }

    /// 操作と子孫を破棄する。
    pub async fn discard(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            discard_transition(entries, operation_id);
        })
        .await
    }

    /// 送信中のまま残った操作を Pending に戻す。
    pub async fn recover_interrupted(&self) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            for entry in entries.iter_mut() {
                if entry.state == OutboxState::Sending {
                    entry.state = OutboxState::Pending;
                }
            }
        })
        .await
    }

    /// 認証が無いときに、Sending を試行回数を変えずに Pending へ戻す。
    pub async fn return_to_pending(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            if let Some(entry) = entries
                .iter_mut()
                .find(|entry| entry.operation.operation_id == operation_id)
            {
                if entry.state == OutboxState::Sending {
                    entry.state = OutboxState::Pending;
                }
            }
        })
        .await
    }

    /// データベースと送信待ちの列を削除し、同期の世代を進める。
    pub async fn clear_account(&self) -> Result<(), StorageError> {
        GENERATION.with(|generation| generation.set(generation.get().wrapping_add(1)));
        if let Some(database) = self.inner.database.borrow_mut().take() {
            database.close();
        }
        indexeddb::delete(&self.inner.name).await
    }
}

/// アプリが使う 1 つの保存の入口を返す。無ければ既定の名前で開く。
pub async fn current() -> Result<Storage, StorageError> {
    if let Some(storage) = CURRENT.with(|current| current.borrow().clone()) {
        return Ok(storage);
    }
    let storage = Storage::open().await?;
    CURRENT.with(|current| {
        *current.borrow_mut() = Some(storage.clone());
    });
    Ok(storage)
}

/// アプリが使う保存の入口でデータベースを削除する。
pub async fn clear_account() -> Result<(), StorageError> {
    let storage = current().await?;
    storage.clear_account().await
}

/// 同期の世代。データベースの削除で進む。
pub fn generation() -> u64 {
    GENERATION.with(Cell::get)
}
