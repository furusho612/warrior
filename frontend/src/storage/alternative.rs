//! 代わりの種目の候補を選ぶための、種目ライブラリと過去に記録した種目の ID の
//! 端末内の保存。
//!
//! 候補の並びには種目マスターの内容と、過去に記録した種目の ID だけを使う。
//! 過去の記録の本文は保存しない。ログインのたびにサーバーの値で置き換える。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use warrior_core::exercise::Exercise;
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::{abort, indexeddb, Storage, StorageError, Store};

/// 過去に記録した種目の ID の行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordedExerciseRow {
    exercise_id: String,
}

impl Storage {
    /// 種目ライブラリと過去に記録した種目の ID を置き換える。
    pub async fn save_alternative_source(
        &self,
        exercises: &[Exercise],
        recorded_ids: &[String],
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::LibraryExercises.name(),
                Store::RecordedExercises.name(),
            ],
            IdbTransactionMode::Readwrite,
        )?;
        match save_alternative_source_steps(&transaction, exercises, recorded_ids).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 保存した種目ライブラリを返す。
    pub async fn library_exercises(&self) -> Result<Vec<Exercise>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::LibraryExercises.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(Store::LibraryExercises.name())?;
        let exercises = read_rows::<Exercise>(&store).await?;
        transaction.finish().await?;
        Ok(exercises)
    }

    /// 保存した過去に記録した種目の ID を返す。
    pub async fn recorded_exercise_ids(&self) -> Result<Vec<String>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::RecordedExercises.name()],
            IdbTransactionMode::Readonly,
        )?;
        let store = transaction.store(Store::RecordedExercises.name())?;
        let mut ids = read_rows::<RecordedExerciseRow>(&store)
            .await?
            .into_iter()
            .map(|row| row.exercise_id)
            .collect::<Vec<_>>();
        transaction.finish().await?;
        ids.sort();
        Ok(ids)
    }

    /// 候補の選択に使う種目ライブラリと、過去に記録した種目の ID を返す。
    pub async fn alternative_source(&self) -> Result<(Vec<Exercise>, Vec<String>), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[
                Store::LibraryExercises.name(),
                Store::RecordedExercises.name(),
            ],
            IdbTransactionMode::Readonly,
        )?;
        let library = transaction.store(Store::LibraryExercises.name())?;
        let recorded = transaction.store(Store::RecordedExercises.name())?;
        let exercises = read_rows::<Exercise>(&library).await?;
        let mut ids = read_rows::<RecordedExerciseRow>(&recorded)
            .await?
            .into_iter()
            .map(|row| row.exercise_id)
            .collect::<Vec<_>>();
        transaction.finish().await?;
        ids.sort();
        Ok((exercises, ids))
    }
}

async fn save_alternative_source_steps(
    transaction: &indexeddb::Transaction,
    exercises: &[Exercise],
    recorded_ids: &[String],
) -> Result<(), StorageError> {
    let library = transaction.store(Store::LibraryExercises.name())?;
    replace_library(&library, exercises).await?;
    let recorded = transaction.store(Store::RecordedExercises.name())?;
    replace_recorded(&recorded, recorded_ids).await?;
    Ok(())
}

async fn replace_library(
    store: &IdbObjectStore,
    exercises: &[Exercise],
) -> Result<(), StorageError> {
    let current = read_rows::<Exercise>(store).await?;
    let ids = exercises
        .iter()
        .map(|exercise| exercise.id.as_str())
        .collect::<HashSet<_>>();
    for exercise in &current {
        if !ids.contains(exercise.id.as_str()) {
            indexeddb::delete_entry(store, &JsValue::from_str(&exercise.id)).await?;
        }
    }
    for exercise in exercises {
        indexeddb::put(store, &indexeddb::to_js(exercise)?).await?;
    }
    Ok(())
}

async fn replace_recorded(
    store: &IdbObjectStore,
    recorded_ids: &[String],
) -> Result<(), StorageError> {
    let current = read_rows::<RecordedExerciseRow>(store).await?;
    let ids = recorded_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    for row in &current {
        if !ids.contains(row.exercise_id.as_str()) {
            indexeddb::delete_entry(store, &JsValue::from_str(&row.exercise_id)).await?;
        }
    }
    for exercise_id in recorded_ids {
        let row = RecordedExerciseRow {
            exercise_id: exercise_id.clone(),
        };
        indexeddb::put(store, &indexeddb::to_js(&row)?).await?;
    }
    Ok(())
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
