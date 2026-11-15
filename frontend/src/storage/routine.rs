//! ルーティンの端末内の保存と、送信待ちの列への操作の追加。
//!
//! ルーティンの行と操作を 1 つの IndexedDB のトランザクションで書き、保存を
//! サーバーへの送信の完了条件にしない (ADR-002)。一覧の読み直しはサーバーの
//! 内容で端末の行を置き換える。

use serde::Deserialize;
use warrior_core::routine::Routine;
use warrior_core::sync::{Operation, OperationKind};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::exercise::enqueue_operation;
use super::{abort, indexeddb, outbox, Storage, StorageError, Store};

impl Storage {
    /// 端末に保存したルーティンを並び順で返す。
    pub async fn routines(&self) -> Result<Vec<Routine>, StorageError> {
        let mut routines = self
            .all_records(Store::Routines)
            .await?
            .iter()
            .map(|value| {
                serde_json::from_value::<Routine>(value.clone())
                    .map_err(|error| StorageError::Serialize(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        routines.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(routines)
    }

    /// 端末に保存した 1 つのルーティンを返す。無ければ None。
    pub async fn routine(&self, routine_id: &str) -> Result<Option<Routine>, StorageError> {
        Ok(self
            .routines()
            .await?
            .into_iter()
            .find(|routine| routine.id == routine_id))
    }

    /// ルーティンの作成とその操作を 1 つのトランザクションで保存する。
    pub async fn save_routine_create(
        &self,
        routine: &Routine,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::RoutineCreate || operation.training_id != routine.id {
            return Err(StorageError::InvalidOperation(
                "the operation is not a routine creation".to_owned(),
            ));
        }
        self.save_routine_row(routine, operation).await
    }

    /// ルーティンの更新とその操作を 1 つのトランザクションで保存する。
    pub async fn save_routine_update(
        &self,
        routine: &Routine,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::RoutineUpdate || operation.training_id != routine.id {
            return Err(StorageError::InvalidOperation(
                "the operation is not a routine update".to_owned(),
            ));
        }
        self.save_routine_row(routine, operation).await
    }

    /// ルーティンの複製とその操作を 1 つのトランザクションで保存する。
    pub async fn save_routine_copy(
        &self,
        routine: &Routine,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::RoutineCopy || operation.training_id != routine.id {
            return Err(StorageError::InvalidOperation(
                "the operation is not a routine copy".to_owned(),
            ));
        }
        self.save_routine_row(routine, operation).await
    }

    /// ルーティンの削除とその操作を 1 つのトランザクションで保存する。
    pub async fn delete_routine(
        &self,
        routine_id: &str,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::RoutineDelete || operation.training_id != routine_id {
            return Err(StorageError::InvalidOperation(
                "the operation is not a routine deletion".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Routines.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let routines = transaction.store(Store::Routines.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = match outbox::read(&outbox_store).await {
            Ok(entries) => entries,
            Err(error) => return abort(transaction, error).await,
        };
        if let Err(error) = indexeddb::delete_entry(&routines, &JsValue::from_str(routine_id)).await
        {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// サーバーの一覧で端末のルーティンを置き換える。
    ///
    /// 送信待ちの操作があるルーティンは、サーバーの内容で上書きしない。
    pub async fn replace_routines(&self, routines: &[Routine]) -> Result<(), StorageError> {
        let pending = self
            .outbox_entries()
            .await?
            .into_iter()
            .filter(|entry| entry.operation.kind.is_routine())
            .map(|entry| entry.operation.training_id)
            .collect::<Vec<_>>();
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Routines.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let store = transaction.store(Store::Routines.name())?;
        let stored = match read_ids(&store).await {
            Ok(ids) => ids,
            Err(error) => return abort(transaction, error).await,
        };
        for id in stored {
            if pending.contains(&id) {
                continue;
            }
            if let Err(error) = indexeddb::delete_entry(&store, &JsValue::from_str(&id)).await {
                return abort(transaction, error).await;
            }
        }
        for routine in routines {
            if pending.contains(&routine.id) {
                continue;
            }
            if let Err(error) = indexeddb::put(&store, &indexeddb::to_js(routine)?).await {
                return abort(transaction, error).await;
            }
        }
        transaction.finish().await
    }

    async fn save_routine_row(
        &self,
        routine: &Routine,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Routines.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let routines = transaction.store(Store::Routines.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = match outbox::read(&outbox_store).await {
            Ok(entries) => entries,
            Err(error) => return abort(transaction, error).await,
        };
        if let Err(error) = indexeddb::put(&routines, &indexeddb::to_js(routine)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }
}

async fn read_ids(store: &IdbObjectStore) -> Result<Vec<String>, StorageError> {
    let values = indexeddb::get_all(store).await?;
    let mut ids = Vec::new();
    for value in values {
        let record = indexeddb::from_js::<RoutineId>(&value)?;
        ids.push(record.id);
    }
    Ok(ids)
}

#[derive(Debug, Deserialize)]
struct RoutineId {
    id: String,
}
