//! スーパーセットの組と種目の参照の端末内の保存、読み出し、復元。
//!
//! 組は `supersets` に、組に含める種目は `superset_members` に分けて保存する。
//! 組の書き込みと送信待ちの列への追加を 1 つのトランザクションで行う (ADR-002)。

use serde::{Deserialize, Serialize};
use warrior_core::sync::{
    Operation, OperationKind, SupersetCreatePayload, SupersetDeletePayload, SupersetReorderPayload,
};
use warrior_core::training_record::{SupersetMemberRecord, SupersetRecord, TrainingRecord};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::exercise::{delete_member, enqueue_operation, read_rows};
use super::{abort, indexeddb, outbox, Storage, StorageError, Store};

/// 端末に保存する組の行。組に含める種目は `superset_members` に置く。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SupersetRow {
    pub(super) id: String,
    pub(super) training_id: String,
    pub(super) position: i64,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

impl SupersetRow {
    fn from_record(record: &SupersetRecord) -> Self {
        Self {
            id: record.id.clone(),
            training_id: record.training_id.clone(),
            position: record.position,
            created_at: record.created_at.clone(),
            updated_at: record.updated_at.clone(),
        }
    }
}

impl Storage {
    /// トレーニングに属する組を、位置の昇順で種目とともに返す。
    pub async fn training_supersets(
        &self,
        training_id: &str,
    ) -> Result<Vec<SupersetRecord>, StorageError> {
        let values = self.all_records(Store::Supersets).await?;
        let mut supersets = values
            .iter()
            .map(|value| {
                serde_json::from_value::<SupersetRow>(value.clone())
                    .map_err(|error| StorageError::Serialize(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        supersets.retain(|superset| superset.training_id == training_id);
        supersets.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        let members = self.all_records(Store::SupersetMembers).await?;
        let mut members = members
            .iter()
            .map(|value| {
                serde_json::from_value::<SupersetMemberRecord>(value.clone())
                    .map_err(|error| StorageError::Serialize(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        members.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.training_exercise_id.cmp(&right.training_exercise_id))
        });
        Ok(supersets
            .into_iter()
            .map(|superset| SupersetRecord {
                members: members
                    .iter()
                    .filter(|member| member.superset_id == superset.id)
                    .cloned()
                    .collect(),
                id: superset.id,
                training_id: superset.training_id,
                position: superset.position,
                created_at: superset.created_at,
                updated_at: superset.updated_at,
            })
            .collect())
    }

    /// 組の作成とその操作を 1 つのトランザクションで保存する。
    pub async fn save_superset_create(
        &self,
        superset: &SupersetRecord,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = create_payload(operation)?;
        if payload.superset_id != superset.id
            || payload.training_id != superset.training_id
            || payload.member_ids != member_ids(superset)
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not this superset creation".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let supersets = transaction.store(Store::Supersets.name())?;
        let members = transaction.store(Store::SupersetMembers.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if let Err(error) = put_superset(&supersets, superset).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = delete_members(&members, &superset.id).await {
            return abort(transaction, error).await;
        }
        for member in &superset.members {
            if let Err(error) = indexeddb::put(&members, &indexeddb::to_js(member)?).await {
                return abort(transaction, error).await;
            }
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 組の削除とその操作を 1 つのトランザクションで保存する。
    pub async fn save_superset_delete(
        &self,
        training_id: &str,
        superset_id: &str,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = delete_payload(operation)?;
        if payload.training_id != training_id || payload.superset_id != superset_id {
            return Err(StorageError::InvalidOperation(
                "the operation is not this superset deletion".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let supersets = transaction.store(Store::Supersets.name())?;
        let members = transaction.store(Store::SupersetMembers.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if let Err(error) = delete_members(&members, superset_id).await {
            return abort(transaction, error).await;
        }
        if let Err(error) =
            indexeddb::delete_entry(&supersets, &JsValue::from_str(superset_id)).await
        {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 組の種目の並べ替えとその操作を 1 つのトランザクションで保存する。
    pub async fn save_superset_reorder(
        &self,
        training_id: &str,
        superset_id: &str,
        member_id_list: &[String],
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = reorder_payload(operation)?;
        if payload.training_id != training_id
            || payload.superset_id != superset_id
            || payload.member_ids != member_id_list
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not this superset reorder".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let supersets = transaction.store(Store::Supersets.name())?;
        let members = transaction.store(Store::SupersetMembers.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        let Some(value) = indexeddb::get(&supersets, &JsValue::from_str(superset_id)).await? else {
            return abort(
                transaction,
                StorageError::InvalidOperation("the superset does not exist".to_owned()),
            )
            .await;
        };
        let mut row = indexeddb::from_js::<SupersetRow>(&value)?;
        row.updated_at = crate::ids::now_iso();
        if let Err(error) = indexeddb::put(&supersets, &indexeddb::to_js(&row)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = delete_members(&members, superset_id).await {
            return abort(transaction, error).await;
        }
        for (index, id) in member_id_list.iter().enumerate() {
            let member = SupersetMemberRecord {
                superset_id: superset_id.to_owned(),
                training_exercise_id: id.clone(),
                position: i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
            };
            if let Err(error) = indexeddb::put(&members, &indexeddb::to_js(&member)?).await {
                return abort(transaction, error).await;
            }
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// サーバーに保存済みの 1 つの組と種目を端末へ戻す。
    ///
    /// サーバーに無い組は端末からも削除する。送信待ちの列には触れない。
    pub async fn restore_superset(
        &self,
        record: &TrainingRecord,
        superset_id: &str,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Supersets.name(), Store::SupersetMembers.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let supersets = transaction.store(Store::Supersets.name())?;
        let members = transaction.store(Store::SupersetMembers.name())?;
        if let Err(error) = delete_members(&members, superset_id).await {
            return abort(transaction, error).await;
        }
        if let Err(error) =
            indexeddb::delete_entry(&supersets, &JsValue::from_str(superset_id)).await
        {
            return abort(transaction, error).await;
        }
        let Some(server) = record
            .supersets
            .iter()
            .find(|superset| superset.id == superset_id)
        else {
            return transaction.finish().await;
        };
        if let Err(error) = put_superset(&supersets, server).await {
            return abort(transaction, error).await;
        }
        for member in &server.members {
            if let Err(error) = indexeddb::put(&members, &indexeddb::to_js(member)?).await {
                return abort(transaction, error).await;
            }
        }
        transaction.finish().await
    }
}

/// トレーニングに属する組の行を返す。
pub(super) async fn read_superset_rows(
    store: &IdbObjectStore,
) -> Result<Vec<SupersetRow>, StorageError> {
    read_rows::<SupersetRow>(store).await
}

/// 組の種目の行を削除する。
pub(super) async fn delete_members(
    store: &IdbObjectStore,
    superset_id: &str,
) -> Result<(), StorageError> {
    let members = read_rows::<SupersetMemberRecord>(store).await?;
    for member in members
        .iter()
        .filter(|member| member.superset_id == superset_id)
    {
        delete_member(store, &member.superset_id, &member.training_exercise_id).await?;
    }
    Ok(())
}

async fn put_superset(
    store: &IdbObjectStore,
    superset: &SupersetRecord,
) -> Result<(), StorageError> {
    indexeddb::put(
        store,
        &indexeddb::to_js(&SupersetRow::from_record(superset))?,
    )
    .await
}

fn member_ids(superset: &SupersetRecord) -> Vec<String> {
    superset
        .members
        .iter()
        .map(|member| member.training_exercise_id.clone())
        .collect()
}

fn create_payload(operation: &Operation) -> Result<SupersetCreatePayload, StorageError> {
    if operation.kind != OperationKind::SupersetCreate {
        return Err(StorageError::InvalidOperation(
            "the operation is not a superset creation".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn delete_payload(operation: &Operation) -> Result<SupersetDeletePayload, StorageError> {
    if operation.kind != OperationKind::SupersetDelete {
        return Err(StorageError::InvalidOperation(
            "the operation is not a superset deletion".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn reorder_payload(operation: &Operation) -> Result<SupersetReorderPayload, StorageError> {
    if operation.kind != OperationKind::SupersetReorder {
        return Err(StorageError::InvalidOperation(
            "the operation is not a superset reorder".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}
