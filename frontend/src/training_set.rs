//! セットの行と段、送信待ちの列に積む操作の組み立て。
//!
//! セットの追加、更新、削除、並べ替え、完了の操作は端末で作り、サーバーが検証して
//! 保存する (ADR-002)。記録した時点の体重はセットの行に持ち、段の重量は自重種目では
//! セットごとの加重とする。ドロップセットは 1 つのセットに複数の段を持つ。
//! 操作の ID と順序は保存のときに `frontend/src/storage` が決める。

use warrior_core::exercise::GripSelection;
use warrior_core::set_calc::SetKind;
use warrior_core::sync::{
    Operation, OperationKind, SetCompletePayload, SetCreatePayload, SetDeletePayload,
    SetReorderPayload, SetSegmentPayload, SetUpdatePayload,
};
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

use crate::ids::{new_uuid, now_iso};
use crate::storage::StorageError;

/// セットの 1 つの段の入力の値。
#[derive(Debug, Clone, PartialEq)]
pub struct SetSegmentValues {
    /// 重量 (kg)。自重種目は加重である。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
}

impl SetSegmentValues {
    /// 段に記録する値があるか。
    pub fn has_values(&self) -> bool {
        self.reps > 0 || self.duration_seconds.is_some_and(|duration| duration > 0)
    }
}

/// セットの入力の値。
#[derive(Debug, Clone, PartialEq)]
pub struct SetValues {
    /// セット種別。
    pub kind: SetKind,
    /// 段の入力。並び順である。
    pub segments: Vec<SetSegmentValues>,
    /// 完了しているか。値が無いときは完了にしない。
    pub completed: bool,
    /// 記録した時点の体重 (kg)。身体情報が無いときは None。
    pub body_weight_kg: Option<f64>,
    /// RPE。ドロップセットも親セットに 1 つ持つ。
    pub rpe: Option<f64>,
    /// グリップ。選ばない種目は None。
    pub grip: Option<GripSelection>,
}

impl SetValues {
    /// すべての段に記録する値があるか。
    pub fn has_values(&self) -> bool {
        !self.segments.is_empty() && self.segments.iter().all(SetSegmentValues::has_values)
    }

    /// 値を保ったまま完了できるか。
    pub fn completed_with_values(&self) -> bool {
        self.completed && self.has_values()
    }
}

/// 端末に保存する 1 つのセットと段。
#[derive(Debug, Clone, PartialEq)]
pub struct SetRows {
    /// セットの行。
    pub set: SetRecordRow,
    /// セットの段。並び順である。
    pub segments: Vec<SetSegmentRecord>,
}

/// セットの追加の行と操作を作る。
pub fn create(
    training_id: &str,
    training_exercise_id: &str,
    position: i64,
    values: &SetValues,
) -> Result<(SetRows, Operation), StorageError> {
    if values.segments.is_empty() {
        return Err(StorageError::InvalidOperation(
            "the set has no segments".to_owned(),
        ));
    }
    let now = now_iso();
    let set_id = new_uuid()?;
    let segment_ids = values
        .segments
        .iter()
        .map(|_| new_uuid())
        .collect::<Result<Vec<_>, _>>()?;
    let segments = segment_ids
        .iter()
        .zip(&values.segments)
        .enumerate()
        .map(|(index, (segment_id, values))| {
            segment_record(segment_id, &set_id, position_of(index), values, &now)
        })
        .collect::<Vec<_>>();
    let set = SetRecordRow {
        id: set_id.clone(),
        training_exercise_id: training_exercise_id.to_owned(),
        position,
        kind: values.kind.as_str().to_owned(),
        completed: 0,
        rpe: values.rpe,
        grip: grip_json(values.grip),
        body_weight_kg: values.body_weight_kg,
        recorded_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let payload = SetCreatePayload {
        training_id: training_id.to_owned(),
        training_exercise_id: training_exercise_id.to_owned(),
        set_id,
        position,
        kind: values.kind,
        rpe: values.rpe,
        grip: values.grip,
        body_weight_kg: values.body_weight_kg,
        segments: payload_segments(&segment_ids, &values.segments),
    };
    let operation = operation(
        training_id,
        OperationKind::SetCreate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )?;
    Ok((SetRows { set, segments }, operation))
}

/// セットの更新の行と操作を作る。段は全置換である。
pub fn update(
    training_id: &str,
    set: &SetRecordRow,
    segments: &[SetSegmentRecord],
    values: &SetValues,
) -> Result<(SetRows, Operation), StorageError> {
    if segments.len() != values.segments.len() {
        return Err(StorageError::InvalidOperation(
            "the update must keep the number of segments".to_owned(),
        ));
    }
    let now = now_iso();
    let updated_set = SetRecordRow {
        kind: values.kind.as_str().to_owned(),
        completed: if values.completed_with_values() { 1 } else { 0 },
        rpe: values.rpe,
        grip: grip_json(values.grip),
        body_weight_kg: values.body_weight_kg,
        updated_at: now.clone(),
        ..set.clone()
    };
    let updated_segments = segments
        .iter()
        .zip(&values.segments)
        .enumerate()
        .map(|(index, (segment, values))| {
            updated_segment_record(segment, position_of(index), values, &now)
        })
        .collect::<Vec<_>>();
    let segment_ids = updated_segments
        .iter()
        .map(|segment| segment.id.clone())
        .collect::<Vec<_>>();
    let payload = SetUpdatePayload {
        training_id: training_id.to_owned(),
        set_id: set.id.clone(),
        position: updated_set.position,
        kind: values.kind,
        completed: values.completed_with_values(),
        rpe: values.rpe,
        grip: values.grip,
        body_weight_kg: values.body_weight_kg,
        segments: payload_segments(&segment_ids, &values.segments),
    };
    let operation = operation(
        training_id,
        OperationKind::SetUpdate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )?;
    Ok((
        SetRows {
            set: updated_set,
            segments: updated_segments,
        },
        operation,
    ))
}

/// セットの削除の操作を作る。
pub fn delete(training_id: &str, set_id: &str) -> Result<Operation, StorageError> {
    let payload = SetDeletePayload {
        training_id: training_id.to_owned(),
        set_id: set_id.to_owned(),
    };
    operation(
        training_id,
        OperationKind::SetDelete,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// セットの並べ替えの操作を作る。
pub fn reorder(
    training_id: &str,
    training_exercise_id: &str,
    ordered_ids: &[String],
) -> Result<Operation, StorageError> {
    let payload = SetReorderPayload {
        training_id: training_id.to_owned(),
        training_exercise_id: training_exercise_id.to_owned(),
        ordered_ids: ordered_ids.to_vec(),
    };
    operation(
        training_id,
        OperationKind::SetReorder,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// セットの完了の状態の変更の行と操作を作る。
pub fn complete(
    training_id: &str,
    set: &SetRecordRow,
    completed: bool,
) -> Result<(SetRecordRow, Operation), StorageError> {
    let now = now_iso();
    let updated = SetRecordRow {
        completed: if completed { 1 } else { 0 },
        updated_at: now,
        ..set.clone()
    };
    let payload = SetCompletePayload {
        training_id: training_id.to_owned(),
        set_id: set.id.clone(),
        completed,
    };
    let operation = operation(
        training_id,
        OperationKind::SetComplete,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )?;
    Ok((updated, operation))
}

fn segment_record(
    segment_id: &str,
    set_id: &str,
    position: i64,
    values: &SetSegmentValues,
    now: &str,
) -> SetSegmentRecord {
    SetSegmentRecord {
        id: segment_id.to_owned(),
        set_id: set_id.to_owned(),
        position,
        weight_kg: Some(values.weight_kg),
        reps: Some(i64::from(values.reps)),
        momentum_reps: 0,
        assisted_reps: 0,
        duration_seconds: values.duration_seconds.map(i64::from),
        left_weight_kg: None,
        left_reps: None,
        left_duration_seconds: None,
        right_weight_kg: None,
        right_reps: None,
        right_duration_seconds: None,
        created_at: now.to_owned(),
        updated_at: now.to_owned(),
    }
}

fn updated_segment_record(
    segment: &SetSegmentRecord,
    position: i64,
    values: &SetSegmentValues,
    now: &str,
) -> SetSegmentRecord {
    SetSegmentRecord {
        position,
        weight_kg: Some(values.weight_kg),
        reps: Some(i64::from(values.reps)),
        momentum_reps: 0,
        assisted_reps: 0,
        duration_seconds: values.duration_seconds.map(i64::from),
        left_weight_kg: None,
        left_reps: None,
        left_duration_seconds: None,
        right_weight_kg: None,
        right_reps: None,
        right_duration_seconds: None,
        updated_at: now.to_owned(),
        ..segment.clone()
    }
}

fn payload_segments(segment_ids: &[String], values: &[SetSegmentValues]) -> Vec<SetSegmentPayload> {
    segment_ids
        .iter()
        .zip(values)
        .enumerate()
        .map(|(index, (segment_id, values))| SetSegmentPayload {
            segment_id: segment_id.clone(),
            position: position_of(index),
            weight_kg: values.weight_kg,
            reps: values.reps,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: values.duration_seconds,
            left: None,
            right: None,
        })
        .collect()
}

fn position_of(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1)
}

fn grip_json(grip: Option<GripSelection>) -> Option<String> {
    grip.and_then(|grip| serde_json::to_string(&grip).ok())
}

fn operation(
    training_id: &str,
    kind: OperationKind,
    payload: serde_json::Value,
) -> Result<Operation, StorageError> {
    Ok(Operation {
        operation_id: new_uuid()?,
        training_id: training_id.to_owned(),
        sequence: 0,
        kind,
        parent_operation_id: None,
        payload,
    })
}
