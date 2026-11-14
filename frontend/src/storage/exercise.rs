//! 種目の行と、種目に属するセット、段、スーパーセットの参照の端末内の保存と削除。

use js_sys::Array;
use serde::Deserialize;
use warrior_core::sync::{unresolved_create, Operation, OperationKind, OutboxEntry, OutboxState};
use warrior_core::training_record::{TrainingExerciseRecord, TrainingRecord};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::{abort, indexeddb, outbox, put_rows, Storage, StorageError, Store, StoredSuperset};

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
        if let Err(error) = delete_training_exercise_rows(&transaction, &stored.id).await {
            return abort(transaction, error).await;
        }
        if let Err(error) = enqueue_operation(&outbox_store, &meta, &entries, operation).await {
            return abort(transaction, error).await;
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
                Store::Outbox.name(),
                Store::Meta.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        let exercises = transaction.store(Store::TrainingExercises.name())?;
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
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::TrainingExercises.name(),
                Store::Sets.name(),
                Store::SetSegments.name(),
                Store::Supersets.name(),
                Store::SupersetMembers.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        match restore_training_exercise_steps(&transaction, record, training_exercise_id).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// サーバーの記録にある種目の位置を端末へ戻す。
    ///
    /// 送信待ちの列と、種目以外の行には触れない。並べ替えの破棄で使う。
    pub async fn restore_training_exercise_positions(
        &self,
        record: &TrainingRecord,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::TrainingExercises.name()],
            IdbTransactionMode::Readwrite,
        )?;
        match restore_training_exercise_positions_steps(&transaction, record).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }
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
    put_rows(
        transaction,
        Store::TrainingExercises,
        std::slice::from_ref(server_exercise),
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
    let members = supersets
        .iter()
        .flat_map(|superset| superset.members.iter())
        .filter(|member| member.training_exercise_id == training_exercise_id)
        .collect::<Vec<_>>();
    put_rows(transaction, Store::Supersets, &stored_supersets).await?;
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
    Ok(())
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

async fn delete_member(
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
