//! ルーティンの API と、送信待ちの列に積む操作の組み立て。
//!
//! 保存、編集、複製、削除は端末内の行と操作を同じトランザクションで保存し、
//! 開始はサーバーで種目とセットを写して記録を返す。操作の ID と順序は保存のときに
//! `frontend/src/storage` が決める。

use serde::Deserialize;
use warrior_core::routine::{Routine, RoutineExercise, RoutineList};
use warrior_core::sync::{
    Operation, OperationKind, RoutineCopyPayload, RoutineCreatePayload, RoutineDeletePayload,
    RoutineExercisePayload, RoutineUpdatePayload,
};
use warrior_core::training_record::TrainingRecord;

use crate::http::{self, HttpError};
use crate::ids::new_uuid;
use crate::storage::{self, ReplacePurpose, StorageError};
use crate::sync;

/// ルーティンの一覧をサーバーから取る。
pub async fn list() -> Result<RoutineList, HttpError> {
    http::request_json("GET", "/api/routines").await
}

/// ルーティンを使っている独自のプログラムの名前。
#[derive(Debug, Deserialize)]
pub struct RoutineUsage {
    /// 使っている独自のプログラムの名前。
    pub programs: Vec<String>,
}

/// ルーティンを使っている独自のプログラムを取る。
pub async fn usage(routine_id: &str) -> Result<RoutineUsage, HttpError> {
    http::request_json("GET", &format!("/api/routines/{routine_id}/usage")).await
}

/// ルーティンからトレーニングを開始し、サーバーに保存された記録を返す。
pub async fn start(routine_id: &str) -> Result<TrainingRecord, HttpError> {
    http::request_json("POST", &format!("/api/routines/{routine_id}/start")).await
}

/// 送信待ちの操作を送ってからルーティンを開始し、端末へ記録を保存する。
///
/// 種目ごとのレスト時間はルーティンの目標を端末の設定へ写す。
/// 返す値は開始したトレーニングの ID である。
pub async fn start_and_store(routine_id: &str) -> Result<String, StorageError> {
    sync::flush().await;
    let record = start(routine_id)
        .await
        .map_err(|_| StorageError::InvalidOperation("the routine start failed".to_owned()))?;
    let storage = storage::current().await?;
    storage
        .replace_session_tree(&record, ReplacePurpose::Reload)
        .await?;
    if let Some(routine) = storage.routine(routine_id).await? {
        for (target, exercise) in routine.exercises.iter().zip(record.exercises.iter()) {
            if let Some(seconds) = target.target.rest_seconds {
                let _ = storage
                    .save_rest_exercise_seconds(&exercise.id, Some(seconds))
                    .await;
            }
        }
    }
    Ok(record.session.id)
}

/// ルーティンの作成の操作を作る。
pub fn create(routine: &Routine) -> Result<Operation, StorageError> {
    let payload = RoutineCreatePayload {
        routine_id: routine.id.clone(),
        name: routine.name.clone(),
        position: routine.position,
        exercises: exercise_payloads(&routine.exercises),
    };
    operation(
        &routine.id,
        OperationKind::RoutineCreate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// ルーティンの更新の操作を作る。
pub fn update(routine: &Routine) -> Result<Operation, StorageError> {
    let payload = RoutineUpdatePayload {
        routine_id: routine.id.clone(),
        name: routine.name.clone(),
        position: routine.position,
        exercises: exercise_payloads(&routine.exercises),
    };
    operation(
        &routine.id,
        OperationKind::RoutineUpdate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// ルーティンの削除の操作を作る。
pub fn delete(routine_id: &str) -> Result<Operation, StorageError> {
    let payload = RoutineDeletePayload {
        routine_id: routine_id.to_owned(),
    };
    operation(
        routine_id,
        OperationKind::RoutineDelete,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// 元のルーティンを複製し、複製の作成の操作を作る。
pub fn copy(
    source: &Routine,
    new_id: &str,
    position: i64,
    now: &str,
) -> Result<(Routine, Operation), StorageError> {
    let new_exercise_ids = source
        .exercises
        .iter()
        .map(|_| new_uuid())
        .collect::<Result<Vec<_>, _>>()?;
    let duplicate = source
        .duplicate(new_id, position, &new_exercise_ids, now)
        .ok_or_else(|| StorageError::InvalidOperation("the routine cannot be copied".to_owned()))?;
    let payload = RoutineCopyPayload {
        routine_id: source.id.clone(),
        new_routine_id: duplicate.id.clone(),
        name: duplicate.name.clone(),
        position: duplicate.position,
        exercises: exercise_payloads(&duplicate.exercises),
    };
    let operation = operation(
        &duplicate.id,
        OperationKind::RoutineCopy,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )?;
    Ok((duplicate, operation))
}

fn exercise_payloads(exercises: &[RoutineExercise]) -> Vec<RoutineExercisePayload> {
    exercises
        .iter()
        .map(|exercise| RoutineExercisePayload {
            routine_exercise_id: exercise.id.clone(),
            exercise_id: exercise.exercise_id.clone(),
            snapshot: exercise.snapshot.clone(),
            position: exercise.position,
            target: exercise.target.clone(),
        })
        .collect()
}

fn operation(
    routine_id: &str,
    kind: OperationKind,
    payload: serde_json::Value,
) -> Result<Operation, StorageError> {
    Ok(Operation {
        operation_id: new_uuid()?,
        training_id: routine_id.to_owned(),
        sequence: 0,
        kind,
        parent_operation_id: None,
        payload,
    })
}
