//! 記録の端末内の保存と、送信待ちの列。
//!
//! 記録の実体と送信待ちの操作を 1 つの IndexedDB のトランザクションで書き、
//! サーバーへの送信を入力の完了条件にしない (ADR-002)。
//! スキーマは版で管理し、移行に失敗してもデータベースを削除しない。

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use js_sys::Array;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use web_sys::{IdbDatabase, IdbObjectStore, IdbTransactionMode};

use warrior_core::sync::{
    discard as discard_transition, mark_permanent_failure as permanent_failure_transition,
    mark_sending as sending_transition, mark_success as success_transition,
    mark_transient_failure as transient_failure_transition, pending_update,
    recover_interrupted as recover_transition, retry as retry_transition, unresolved_create,
    Operation, OperationKind, OutboxEntry, OutboxState, SessionCreatePayload, SessionRecord,
    SessionUpdatePayload,
};

pub mod indexeddb;
pub mod outbox;

pub use indexeddb::request_persistence;

/// 既定のデータベースの名前。
pub const DATABASE_NAME: &str = "warrior";
/// スキーマの版。
pub const SCHEMA_VERSION: u32 = 2;

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

/// 破棄した操作の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscardOutcome {
    /// 作成の破棄で、トレーニングと配下の行を削除した。
    SessionDeleted,
    /// 更新の破棄で、メモをサーバーが受理した値に戻した。
    MemoReverted(SessionRecord),
    /// 操作だけを削除した。
    OperationsRemoved,
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

    /// トレーニングの更新とその操作を 1 つのトランザクションで保存する。
    pub async fn save_session_update(
        &self,
        session: &SessionRecord,
        operation: &Operation,
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

        let existing_session = indexeddb::get(&sessions, &JsValue::from_str(&session.id)).await?;
        let Some(existing_session) = existing_session else {
            return Err(StorageError::InvalidOperation(
                "the session does not exist".to_owned(),
            ));
        };
        let existing_session: SessionRecord = indexeddb::from_js(&existing_session)?;
        let mut session_to_store = session.clone();
        session_to_store.confirmed_memo = existing_session.confirmed_memo;

        let entries = outbox::read(&outbox_store).await?;
        let merge_target = pending_update(&entries, &operation.training_id);
        let (operation_id, sequence, attempts, parent, reuse) = match merge_target {
            Some(entry) => (
                entry.operation.operation_id.clone(),
                entry.operation.sequence,
                entry.attempts,
                entry.operation.parent_operation_id.clone(),
                true,
            ),
            None => (
                operation.operation_id.clone(),
                outbox::next_sequence(&meta, &operation.training_id).await?,
                0,
                unresolved_create(&entries, &operation.training_id)
                    .map(|entry| entry.operation.operation_id.clone()),
                false,
            ),
        };
        let mut stored_operation = operation.clone();
        stored_operation.operation_id = operation_id;
        stored_operation.sequence = sequence;
        stored_operation.parent_operation_id = parent.clone();
        let state = if reuse {
            OutboxState::Pending
        } else if parent.is_some() {
            OutboxState::Blocked
        } else {
            OutboxState::Pending
        };
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

    /// 任意の記録用ストアに 1 件を書く。
    ///
    /// 0021 のブラウザテスト用であり、0025 はセット用の型付き API を使う。
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
    /// 0021 のブラウザテスト用であり、0025 はセット用の型付き API を使う。
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
        write_outbox_differences(&store, &entries, &updated).await?;
        transaction.finish().await
    }

    /// 操作を送信中にする。
    pub async fn mark_sending(&self, operation_id: &str) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            sending_transition(entries, operation_id);
        })
        .await
    }

    /// 操作を成功として列から削除し、サーバーが受理したメモを記録する。
    pub async fn mark_success(&self, operation_id: &str) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Outbox.name(), Store::TrainingSessions.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let sessions = transaction.store(Store::TrainingSessions.name())?;
        let entries = outbox::read(&outbox_store).await?;
        let Some(target) = entries
            .iter()
            .find(|entry| entry.operation.operation_id == operation_id)
            .cloned()
        else {
            transaction.finish().await?;
            return Ok(());
        };
        let mut updated = entries.clone();
        success_transition(&mut updated, operation_id);
        write_outbox_differences(&outbox_store, &entries, &updated).await?;

        if let Some(memo) = payload_memo(&target.operation) {
            let key = JsValue::from_str(&target.operation.training_id);
            if let Some(value) = indexeddb::get(&sessions, &key).await? {
                let mut session: SessionRecord = indexeddb::from_js(&value)?;
                session.confirmed_memo = memo;
                indexeddb::put(&sessions, &indexeddb::to_js(&session)?).await?;
            }
        }
        transaction.finish().await
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
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let entries = outbox::read(&outbox_store).await?;
        let Some(target) = entries
            .iter()
            .find(|entry| entry.operation.operation_id == operation_id)
            .cloned()
        else {
            transaction.finish().await?;
            return Ok(DiscardOutcome::OperationsRemoved);
        };
        let mut updated = entries.clone();
        let removed = discard_transition(&mut updated, operation_id);
        for operation_id in removed {
            indexeddb::delete_entry(&outbox_store, &JsValue::from_str(&operation_id)).await?;
        }

        let outcome = if target.operation.kind == OperationKind::TrainingSessionCreate {
            delete_session_tree(&transaction, &target.operation.training_id).await?;
            DiscardOutcome::SessionDeleted
        } else {
            revert_session_memo(&transaction, &target.operation.training_id).await?
        };
        transaction.finish().await?;
        Ok(outcome)
    }

    /// 送信中のまま残った操作を Pending に戻す。
    pub async fn recover_interrupted(&self) -> Result<(), StorageError> {
        self.transition_outbox(|entries| {
            recover_transition(entries);
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

async fn abort<T>(
    transaction: indexeddb::Transaction,
    error: StorageError,
) -> Result<T, StorageError> {
    transaction.abort();
    let _ = transaction.finish().await;
    Err(error)
}

fn ensure_record_store(store: Store) -> Result<(), StorageError> {
    if matches!(store, Store::Outbox | Store::Meta) {
        return Err(StorageError::InvalidOperation(
            "the store is not a record store".to_owned(),
        ));
    }
    Ok(())
}

fn payload_memo(operation: &Operation) -> Option<String> {
    match operation.kind {
        OperationKind::TrainingSessionCreate => {
            serde_json::from_value::<SessionCreatePayload>(operation.payload.clone())
                .ok()
                .map(|payload| payload.memo)
        }
        OperationKind::TrainingSessionUpdate => {
            serde_json::from_value::<SessionUpdatePayload>(operation.payload.clone())
                .ok()
                .map(|payload| payload.memo)
        }
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

#[derive(Deserialize)]
struct ExerciseRow {
    id: String,
    training_id: String,
}

#[derive(Deserialize)]
struct SetRow {
    id: String,
    training_exercise_id: String,
}

#[derive(Deserialize)]
struct SegmentRow {
    id: String,
    set_id: String,
}

#[derive(Deserialize)]
struct SupersetRow {
    id: String,
    training_id: String,
}

#[derive(Deserialize)]
struct MemberRow {
    superset_id: String,
    training_exercise_id: String,
}

async fn read_rows<T: serde::de::DeserializeOwned>(
    store: &IdbObjectStore,
) -> Result<Vec<T>, StorageError> {
    indexeddb::get_all(store)
        .await?
        .iter()
        .map(indexeddb::from_js::<T>)
        .collect()
}

async fn delete_session_tree(
    transaction: &indexeddb::Transaction,
    training_id: &str,
) -> Result<(), StorageError> {
    let sessions = transaction.store(Store::TrainingSessions.name())?;
    indexeddb::delete_entry(&sessions, &JsValue::from_str(training_id)).await?;

    let exercises_store = transaction.store(Store::TrainingExercises.name())?;
    let exercises = read_rows::<ExerciseRow>(&exercises_store).await?;
    let exercise_ids = exercises
        .iter()
        .filter(|row| row.training_id == training_id)
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    for id in &exercise_ids {
        indexeddb::delete_entry(&exercises_store, &JsValue::from_str(id)).await?;
    }

    let sets_store = transaction.store(Store::Sets.name())?;
    let sets = read_rows::<SetRow>(&sets_store).await?;
    let set_ids = sets
        .iter()
        .filter(|row| exercise_ids.contains(&row.training_exercise_id))
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    for id in &set_ids {
        indexeddb::delete_entry(&sets_store, &JsValue::from_str(id)).await?;
    }

    let segments_store = transaction.store(Store::SetSegments.name())?;
    let segments = read_rows::<SegmentRow>(&segments_store).await?;
    let segment_ids = segments
        .iter()
        .filter(|row| set_ids.contains(&row.set_id))
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    for id in &segment_ids {
        indexeddb::delete_entry(&segments_store, &JsValue::from_str(id)).await?;
    }

    let supersets_store = transaction.store(Store::Supersets.name())?;
    let supersets = read_rows::<SupersetRow>(&supersets_store).await?;
    let superset_ids = supersets
        .iter()
        .filter(|row| row.training_id == training_id)
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    for id in &superset_ids {
        indexeddb::delete_entry(&supersets_store, &JsValue::from_str(id)).await?;
    }

    let members_store = transaction.store(Store::SupersetMembers.name())?;
    let members = read_rows::<MemberRow>(&members_store).await?;
    for member in members.iter().filter(|row| {
        superset_ids.contains(&row.superset_id) || exercise_ids.contains(&row.training_exercise_id)
    }) {
        let key = Array::new();
        key.push(&JsValue::from_str(&member.superset_id));
        key.push(&JsValue::from_str(&member.training_exercise_id));
        indexeddb::delete_entry(&members_store, &key.into()).await?;
    }
    Ok(())
}

async fn revert_session_memo(
    transaction: &indexeddb::Transaction,
    training_id: &str,
) -> Result<DiscardOutcome, StorageError> {
    let sessions = transaction.store(Store::TrainingSessions.name())?;
    let Some(value) = indexeddb::get(&sessions, &JsValue::from_str(training_id)).await? else {
        return Ok(DiscardOutcome::OperationsRemoved);
    };
    let mut session: SessionRecord = indexeddb::from_js(&value)?;
    session.memo = session.confirmed_memo.clone();
    indexeddb::put(&sessions, &indexeddb::to_js(&session)?).await?;
    Ok(DiscardOutcome::MemoReverted(session))
}
