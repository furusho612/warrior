//! セットと段の端末内の保存、並べ替え、完了と、送信待ちの列への追加。
//!
//! セットの追加、更新、削除、並べ替え、完了は、行の書き込みと同じトランザクションで
//! 操作を送信待ちの列へ積む (ADR-002)。段はセットの更新で全置換し、削除ではセットと
//! 一緒に消す。記録した時点の体重はセットの行が持ち、段の重量は自重種目では加重である。

use warrior_core::sync::{
    Operation, OperationKind, OutboxEntry, SetCompletePayload, SetCreatePayload, SetDeletePayload,
    SetReorderPayload, SetSegmentPayload, SetUpdatePayload,
};
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord, TrainingRecord};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::exercise::{enqueue_operation, read_rows};
use super::{abort, indexeddb, outbox, RestoreOutcome, Storage, StorageError, Store};

/// 端末に保存した 1 つのセットと、その段。
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSet {
    /// セットの行。
    pub set: SetRecordRow,
    /// セットの段。並び順である。
    pub segments: Vec<SetSegmentRecord>,
}

impl Storage {
    /// 種目に属するセットを、位置の昇順で段とともに返す。
    pub async fn training_sets(
        &self,
        training_exercise_id: &str,
    ) -> Result<Vec<StoredSet>, StorageError> {
        let values = self.all_records(Store::Sets).await?;
        let mut sets = values
            .iter()
            .map(|value| {
                serde_json::from_value::<SetRecordRow>(value.clone())
                    .map_err(|error| StorageError::Serialize(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        sets.retain(|set| set.training_exercise_id == training_exercise_id);
        sets.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        let segments = self.all_records(Store::SetSegments).await?;
        let mut segments = segments
            .iter()
            .map(|value| {
                serde_json::from_value::<SetSegmentRecord>(value.clone())
                    .map_err(|error| StorageError::Serialize(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        segments.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(sets
            .into_iter()
            .map(|set| StoredSet {
                segments: segments
                    .iter()
                    .filter(|segment| segment.set_id == set.id)
                    .cloned()
                    .collect(),
                set,
            })
            .collect())
    }

    /// セットの追加とその操作を 1 つのトランザクションで保存する。
    pub async fn save_set_create(
        &self,
        set: &SetRecordRow,
        segments: &[SetSegmentRecord],
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = create_payload(operation)?;
        if payload.set_id != set.id
            || payload.training_exercise_id != set.training_exercise_id
            || payload.training_id != operation.training_id
            || !same_segments(&payload.segments, segments)
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not this set creation".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let segment_store = transaction.store(Store::SetSegments.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if let Err(error) = indexeddb::put(&sets, &indexeddb::to_js(set)?).await {
            return abort(transaction, error).await;
        }
        for segment in segments {
            if let Err(error) = indexeddb::put(&segment_store, &indexeddb::to_js(segment)?).await {
                return abort(transaction, error).await;
            }
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// セットの更新とその操作を 1 つのトランザクションで保存する。段は全置換する。
    pub async fn save_set_update(
        &self,
        set: &SetRecordRow,
        segments: &[SetSegmentRecord],
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = update_payload(operation)?;
        if payload.set_id != set.id
            || payload.training_id != operation.training_id
            || !same_segments(&payload.segments, segments)
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not this set update".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let segment_store = transaction.store(Store::SetSegments.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if indexeddb::get(&sets, &JsValue::from_str(&set.id))
            .await?
            .is_none()
        {
            transaction.finish().await?;
            return Ok(());
        }
        if let Err(error) = indexeddb::put(&sets, &indexeddb::to_js(set)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = delete_segments(&segment_store, &set.id).await {
            return abort(transaction, error).await;
        }
        for segment in segments {
            if let Err(error) = indexeddb::put(&segment_store, &indexeddb::to_js(segment)?).await {
                return abort(transaction, error).await;
            }
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// セットの削除とその操作を 1 つのトランザクションで保存する。
    ///
    /// セットの段も同じトランザクションで削除する。
    pub async fn save_set_delete(
        &self,
        training_id: &str,
        set_id: &str,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = delete_payload(operation)?;
        if payload.set_id != set_id || payload.training_id != training_id {
            return Err(StorageError::InvalidOperation(
                "the operation is not this set deletion".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let segments = transaction.store(Store::SetSegments.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if let Err(error) = delete_segments(&segments, set_id).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = indexeddb::delete_entry(&sets, &JsValue::from_str(set_id)).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// セットの並べ替えとその操作を 1 つのトランザクションで保存する。
    pub async fn save_set_reorder(
        &self,
        training_id: &str,
        training_exercise_id: &str,
        ordered_ids: &[String],
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = reorder_payload(operation)?;
        if payload.training_id != training_id
            || payload.training_exercise_id != training_exercise_id
            || payload.ordered_ids != ordered_ids
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not this set reorder".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Sets.name(), Store::Outbox.name(), Store::Meta.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let stored = read_rows::<SetRecordRow>(&sets).await?;
        let now = crate::ids::now_iso();
        for (index, id) in ordered_ids.iter().enumerate() {
            let Some(set) = stored
                .iter()
                .find(|set| set.training_exercise_id == training_exercise_id && set.id == *id)
            else {
                return abort(
                    transaction,
                    StorageError::InvalidOperation(
                        "the set does not exist in the training exercise".to_owned(),
                    ),
                )
                .await;
            };
            let mut updated = set.clone();
            updated.position = i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1);
            updated.updated_at = now.clone();
            if let Err(error) = indexeddb::put(&sets, &indexeddb::to_js(&updated)?).await {
                return abort(transaction, error).await;
            }
        }
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// セットの完了の状態の変更とその操作を 1 つのトランザクションで保存する。
    pub async fn save_set_complete(
        &self,
        set: &SetRecordRow,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        let payload = complete_payload(operation)?;
        if payload.set_id != set.id || payload.training_id != operation.training_id {
            return Err(StorageError::InvalidOperation(
                "the operation is not this set completion".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Sets.name(), Store::Outbox.name(), Store::Meta.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if indexeddb::get(&sets, &JsValue::from_str(&set.id))
            .await?
            .is_none()
        {
            transaction.finish().await?;
            return Ok(());
        }
        if let Err(error) = indexeddb::put(&sets, &indexeddb::to_js(set)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// サーバーに保存済みの 1 つのセットと段を端末へ戻す。
    pub async fn restore_set(
        &self,
        record: &TrainingRecord,
        set_id: &str,
        after_sequence: u64,
    ) -> Result<(), StorageError> {
        self.restore_set_with_outcome(record, set_id, after_sequence)
            .await
            .map(|_| ())
    }

    /// サーバーに保存済みの 1 つのセットと段を端末へ戻し、見送ったかを返す。
    ///
    /// 破棄した操作より後の、同じセットの未解決の操作または同じ種目の並べ替えがあるときは、
    /// 端末の値を上書きしない。
    pub async fn restore_set_with_outcome(
        &self,
        record: &TrainingRecord,
        set_id: &str,
        after_sequence: u64,
    ) -> Result<RestoreOutcome, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Outbox.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let segments = transaction.store(Store::SetSegments.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let entries = outbox::read(&outbox_store).await?;
        let training_exercise_id = record
            .sets
            .iter()
            .find(|set| set.id == set_id)
            .map(|set| set.training_exercise_id.as_str());
        if has_newer_set_change(&entries, set_id, training_exercise_id, after_sequence) {
            transaction.finish().await?;
            return Ok(RestoreOutcome::Deferred);
        }
        if let Err(error) = delete_set_rows_in(&sets, &segments, set_id).await {
            return abort(transaction, error).await;
        }
        if let Some(set) = record.sets.iter().find(|set| set.id == set_id) {
            if let Err(error) = indexeddb::put(&sets, &indexeddb::to_js(set)?).await {
                return abort(transaction, error).await;
            }
            for segment in record
                .segments
                .iter()
                .filter(|segment| segment.set_id == set_id)
            {
                if let Err(error) = indexeddb::put(&segments, &indexeddb::to_js(segment)?).await {
                    return abort(transaction, error).await;
                }
            }
        }
        transaction.finish().await?;
        Ok(RestoreOutcome::Restored)
    }

    /// サーバーに保存済みのセットの位置を端末へ戻す。
    ///
    /// サーバーに無いセットは、端末だけにある未送信の行として残す。
    pub async fn restore_set_positions(&self, record: &TrainingRecord) -> Result<(), StorageError> {
        self.restore_set_positions_with_outcome(record, None, 0)
            .await
            .map(|_| ())
    }

    /// サーバーに保存済みのセットの位置を端末へ戻し、見送ったかを返す。
    pub async fn restore_set_positions_with_outcome(
        &self,
        record: &TrainingRecord,
        training_exercise_id: Option<&str>,
        after_sequence: u64,
    ) -> Result<RestoreOutcome, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Sets.name(), Store::Outbox.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let sets = transaction.store(Store::Sets.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let entries = outbox::read(&outbox_store).await?;
        if training_exercise_id.is_some_and(|training_exercise_id| {
            has_newer_set_reorder(&entries, training_exercise_id, after_sequence)
        }) {
            transaction.finish().await?;
            return Ok(RestoreOutcome::Deferred);
        }
        let rows = read_rows::<SetRecordRow>(&sets).await?;
        for row in rows {
            let Some(server) = record.sets.iter().find(|set| set.id == row.id) else {
                continue;
            };
            let mut updated = row.clone();
            updated.position = server.position;
            updated.updated_at = server.updated_at.clone();
            if let Err(error) = indexeddb::put(&sets, &indexeddb::to_js(&updated)?).await {
                return abort(transaction, error).await;
            }
        }
        transaction.finish().await?;
        Ok(RestoreOutcome::Restored)
    }
}

async fn delete_segments(store: &IdbObjectStore, set_id: &str) -> Result<(), StorageError> {
    let segments = read_rows::<SetSegmentRecord>(store).await?;
    for segment in segments.iter().filter(|segment| segment.set_id == set_id) {
        indexeddb::delete_entry(store, &JsValue::from_str(&segment.id)).await?;
    }
    Ok(())
}

async fn delete_set_rows_in(
    sets: &IdbObjectStore,
    segments: &IdbObjectStore,
    set_id: &str,
) -> Result<(), StorageError> {
    delete_segments(segments, set_id).await?;
    indexeddb::delete_entry(sets, &JsValue::from_str(set_id)).await
}

/// 破棄するセットの行と段を削除する。
///
/// 送信待ちの列の破棄で、サーバーに無いセットの作成を取り消すときに使う。
pub(super) async fn delete_set_rows(
    transaction: &indexeddb::Transaction,
    set_id: &str,
) -> Result<(), StorageError> {
    let sets = transaction.store(Store::Sets.name())?;
    let segments = transaction.store(Store::SetSegments.name())?;
    delete_set_rows_in(&sets, &segments, set_id).await
}

/// 操作の本文からセットの ID を取る。
pub(super) fn payload_set_id(operation: &Operation) -> Result<String, StorageError> {
    operation
        .payload
        .get("set_id")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .ok_or_else(|| StorageError::InvalidOperation("the operation has no set id".to_owned()))
}

/// 同じセットの、指定の順序より後の未解決の変更があるか。
///
/// 失敗した操作の破棄で、端末に残る未送信の値をサーバーの値で上書きしないために見る。
fn has_newer_set_change(
    entries: &[OutboxEntry],
    set_id: &str,
    training_exercise_id: Option<&str>,
    after_sequence: u64,
) -> bool {
    let same_set = entries.iter().any(|entry| {
        let operation = &entry.operation;
        operation.sequence > after_sequence
            && matches!(
                operation.kind,
                OperationKind::SetCreate
                    | OperationKind::SetUpdate
                    | OperationKind::SetComplete
                    | OperationKind::SetDelete
            )
            && super::discard::payload_text(operation, "set_id") == Some(set_id)
    });
    same_set
        || training_exercise_id.is_some_and(|training_exercise_id| {
            has_newer_set_reorder(entries, training_exercise_id, after_sequence)
        })
}

fn has_newer_set_reorder(
    entries: &[OutboxEntry],
    training_exercise_id: &str,
    after_sequence: u64,
) -> bool {
    entries.iter().any(|entry| {
        entry.operation.sequence > after_sequence
            && entry.operation.kind == OperationKind::SetReorder
            && super::discard::payload_text(&entry.operation, "training_exercise_id")
                == Some(training_exercise_id)
    })
}

fn create_payload(operation: &Operation) -> Result<SetCreatePayload, StorageError> {
    if operation.kind != OperationKind::SetCreate {
        return Err(StorageError::InvalidOperation(
            "the operation is not a set creation".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn update_payload(operation: &Operation) -> Result<SetUpdatePayload, StorageError> {
    if operation.kind != OperationKind::SetUpdate {
        return Err(StorageError::InvalidOperation(
            "the operation is not a set update".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn delete_payload(operation: &Operation) -> Result<SetDeletePayload, StorageError> {
    if operation.kind != OperationKind::SetDelete {
        return Err(StorageError::InvalidOperation(
            "the operation is not a set deletion".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn reorder_payload(operation: &Operation) -> Result<SetReorderPayload, StorageError> {
    if operation.kind != OperationKind::SetReorder {
        return Err(StorageError::InvalidOperation(
            "the operation is not a set reorder".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn complete_payload(operation: &Operation) -> Result<SetCompletePayload, StorageError> {
    if operation.kind != OperationKind::SetComplete {
        return Err(StorageError::InvalidOperation(
            "the operation is not a set completion".to_owned(),
        ));
    }
    serde_json::from_value(operation.payload.clone())
        .map_err(|error| StorageError::Serialize(error.to_string()))
}

fn same_segments(payload: &[SetSegmentPayload], segments: &[SetSegmentRecord]) -> bool {
    payload.len() == segments.len()
        && payload.iter().zip(segments).all(|(payload, segment)| {
            payload.segment_id == segment.id
                && payload.position == segment.position
                && payload.weight_kg == segment.weight_kg.unwrap_or(0.0)
                && payload.reps == u32::try_from(segment.reps.unwrap_or(0)).unwrap_or(0)
                && i64::from(payload.momentum_reps) == segment.momentum_reps
                && i64::from(payload.assisted_reps) == segment.assisted_reps
                && payload.duration_seconds
                    == segment
                        .duration_seconds
                        .and_then(|duration| u32::try_from(duration).ok())
                && same_side(
                    payload.left.as_ref(),
                    segment.left_weight_kg,
                    segment.left_reps,
                    segment.left_duration_seconds,
                )
                && same_side(
                    payload.right.as_ref(),
                    segment.right_weight_kg,
                    segment.right_reps,
                    segment.right_duration_seconds,
                )
        })
}

fn same_side(
    side: Option<&warrior_core::sync::SetSidePayload>,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> bool {
    match side {
        None => weight_kg.is_none() && reps.is_none() && duration_seconds.is_none(),
        Some(side) => {
            weight_kg == side.weight_kg
                && reps == Some(i64::from(side.reps))
                && duration_seconds == side.duration_seconds.map(i64::from)
        }
    }
}
