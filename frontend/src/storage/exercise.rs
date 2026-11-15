//! 種目の行と、種目に属するセット、段、スーパーセットの参照の端末内の保存と削除。

use js_sys::Array;
use serde::Deserialize;
use warrior_core::superset::SupersetMembers;
use warrior_core::sync::{
    pending_exercise_memo, unresolved_create, Operation, OperationKind, OutboxEntry, OutboxState,
    TrainingExerciseMemoPayload,
};
use warrior_core::training_record::{SupersetMemberRecord, TrainingExerciseRecord, TrainingRecord};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::superset::{delete_members, put_superset, read_superset_rows};
use super::{
    abort, indexeddb, outbox, put_rows, RestoreOutcome, Storage, StorageError, Store,
    StoredSuperset,
};

impl Storage {
    /// トレーニングに追加した種目を位置の昇順で返す。
    pub async fn training_exercises(
        &self,
        training_id: &str,
    ) -> Result<Vec<TrainingExerciseRecord>, StorageError> {
        let values = self.all_records(Store::TrainingExercises).await?;
        let mut exercises = values
            .iter()
            .map(|value| {
                serde_json::from_value::<TrainingExerciseRecord>(value.clone())
                    .map_err(|error| StorageError::Serialize(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        exercises.retain(|exercise| exercise.training_id == training_id);
        exercises.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(exercises)
    }

    /// トレーニングへの種目の追加とその操作を 1 つのトランザクションで保存する。
    pub async fn save_training_exercise_create(
        &self,
        exercise: &TrainingExerciseRecord,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingExerciseCreate
            || operation.training_id != exercise.training_id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not an exercise creation".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingExercises.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let exercises = transaction.store(Store::TrainingExercises.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;

        if let Err(error) = indexeddb::put(&exercises, &indexeddb::to_js(exercise)?).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 種目のメモとその操作を 1 つのトランザクションで保存する。
    ///
    /// 送信待ちの同じ種目のメモがまだ送られていないときは、その操作の本文を新しいメモへ
    /// 置き換える。入力のたびに操作を増やさないためである。
    pub async fn save_training_exercise_memo(
        &self,
        training_id: &str,
        training_exercise_id: &str,
        memo: &str,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingExerciseMemo
            || operation.training_id != training_id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not an exercise memo".to_owned(),
            ));
        }
        let payload: TrainingExerciseMemoPayload =
            serde_json::from_value(operation.payload.clone())
                .map_err(|error| StorageError::Serialize(error.to_string()))?;
        if payload.training_id != training_id
            || payload.training_exercise_id != training_exercise_id
            || payload.memo != memo
        {
            return Err(StorageError::InvalidOperation(
                "the operation does not match the memo".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingExercises.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let exercises = transaction.store(Store::TrainingExercises.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let Some(mut stored) = read_exercise(&exercises, training_exercise_id).await? else {
            return Err(StorageError::InvalidOperation(
                "the exercise does not exist".to_owned(),
            ));
        };
        if stored.training_id != training_id {
            return Err(StorageError::InvalidOperation(
                "the exercise does not exist in the training".to_owned(),
            ));
        }
        stored.memo = memo.to_owned();
        stored.updated_at = crate::ids::now_iso();
        if let Err(error) = indexeddb::put(&exercises, &indexeddb::to_js(&stored)?).await {
            return abort(transaction, error).await;
        }
        let entries = outbox::read(&outbox_store).await?;
        let Some(target) =
            pending_exercise_memo(&entries, training_id, training_exercise_id).cloned()
        else {
            if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
                return abort(transaction, error).await;
            }
            return transaction.finish().await;
        };
        let mut stored_operation = target.operation;
        stored_operation.payload = operation.payload.clone();
        let entry = OutboxEntry {
            operation: stored_operation,
            state: target.state,
            attempts: target.attempts,
            attempted: false,
        };
        if let Err(error) = indexeddb::put(&outbox_store, &indexeddb::to_js(&entry)?).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// トレーニングからの種目の削除とその操作を 1 つのトランザクションで保存する。
    ///
    /// 種目に属するセットと段、スーパーセットの参照も同じトランザクションで削除する。
    pub async fn save_training_exercise_delete(
        &self,
        training_id: &str,
        training_exercise_id: &str,
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingExerciseDelete
            || operation.training_id != training_id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not an exercise deletion".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
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
        let exercises = transaction.store(Store::TrainingExercises.name())?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        let stored = read_exercise(&exercises, training_exercise_id).await?;
        let Some(stored) = stored.filter(|exercise| exercise.training_id == training_id) else {
            return Err(StorageError::InvalidOperation(
                "the exercise does not exist in the training".to_owned(),
            ));
        };
        let affected = superset_ids_for_exercise(&transaction, &stored.id).await?;
        if let Err(error) = delete_training_exercise_rows(&transaction, &stored.id).await {
            return abort(transaction, error).await;
        }
        let order = exercise_order(&transaction, training_id).await?;
        let group_operations =
            match sync_supersets_after_change(&transaction, training_id, &affected, &order).await {
                Ok(operations) => operations,
                Err(error) => return abort(transaction, error).await,
            };
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        for group_operation in &group_operations {
            if let Err(error) = enqueue_child_operation(
                &outbox_store,
                &meta,
                group_operation,
                &operation.operation_id,
            )
            .await
            {
                return abort(transaction, error).await;
            }
        }
        transaction.finish().await
    }

    /// トレーニングの種目の並べ替えとその操作を 1 つのトランザクションで保存する。
    pub async fn save_training_exercise_reorder(
        &self,
        training_id: &str,
        ordered_ids: &[String],
        operation: &Operation,
    ) -> Result<(), StorageError> {
        if operation.kind != OperationKind::TrainingExerciseReorder
            || operation.training_id != training_id
        {
            return Err(StorageError::InvalidOperation(
                "the operation is not an exercise reorder".to_owned(),
            ));
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingExercises.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let exercises = transaction.store(Store::TrainingExercises.name())?;
        let supersets = transaction.store(Store::Supersets.name())?;
        let stored = read_rows::<TrainingExerciseRecord>(&exercises).await?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let meta = transaction.store(Store::Meta.name())?;
        let entries = outbox::read(&outbox_store).await?;
        let now = crate::ids::now_iso();
        for (index, id) in ordered_ids.iter().enumerate() {
            let Some(exercise) = stored
                .iter()
                .find(|exercise| exercise.training_id == training_id && exercise.id == *id)
            else {
                continue;
            };
            let mut updated = exercise.clone();
            updated.position = i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1);
            updated.updated_at = now.clone();
            if let Err(error) = indexeddb::put(&exercises, &indexeddb::to_js(&updated)?).await {
                return abort(transaction, error).await;
            }
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
        }
        let superset_ids = read_superset_rows(&supersets)
            .await?
            .into_iter()
            .filter(|superset| superset.training_id == training_id)
            .map(|superset| superset.id)
            .collect::<Vec<_>>();
        let group_operations = match sync_supersets_after_change(
            &transaction,
            training_id,
            &superset_ids,
            ordered_ids,
        )
        .await
        {
            Ok(operations) => operations,
            Err(error) => return abort(transaction, error).await,
        };
        for group_operation in &group_operations {
            if let Err(error) = enqueue_child_operation(
                &outbox_store,
                &meta,
                group_operation,
                &operation.operation_id,
            )
            .await
            {
                return abort(transaction, error).await;
            }
        }
        transaction.finish().await
    }

    /// サーバーの記録から、削除した種目の行と、その種目に属するセット、段、
    /// スーパーセットの参照だけを戻す。
    ///
    /// 送信待ちの列、メモ、ほかの種目の行と入力には触れない。削除の破棄で使う。
    pub async fn restore_training_exercise(
        &self,
        record: &TrainingRecord,
        training_exercise_id: &str,
    ) -> Result<(), StorageError> {
        self.restore_training_exercise_with_outcome(record, training_exercise_id, 0)
            .await
            .map(|_| ())
    }

    /// 削除した種目を戻し、見送ったかを返す。
    ///
    /// 破棄した操作より後の、その種目またはその種目に属するセットの未解決の操作があるときは、
    /// 端末の値を上書きしない。
    pub async fn restore_training_exercise_with_outcome(
        &self,
        record: &TrainingRecord,
        training_exercise_id: &str,
        after_sequence: u64,
    ) -> Result<RestoreOutcome, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingExercises.name(),
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let entries = super::outbox::read(&outbox_store).await?;
        let set_ids = record
            .sets
            .iter()
            .filter(|set| set.training_exercise_id == training_exercise_id)
            .map(|set| set.id.as_str())
            .collect::<Vec<_>>();
        if has_newer_exercise_change(
            &entries,
            &record.session.id,
            training_exercise_id,
            &set_ids,
            after_sequence,
        ) {
            transaction.finish().await?;
            return Ok(RestoreOutcome::Deferred);
        }
        match restore_training_exercise_steps(&transaction, record, training_exercise_id).await {
            Ok(()) => {
                transaction.finish().await?;
                Ok(RestoreOutcome::Restored)
            }
            Err(error) => abort(transaction, error).await,
        }
    }

    /// サーバーの記録にある種目の位置を端末へ戻す。
    ///
    /// 種目の並べ替えに追随した組の並びもサーバーの記録へ戻す。送信待ちの列と、
    /// 種目以外の行には触れない。並べ替えの破棄で使う。
    pub async fn restore_training_exercise_positions(
        &self,
        record: &TrainingRecord,
    ) -> Result<(), StorageError> {
        self.restore_training_exercise_positions_with_outcome(record, 0)
            .await
            .map(|_| ())
    }

    /// サーバーの記録にある種目の位置を戻し、見送ったかを返す。
    pub async fn restore_training_exercise_positions_with_outcome(
        &self,
        record: &TrainingRecord,
        after_sequence: u64,
    ) -> Result<RestoreOutcome, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingExercises.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
                Store::Outbox.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let outbox_store = transaction.store(Store::Outbox.name())?;
        let entries = super::outbox::read(&outbox_store).await?;
        if has_newer_exercise_reorder(&entries, &record.session.id, after_sequence) {
            transaction.finish().await?;
            return Ok(RestoreOutcome::Deferred);
        }
        match restore_training_exercise_positions_steps(&transaction, record).await {
            Ok(()) => {
                transaction.finish().await?;
                Ok(RestoreOutcome::Restored)
            }
            Err(error) => abort(transaction, error).await,
        }
    }
}

/// 種目または種目に属するセットの、指定の順序より後の未解決の操作があるか。
fn has_newer_exercise_change(
    entries: &[OutboxEntry],
    training_id: &str,
    training_exercise_id: &str,
    set_ids: &[&str],
    after_sequence: u64,
) -> bool {
    let same_change = entries.iter().any(|entry| {
        let operation = &entry.operation;
        if operation.sequence <= after_sequence {
            return false;
        }
        let same_exercise = super::discard::payload_text(operation, "training_exercise_id")
            == Some(training_exercise_id);
        let same_set = super::discard::payload_text(operation, "set_id")
            .is_some_and(|set_id| set_ids.contains(&set_id));
        (matches!(
            operation.kind,
            OperationKind::SetCreate
                | OperationKind::SetReorder
                | OperationKind::TrainingExerciseDelete
                | OperationKind::TrainingExerciseMemo
        ) && same_exercise)
            || (matches!(
                operation.kind,
                OperationKind::SetUpdate | OperationKind::SetComplete | OperationKind::SetDelete
            ) && same_set)
    });
    same_change || has_newer_exercise_reorder(entries, training_id, after_sequence)
}

fn has_newer_exercise_reorder(
    entries: &[OutboxEntry],
    training_id: &str,
    after_sequence: u64,
) -> bool {
    entries.iter().any(|entry| {
        entry.operation.sequence > after_sequence
            && entry.operation.kind == OperationKind::TrainingExerciseReorder
            && entry.operation.training_id == training_id
    })
}

async fn restore_training_exercise_steps(
    transaction: &indexeddb::Transaction,
    record: &TrainingRecord,
    training_exercise_id: &str,
) -> Result<(), StorageError> {
    let training_id = record.session.id.as_str();
    let server_exercise = record.exercises.iter().find(|exercise| {
        exercise.training_id == training_id && exercise.id == training_exercise_id
    });
    delete_training_exercise_rows(transaction, training_exercise_id).await?;
    let Some(server_exercise) = server_exercise else {
        return Ok(());
    };
    let mut server_exercise = server_exercise.clone();
    server_exercise.confirmed_memo = server_exercise.memo.clone();
    put_rows(
        transaction,
        Store::TrainingExercises,
        std::slice::from_ref(&server_exercise),
    )
    .await?;

    let sets = record
        .sets
        .iter()
        .filter(|set| set.training_exercise_id == training_exercise_id)
        .collect::<Vec<_>>();
    let set_ids = sets.iter().map(|set| set.id.as_str()).collect::<Vec<_>>();
    let segments = record
        .segments
        .iter()
        .filter(|segment| set_ids.contains(&segment.set_id.as_str()))
        .collect::<Vec<_>>();
    put_rows(transaction, Store::Sets, &sets).await?;
    put_rows(transaction, Store::SetSegments, &segments).await?;

    let supersets = record
        .supersets
        .iter()
        .filter(|superset| {
            superset
                .members
                .iter()
                .any(|member| member.training_exercise_id == training_exercise_id)
        })
        .collect::<Vec<_>>();
    let members_store = transaction.store(Store::SupersetMembers.name())?;
    let stored_supersets = supersets
        .iter()
        .map(|superset| StoredSuperset {
            id: &superset.id,
            training_id: &superset.training_id,
            position: superset.position,
            created_at: &superset.created_at,
            updated_at: &superset.updated_at,
        })
        .collect::<Vec<_>>();
    for superset in &supersets {
        delete_members(&members_store, &superset.id).await?;
    }
    put_rows(transaction, Store::Supersets, &stored_supersets).await?;
    let members = supersets
        .iter()
        .flat_map(|superset| superset.members.iter())
        .collect::<Vec<_>>();
    put_rows(transaction, Store::SupersetMembers, &members).await?;
    Ok(())
}

async fn restore_training_exercise_positions_steps(
    transaction: &indexeddb::Transaction,
    record: &TrainingRecord,
) -> Result<(), StorageError> {
    let training_id = record.session.id.as_str();
    let store = transaction.store(Store::TrainingExercises.name())?;
    let rows = read_rows::<TrainingExerciseRecord>(&store).await?;
    for row in rows.iter().filter(|row| row.training_id == training_id) {
        let Some(server) = record
            .exercises
            .iter()
            .find(|server| server.training_id == training_id && server.id == row.id)
        else {
            continue;
        };
        let mut updated = row.clone();
        updated.position = server.position;
        indexeddb::put(&store, &indexeddb::to_js(&updated)?).await?;
    }
    restore_training_supersets(transaction, record, training_id).await
}

/// サーバーの記録にある組と種目を端末へ戻す。
async fn restore_training_supersets(
    transaction: &indexeddb::Transaction,
    record: &TrainingRecord,
    training_id: &str,
) -> Result<(), StorageError> {
    let supersets = transaction.store(Store::Supersets.name())?;
    let members = transaction.store(Store::SupersetMembers.name())?;
    let local = read_superset_rows(&supersets).await?;
    for row in local.iter().filter(|row| row.training_id == training_id) {
        delete_members(&members, &row.id).await?;
        indexeddb::delete_entry(&supersets, &JsValue::from_str(&row.id)).await?;
    }
    for superset in record
        .supersets
        .iter()
        .filter(|superset| superset.training_id == training_id)
    {
        put_superset(&supersets, superset).await?;
        for member in &superset.members {
            indexeddb::put(&members, &indexeddb::to_js(member)?).await?;
        }
    }
    Ok(())
}

async fn superset_ids_for_exercise(
    transaction: &indexeddb::Transaction,
    training_exercise_id: &str,
) -> Result<Vec<String>, StorageError> {
    let store = transaction.store(Store::SupersetMembers.name())?;
    let members = read_rows::<SupersetMemberRecord>(&store).await?;
    let mut ids = Vec::new();
    for member in members
        .iter()
        .filter(|member| member.training_exercise_id == training_exercise_id)
    {
        if !ids.contains(&member.superset_id) {
            ids.push(member.superset_id.clone());
        }
    }
    Ok(ids)
}

async fn exercise_order(
    transaction: &indexeddb::Transaction,
    training_id: &str,
) -> Result<Vec<String>, StorageError> {
    let store = transaction.store(Store::TrainingExercises.name())?;
    let mut rows = read_rows::<TrainingExerciseRecord>(&store)
        .await?
        .into_iter()
        .filter(|row| row.training_id == training_id)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(rows.into_iter().map(|row| row.id).collect())
}

/// 種目の削除と並べ替えの後に、組の種目を種目の順序へ合わせる。
///
/// 組の種目が 2 つ未満になったときは組を解散し、送信待ちの列に積む操作を返す。
async fn sync_supersets_after_change(
    transaction: &indexeddb::Transaction,
    training_id: &str,
    superset_ids: &[String],
    exercise_order: &[String],
) -> Result<Vec<Operation>, StorageError> {
    let supersets_store = transaction.store(Store::Supersets.name())?;
    let members_store = transaction.store(Store::SupersetMembers.name())?;
    let stored_supersets = read_superset_rows(&supersets_store).await?;
    let members = read_rows::<SupersetMemberRecord>(&members_store).await?;
    let mut operations = Vec::new();
    for superset_id in superset_ids {
        let mut current = members
            .iter()
            .filter(|member| member.superset_id == *superset_id)
            .collect::<Vec<_>>();
        current.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.training_exercise_id.cmp(&right.training_exercise_id))
        });
        let current_ids = current
            .iter()
            .map(|member| member.training_exercise_id.clone())
            .collect::<Vec<_>>();
        let updated = SupersetMembers::new(current_ids.clone())
            .ok()
            .and_then(|members| members.after_exercise_change(exercise_order));
        match updated {
            Some(updated) if updated.as_slice() != current_ids.as_slice() => {
                let operation = crate::superset::reorder(training_id, superset_id, &updated)
                    .map_err(|error| StorageError::Serialize(error.to_string()))?;
                for member in &current {
                    delete_member(
                        &members_store,
                        &member.superset_id,
                        &member.training_exercise_id,
                    )
                    .await?;
                }
                for (index, id) in updated.as_slice().iter().enumerate() {
                    let member = SupersetMemberRecord {
                        superset_id: superset_id.clone(),
                        training_exercise_id: id.clone(),
                        position: i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
                    };
                    indexeddb::put(&members_store, &indexeddb::to_js(&member)?).await?;
                }
                operations.push(operation);
            }
            Some(_) => {}
            None => {
                for member in &current {
                    delete_member(
                        &members_store,
                        &member.superset_id,
                        &member.training_exercise_id,
                    )
                    .await?;
                }
                if stored_supersets.iter().any(|row| row.id == *superset_id) {
                    indexeddb::delete_entry(&supersets_store, &JsValue::from_str(superset_id))
                        .await?;
                }
                operations.push(
                    crate::superset::delete(training_id, superset_id)
                        .map_err(|error| StorageError::Serialize(error.to_string()))?,
                );
            }
        }
    }
    Ok(operations)
}

pub(super) async fn enqueue_child_operation(
    outbox_store: &IdbObjectStore,
    meta: &IdbObjectStore,
    operation: &Operation,
    parent_operation_id: &str,
) -> Result<(), StorageError> {
    let mut stored = operation.clone();
    stored.sequence = outbox::next_sequence(meta, &operation.training_id).await?;
    stored.parent_operation_id = Some(parent_operation_id.to_owned());
    let entry = OutboxEntry {
        operation: stored,
        state: OutboxState::Blocked,
        attempts: 0,
        attempted: false,
    };
    indexeddb::add(outbox_store, &indexeddb::to_js(&entry)?).await
}

pub(super) async fn enqueue_operation(
    outbox_store: &IdbObjectStore,
    meta: &IdbObjectStore,
    entries: &[OutboxEntry],
    operation: &Operation,
) -> Result<(), StorageError> {
    let parent = unresolved_create(entries, &operation.training_id)
        .map(|entry| entry.operation.operation_id.clone());
    let state = if parent.is_some() {
        OutboxState::Blocked
    } else {
        OutboxState::Pending
    };
    let mut stored = operation.clone();
    stored.sequence = outbox::next_sequence(meta, &operation.training_id).await?;
    stored.parent_operation_id = parent;
    let entry = OutboxEntry {
        operation: stored,
        state,
        attempts: 0,
        attempted: false,
    };
    indexeddb::add(outbox_store, &indexeddb::to_js(&entry)?).await
}

pub(super) fn payload_training_exercise_id(operation: &Operation) -> Result<String, StorageError> {
    operation
        .payload
        .get("training_exercise_id")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            StorageError::InvalidOperation("the operation has no training exercise id".to_owned())
        })
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

pub(super) async fn read_rows<T: serde::de::DeserializeOwned>(
    store: &IdbObjectStore,
) -> Result<Vec<T>, StorageError> {
    indexeddb::get_all(store)
        .await?
        .iter()
        .map(indexeddb::from_js::<T>)
        .collect()
}

async fn read_exercise(
    store: &IdbObjectStore,
    training_exercise_id: &str,
) -> Result<Option<TrainingExerciseRecord>, StorageError> {
    let value = indexeddb::get(store, &JsValue::from_str(training_exercise_id)).await?;
    value
        .as_ref()
        .map(indexeddb::from_js::<TrainingExerciseRecord>)
        .transpose()
}

/// 1 つの種目の行と、その種目に属するセット、段、スーパーセットの参照を削除する。
pub(super) async fn delete_training_exercise_rows(
    transaction: &indexeddb::Transaction,
    training_exercise_id: &str,
) -> Result<(), StorageError> {
    let sets_store = transaction.store(Store::Sets.name())?;
    let sets = read_rows::<SetRow>(&sets_store).await?;
    let set_ids = sets
        .iter()
        .filter(|row| row.training_exercise_id == training_exercise_id)
        .map(|row| row.id.clone())
        .collect::<Vec<_>>();
    for id in &set_ids {
        indexeddb::delete_entry(&sets_store, &JsValue::from_str(id)).await?;
    }

    let segments_store = transaction.store(Store::SetSegments.name())?;
    let segments = read_rows::<SegmentRow>(&segments_store).await?;
    for segment in segments.iter().filter(|row| set_ids.contains(&row.set_id)) {
        indexeddb::delete_entry(&segments_store, &JsValue::from_str(&segment.id)).await?;
    }

    let members_store = transaction.store(Store::SupersetMembers.name())?;
    let members = read_rows::<MemberRow>(&members_store).await?;
    for member in members
        .iter()
        .filter(|row| row.training_exercise_id == training_exercise_id)
    {
        delete_member(
            &members_store,
            &member.superset_id,
            &member.training_exercise_id,
        )
        .await?;
    }

    let exercises_store = transaction.store(Store::TrainingExercises.name())?;
    indexeddb::delete_entry(&exercises_store, &JsValue::from_str(training_exercise_id)).await?;
    Ok(())
}

pub(super) async fn delete_member(
    store: &IdbObjectStore,
    superset_id: &str,
    training_exercise_id: &str,
) -> Result<(), StorageError> {
    let key = Array::new();
    key.push(&JsValue::from_str(superset_id));
    key.push(&JsValue::from_str(training_exercise_id));
    indexeddb::delete_entry(store, &key.into()).await
}

/// トレーニングのセッション、種目、セット、段、スーパーセットを削除する。
pub(super) async fn delete_session_tree(
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
        delete_training_exercise_rows(transaction, id).await?;
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
        delete_member(
            &members_store,
            &member.superset_id,
            &member.training_exercise_id,
        )
        .await?;
    }
    Ok(())
}
