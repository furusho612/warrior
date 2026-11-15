//! 記録の端末内の保存と、送信待ちの列。
//!
//! 記録の実体と送信待ちの操作を 1 つの IndexedDB のトランザクションで書き、
//! サーバーへの送信を入力の完了条件にしない (ADR-002)。
//! スキーマは版で管理し、移行に失敗してもデータベースを削除しない。

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use wasm_bindgen::JsValue;
use web_sys::{IdbDatabase, IdbObjectStore, IdbTransactionMode};

use warrior_core::sync::{
    discard as discard_transition, mark_permanent_failure as permanent_failure_transition,
    mark_sending as sending_transition, mark_success as success_transition,
    mark_transient_failure as transient_failure_transition, pending_session_memo, pending_update,
    recover_interrupted as recover_transition, retry as retry_transition, unresolved_create,
    Operation, OperationKind, OutboxEntry, OutboxState, SessionCreatePayload, SessionMemoPayload,
    SessionRecord, SessionUpdatePayload, TrainingExerciseMemoPayload,
};
use warrior_core::training_record::{TrainingExerciseRecord, TrainingRecord};

pub mod indexeddb;
pub mod outbox;

mod account;
mod body_weight;
mod discard;
mod exercise;
mod personal_best;
mod rest_timer;
mod set;
mod settings;
mod stopwatch;
mod superset;

pub use account::clear_after_deletion;
pub use indexeddb::request_persistence;
pub use set::StoredSet;
pub use settings::PendingUserSettings;
pub use stopwatch::StopwatchSide;

/// 既定のデータベースの名前。
pub const DATABASE_NAME: &str = "warrior";
/// スキーマの版。
pub const SCHEMA_VERSION: u32 = 3;

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

/// サーバーに保存済みの記録で端末の木を置き換える目的。
///
/// 引き継ぎは編集権を失った記録を消す。読み直しは閲覧のみの端末で行い、
/// 記録中は編集権を失った記録を残し、終了済みは消す。終了後はどの端末も編集できる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacePurpose {
    /// 編集権をこの端末へ引き継ぐ。
    Takeover,
    /// サーバーに保存済みの内容を読み直す。終了済みの記録は編集権を失った記録を消す。
    Reload,
}

/// 破棄した操作の結果。
#[derive(Debug, Clone, PartialEq)]
pub enum DiscardOutcome {
    /// 作成の破棄で、トレーニングと配下の行を削除した。
    SessionDeleted,
    /// 更新の破棄で、メモをサーバーが受理した値に戻した。
    MemoReverted(Box<SessionRecord>),
    /// 種目のメモの破棄で、メモをサーバーが受理した値に戻した。
    ExerciseMemoReverted(Box<TrainingExerciseRecord>),
    /// 種目の作成の破棄で、端末の種目と配下の行を削除した。
    ExerciseDeleted,
    /// セットの作成の破棄で、端末のセットと段を削除した。
    SetDeleted,
    /// 操作だけを削除した。
    OperationsRemoved,
}

/// 破棄した操作の復元の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// 復元した。
    Restored,
    /// より新しい送信待ちの操作があるため復元を見送った。
    Deferred,
}

/// 削除したトレーニングの端末内の後始末の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    /// 送信待ちの削除の操作を積んだ。
    Queued,
    /// サーバーに届いていないトレーニングだったため、端末の行だけを消した。
    Discarded,
}

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
    /// 身体情報の体重のキャッシュ。
    BodyWeight,
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
            Self::BodyWeight => "body_weight",
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

/// トレーニングのメモの保存の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionChange {
    /// 日付、開始時刻、メモを更新する。
    Update,
    /// メモだけを更新する。
    Memo,
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
        sessions.retain(|session| session.started_at.is_some() && session.finished_at.is_none());
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
        operation: &Operation,
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
            attempted: false,
        };

        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(session)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = indexeddb::add(&outbox_store, &indexeddb::to_js(&entry)?).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 日付を指定したトレーニングの追加とその操作を 1 つのトランザクションで保存する。
    ///
    /// 過去の記録は開始時刻を持たず、終了済みとして保存する。
    pub async fn save_session_backfill(
        &self,
        session: &SessionRecord,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingSessionBackfill
            || operation.training_id != session.id
            || session.started_at.is_some()
            || session.finished_at.is_none()
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not a past record addition".to_owned(),
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
        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(session)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) =
            exercise::enqueue_operation(&outbox_store, &meta, &entries, operation).await
        {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 端末のトレーニングと配下の行を削除し、削除の操作を送信待ちの列へ積む。
    ///
    /// 作成がまだサーバーに届いていないトレーニングは、端末の行と送信待ちの操作だけを消す。
    /// 削除の操作を送っても対象が無く、ほかの操作を失敗させるためである。
    pub async fn delete_session_tree(
        &self,
        training_id: &str,
        operation: &Operation,
    ) -> Result<DeleteOutcome, StorageError> {
        if operation.kind != OperationKind::TrainingSessionDelete
            || operation.training_id != training_id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not a training deletion".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingSessions.name(),
                Store::TrainingExercises.name(),
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let entries = match outbox::read(&outbox_store).await {
            Ok(entries) => entries,
            Err(error) => return abort(transaction, error).await,
        };
        let unsent_create = unresolved_create(&entries, training_id).is_some();
        if let Err(error) = exercise::delete_session_tree(&transaction, training_id).await {
            return abort(transaction, error).await;
        }
        if unsent_create {
            for entry in entries
                .iter()
                .filter(|entry| entry.operation.training_id == training_id)
            {
                if let Err(error) = indexeddb::delete_entry(
                    &outbox_store,
                    &JsValue::from_str(&entry.operation.operation_id),
                )
                .await
                {
                    return abort(transaction, error).await;
                }
            }
            transaction.finish().await?;
            return Ok(DeleteOutcome::Discarded);
        }
        let meta = transaction.store(Store::Meta.name())?;
        if let Err(error) =
            exercise::enqueue_operation(&outbox_store, &meta, &entries, operation).await
        {
            return abort(transaction, error).await;
        }
        transaction.finish().await?;
        Ok(DeleteOutcome::Queued)
    }

    /// 端末に保存した終了済みのトレーニングを、日付と終了時刻の新しい順で返す。
    pub async fn finished_sessions(&self) -> Result<Vec<SessionRecord>, StorageError> {
        let values = self.all_records(Store::TrainingSessions).await?;
        let mut sessions = values
            .iter()
            .map(|value| serde_json::from_value::<SessionRecord>(value.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| StorageError::Serialize(error.to_string()))?;
        sessions.retain(|session| session.finished_at.is_some());
        sessions.sort_by(|left, right| {
            right
                .date
                .cmp(&left.date)
                .then_with(|| right.finished_at.cmp(&left.finished_at))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(sessions)
    }

    /// トレーニングの更新とその操作を 1 つのトランザクションで保存する。
    pub async fn save_session_update(
        &self,
        session: &SessionRecord,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        self.save_session_change(session, operation, SessionChange::Update)
            .await
    }

    /// トレーニング全体のメモとその操作を 1 つのトランザクションで保存する。
    pub async fn save_session_memo(
        &self,
        session: &SessionRecord,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        self.save_session_change(session, operation, SessionChange::Memo)
            .await
    }

    async fn save_session_change(
        &self,
        session: &SessionRecord,
        operation: &Operation,
        change: SessionChange,
    ) -> Result<(), StorageError> {
        let expected = match change {
            SessionChange::Update => OperationKind::TrainingSessionUpdate,
            SessionChange::Memo => OperationKind::TrainingSessionMemo,
        };
        if operation.kind != expected || operation.training_id != session.id {
            return Err(StorageError::InvalidOperation(
                "the operation is not a session change".to_owned(),
            ));
        }
        if change == SessionChange::Memo {
            let payload: SessionMemoPayload = serde_json::from_value(operation.payload.clone())
                .map_err(|error| StorageError::Serialize(error.to_string()))?;
            if payload.training_id != session.id || payload.memo != session.memo {
                return Err(StorageError::InvalidOperation(
                    "the operation does not match the memo".to_owned(),
                ));
            }
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

        let existing_session = indexeddb::get(&sessions, &JsValue::from_str(&session.id)).await?;
        let Some(existing_session) = existing_session else {
            return Err(StorageError::InvalidOperation(
                "the session does not exist".to_owned(),
            ));
        };
        let existing_session: SessionRecord = indexeddb::from_js(&existing_session)?;
        let mut session_to_store = session.clone();
        session_to_store.confirmed_memo = existing_session.confirmed_memo;
        session_to_store.confirmed_sequence = existing_session.confirmed_sequence;

        let entries = outbox::read(&outbox_store).await?;
        let merge_target = match change {
            SessionChange::Update => pending_update(&entries, &operation.training_id),
            SessionChange::Memo => pending_session_memo(&entries, &operation.training_id),
        };
        let (operation_id, sequence, attempts, parent, state, reuse) = match merge_target {
            Some(entry) => (
                entry.operation.operation_id.clone(),
                entry.operation.sequence,
                entry.attempts,
                entry.operation.parent_operation_id.clone(),
                entry.state,
                true,
            ),
            None => {
                let parent = unresolved_create(&entries, &operation.training_id)
                    .map(|entry| entry.operation.operation_id.clone());
                let state = if parent.is_some() {
                    OutboxState::Blocked
                } else {
                    OutboxState::Pending
                };
                (
                    operation.operation_id.clone(),
                    outbox::next_sequence(&meta, &operation.training_id).await?,
                    0,
                    parent,
                    state,
                    false,
                )
            }
        };
        let mut stored_operation = operation.clone();
        stored_operation.operation_id = operation_id;
        stored_operation.sequence = sequence;
        stored_operation.parent_operation_id = parent;
        let entry = OutboxEntry {
            operation: stored_operation,
            state,
            attempts,
            attempted: false,
        };

        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(&session_to_store)?).await {
            return abort(transaction, error).await;
        }
        let write = if reuse {
            indexeddb::put(&outbox_store, &indexeddb::to_js(&entry)?).await
        } else {
            indexeddb::add(&outbox_store, &indexeddb::to_js(&entry)?).await
        };
        if let Err(error) = write {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 終了したトレーニングのサマリーを端末内へ保存する。
    ///
    /// サマリーはサーバーが計算して保存した値であり、送信待ちの列には入れない。
    pub async fn save_session_summary(&self, session: &SessionRecord) -> Result<(), StorageError> {
        if session.finished_at.is_none() {
            return Err(StorageError::InvalidOperation(
                "the session is not finished".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::TrainingSessions.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let sessions = transaction.store(Store::TrainingSessions.name())?;
        let Some(existing) = indexeddb::get(&sessions, &JsValue::from_str(&session.id)).await?
        else {
            return Err(StorageError::InvalidOperation(
                "the session does not exist".to_owned(),
            ));
        };
        let existing: SessionRecord = indexeddb::from_js(&existing)?;
        let mut session_to_store = session.clone();
        session_to_store.confirmed_memo = existing.confirmed_memo;
        session_to_store.confirmed_sequence = existing.confirmed_sequence;
        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(&session_to_store)?).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 任意の記録用ストアに 1 件を書く。
    ///
    /// ブラウザテストの準備に使う。型付きの API があるストアには型付きの API を使う。
    pub async fn put_record(
        &self,
        store_name: Store,
        record: &serde_json::Value,
    ) -> Result<(), StorageError> {
        ensure_record_store(store_name)?;
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

    /// 任意の記録用ストアのすべての行を読む。
    ///
    /// ブラウザテストの準備に使う。型付きの API があるストアには型付きの API を使う。
    pub async fn all_records(
        &self,
        store_name: Store,
    ) -> Result<Vec<serde_json::Value>, StorageError> {
        ensure_record_store(store_name)?;
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
        match write_outbox_differences(&store, &entries, &updated).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 操作を送信中にする。
    pub async fn mark_sending(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            sending_transition(entries, operation_id);
        })
        .await
    }

    /// 操作を成功として列から削除し、サーバーが受理したメモと順序を記録する。
    pub async fn mark_success(&self, operation_id: &str) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Outbox.name(),
                Store::TrainingSessions.name(),
                Store::TrainingExercises.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        match mark_success_steps(&transaction, operation_id).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
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

    /// Failed の操作を Pending に戻す。確定済みの古い更新は戻さない。
    pub async fn retry(&self, operation_id: &str) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Outbox.name(), Store::TrainingSessions.name()],
            IdbTransactionMode::Readwrite,
        )?;
        match retry_steps(&transaction, operation_id).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 操作と子孫を破棄し、作成なら記録を、更新ならメモを戻す。
    pub async fn discard(&self, operation_id: &str) -> Result<DiscardOutcome, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Outbox.name(),
                Store::TrainingSessions.name(),
                Store::TrainingExercises.name(),
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        match discard::steps(&transaction, operation_id).await {
            Ok(outcome) => {
                transaction.finish().await?;
                Ok(outcome)
            }
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 送信中のまま残った操作を Pending に戻す。
    pub async fn recover_interrupted(&self) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            recover_transition(entries);
        })
        .await
    }

    /// 編集権を失ったトレーニングを記録し、以後の送信を止める。
    pub async fn mark_training_forbidden(&self, training_id: &str) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        match outbox::mark_forbidden(&meta, training_id).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 編集権を失ったトレーニングの ID を返す。
    pub async fn forbidden_trainings(&self) -> Result<Vec<String>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            IdbTransactionMode::Readonly,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        let trainings = outbox::forbidden_trainings(&meta).await?;
        transaction.finish().await?;
        Ok(trainings)
    }

    /// 身体情報の体重を端末内へキャッシュする。None のときはキャッシュを消す。
    pub async fn cache_body_weight(&self, body_weight_kg: Option<f64>) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::BodyWeight.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let store = transaction.store(Store::BodyWeight.name())?;
        match body_weight::save(&store, body_weight_kg).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// キャッシュした身体情報の体重を返す。
    pub async fn cached_body_weight(&self) -> Result<Option<f64>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::BodyWeight.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(Store::BodyWeight.name())?;
        let weight = body_weight::cached(&store).await?;
        transaction.finish().await?;
        Ok(weight)
    }

    /// サーバーに保存済みの記録で、端末の記録と送信待ちの列を置き換える。
    ///
    /// 引き継ぎと、閲覧のみの端末の読み直しで使う。トレーニングの木の古い行と、
    /// そのトレーニングの送信待ちの操作を削除してから、応答の行だけを書く (ADR-002)。
    /// 編集権を失った記録を消すのは引き継ぎと、終了済みの記録の読み直しである。
    pub async fn replace_session_tree(
        &self,
        record: &TrainingRecord,
        purpose: ReplacePurpose,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingSessions.name(),
                Store::TrainingExercises.name(),
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        match replace_session_tree_steps(&transaction, record, purpose).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
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

async fn abort<T>(
    transaction: indexeddb::Transaction,
    error: StorageError,
) -> Result<T, StorageError> {
    transaction.abort();
    let _ = transaction.finish().await;
    Err(error)
}

/// 端末に保存するスーパーセットの行。組に含める種目は `superset_members` に置く。
#[derive(serde::Serialize)]
pub(super) struct StoredSuperset<'a> {
    id: &'a str,
    training_id: &'a str,
    position: i64,
    created_at: &'a str,
    updated_at: &'a str,
}

async fn replace_session_tree_steps(
    transaction: &indexeddb::Transaction,
    record: &TrainingRecord,
    purpose: ReplacePurpose,
) -> Result<(), StorageError> {
    let training_id = record.session.id.as_str();
    let outbox_store = transaction.store(Store::Outbox.name())?;
    let entries = outbox::read(&outbox_store).await?;
    for entry in entries
        .iter()
        .filter(|entry| entry.operation.training_id == training_id)
    {
        indexeddb::delete_entry(
            &outbox_store,
            &JsValue::from_str(&entry.operation.operation_id),
        )
        .await?;
    }

    exercise::delete_session_tree(transaction, training_id).await?;
    let sessions = transaction.store(Store::TrainingSessions.name())?;
    let mut session = record.session.clone();
    session.confirmed_memo = session.memo.clone();
    session.confirmed_sequence = 0;
    indexeddb::put(&sessions, &indexeddb::to_js(&session)?).await?;

    let exercises = record
        .exercises
        .iter()
        .map(|exercise| {
            let mut stored = exercise.clone();
            stored.confirmed_memo = stored.memo.clone();
            stored
        })
        .collect::<Vec<_>>();
    put_rows(transaction, Store::TrainingExercises, &exercises).await?;
    put_rows(transaction, Store::Sets, &record.sets).await?;
    put_rows(transaction, Store::SetSegments, &record.segments).await?;
    let supersets = record
        .supersets
        .iter()
        .map(|superset| StoredSuperset {
            id: &superset.id,
            training_id: &superset.training_id,
            position: superset.position,
            created_at: &superset.created_at,
            updated_at: &superset.updated_at,
        })
        .collect::<Vec<_>>();
    put_rows(transaction, Store::Supersets, &supersets).await?;
    let members = record
        .supersets
        .iter()
        .flat_map(|superset| superset.members.iter())
        .collect::<Vec<_>>();
    put_rows(transaction, Store::SupersetMembers, &members).await?;

    if purpose == ReplacePurpose::Takeover || record.session.finished_at.is_some() {
        let meta = transaction.store(Store::Meta.name())?;
        outbox::clear_forbidden(&meta, training_id).await?;
    }
    Ok(())
}

pub(super) async fn put_rows<T: serde::Serialize>(
    transaction: &indexeddb::Transaction,
    store: Store,
    rows: &[T],
) -> Result<(), StorageError> {
    let object_store = transaction.store(store.name())?;
    for row in rows {
        indexeddb::put(&object_store, &indexeddb::to_js(row)?).await?;
    }
    Ok(())
}

fn ensure_record_store(store: Store) -> Result<(), StorageError> {
    if matches!(store, Store::Outbox | Store::Meta) {
        return Err(StorageError::InvalidOperation(
            "the store is not a record store".to_owned(),
        ));
    }
    Ok(())
}

fn session_payload_memo(operation: &Operation) -> Result<String, StorageError> {
    match operation.kind {
        OperationKind::TrainingSessionCreate => {
            serde_json::from_value::<SessionCreatePayload>(operation.payload.clone())
                .map(|payload| payload.memo)
                .map_err(|error| StorageError::Serialize(error.to_string()))
        }
        OperationKind::TrainingSessionUpdate => {
            serde_json::from_value::<SessionUpdatePayload>(operation.payload.clone())
                .map(|payload| payload.memo)
                .map_err(|error| StorageError::Serialize(error.to_string()))
        }
        OperationKind::TrainingSessionMemo => {
            serde_json::from_value::<SessionMemoPayload>(operation.payload.clone())
                .map(|payload| payload.memo)
                .map_err(|error| StorageError::Serialize(error.to_string()))
        }
        OperationKind::TrainingExerciseCreate
        | OperationKind::TrainingExerciseDelete
        | OperationKind::TrainingExerciseReorder
        | OperationKind::TrainingExerciseMemo
        | OperationKind::SetCreate
        | OperationKind::SetUpdate
        | OperationKind::SetDelete
        | OperationKind::SetReorder
        | OperationKind::SetComplete
        | OperationKind::SupersetCreate
        | OperationKind::SupersetDelete
        | OperationKind::SupersetReorder
        | OperationKind::TrainingSessionBackfill
        | OperationKind::TrainingSessionDelete => Err(StorageError::InvalidOperation(
            "the operation has no session memo".to_owned(),
        )),
    }
}

fn exercise_payload_memo(operation: &Operation) -> Result<(String, String), StorageError> {
    if operation.kind != OperationKind::TrainingExerciseMemo {
        return Err(StorageError::InvalidOperation(
            "the operation has no exercise memo".to_owned(),
        ));
    }
    serde_json::from_value::<TrainingExerciseMemoPayload>(operation.payload.clone())
        .map(|payload| (payload.training_exercise_id, payload.memo))
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

async fn retry_steps(
    transaction: &indexeddb::Transaction,
    operation_id: &str,
) -> Result<(), StorageError> {
    let outbox_store = transaction.store(Store::Outbox.name())?;
    let sessions = transaction.store(Store::TrainingSessions.name())?;
    let entries = outbox::read(&outbox_store).await?;
    let Some(target) = entries
        .iter()
        .find(|entry| entry.operation.operation_id == operation_id)
        .cloned()
    else {
        return Ok(());
    };
    let target_is_session_memo = matches!(
        target.operation.kind,
        OperationKind::TrainingSessionUpdate | OperationKind::TrainingSessionMemo
    );
    let key = JsValue::from_str(&target.operation.training_id);
    if target_is_session_memo {
        if let Some(value) = indexeddb::get(&sessions, &key).await? {
            let session: SessionRecord = indexeddb::from_js(&value)?;
            if target.operation.sequence <= session.confirmed_sequence {
                return Ok(());
            }
        }
    }

    let mut updated = entries.clone();
    if retry_transition(&mut updated, operation_id) {
        write_outbox_differences(&outbox_store, &entries, &updated).await?;
    }
    Ok(())
}

async fn mark_success_steps(
    transaction: &indexeddb::Transaction,
    operation_id: &str,
) -> Result<(), StorageError> {
    let outbox_store = transaction.store(Store::Outbox.name())?;
    let sessions = transaction.store(Store::TrainingSessions.name())?;
    let exercises = transaction.store(Store::TrainingExercises.name())?;
    let entries = outbox::read(&outbox_store).await?;
    let Some(target) = entries
        .iter()
        .find(|entry| entry.operation.operation_id == operation_id)
        .cloned()
    else {
        return Ok(());
    };

    let mut updated = entries.clone();
    success_transition(&mut updated, operation_id);
    discard_superseded_memos(&mut updated, &entries, &target.operation);
    write_outbox_differences(&outbox_store, &entries, &updated).await?;

    if matches!(target.operation.kind, OperationKind::TrainingExerciseMemo) {
        let (training_exercise_id, memo) = exercise_payload_memo(&target.operation)?;
        let key = JsValue::from_str(&training_exercise_id);
        if let Some(value) = indexeddb::get(&exercises, &key).await? {
            let mut exercise: TrainingExerciseRecord = indexeddb::from_js(&value)?;
            exercise.confirmed_memo = memo;
            indexeddb::put(&exercises, &indexeddb::to_js(&exercise)?).await?;
        }
        return Ok(());
    }

    if !matches!(
        target.operation.kind,
        OperationKind::TrainingSessionCreate
            | OperationKind::TrainingSessionUpdate
            | OperationKind::TrainingSessionMemo
    ) {
        return Ok(());
    }
    let memo = session_payload_memo(&target.operation)?;
    let key = JsValue::from_str(&target.operation.training_id);
    if let Some(value) = indexeddb::get(&sessions, &key).await? {
        let mut session: SessionRecord = indexeddb::from_js(&value)?;
        session.confirmed_memo = memo;
        session.confirmed_sequence = target.operation.sequence;
        indexeddb::put(&sessions, &indexeddb::to_js(&session)?).await?;
    }
    Ok(())
}

/// 成功したメモの保存より古い、同じ対象のメモの操作を列から外す。
///
/// 古い操作を残すと、再試行で新しいメモを古い値へ戻してしまう。
fn discard_superseded_memos(
    updated: &mut Vec<OutboxEntry>,
    entries: &[OutboxEntry],
    operation: &Operation,
) {
    let obsolete = entries
        .iter()
        .filter(|entry| {
            entry.operation.training_id == operation.training_id
                && entry.operation.sequence < operation.sequence
                && matches!(
                    entry.state,
                    OutboxState::Pending | OutboxState::Blocked | OutboxState::Failed
                )
                && match operation.kind {
                    OperationKind::TrainingSessionUpdate | OperationKind::TrainingSessionMemo => {
                        matches!(
                            entry.operation.kind,
                            OperationKind::TrainingSessionUpdate
                                | OperationKind::TrainingSessionMemo
                        )
                    }
                    OperationKind::TrainingExerciseMemo => {
                        entry.operation.kind == OperationKind::TrainingExerciseMemo
                            && entry.operation.payload.get("training_exercise_id")
                                == operation.payload.get("training_exercise_id")
                    }
                    _ => false,
                }
        })
        .map(|entry| entry.operation.operation_id.clone())
        .collect::<Vec<_>>();
    for obsolete_id in obsolete {
        discard_transition(updated, &obsolete_id);
    }
}

async fn write_outbox_differences(
    store: &IdbObjectStore,
    before: &[OutboxEntry],
    after: &[OutboxEntry],
) -> Result<(), StorageError> {
    for old in before {
        if !after
            .iter()
            .any(|entry| entry.operation.operation_id == old.operation.operation_id)
        {
            indexeddb::delete_entry(store, &JsValue::from_str(&old.operation.operation_id)).await?;
        }
    }
    for entry in after {
        if !before.iter().any(|old| old == entry) {
            indexeddb::put(store, &indexeddb::to_js(entry)?).await?;
        }
    }
    Ok(())
}
