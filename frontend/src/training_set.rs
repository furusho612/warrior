//! セットの行と、送信待ちの列に積む操作の組み立て。
//!
//! セットの追加、更新、削除、並べ替え、完了の操作は端末で作り、サーバーが検証して
//! 保存する (ADR-002)。記録した時点の体重はセットの行に持ち、段の重量は自重種目では
//! セットごとの加重とする。操作の ID と順序は保存のときに `frontend/src/storage` が決める。

use warrior_core::exercise::GripSelection;
use warrior_core::set_calc::SetKind;
use warrior_core::sync::{
    Operation, OperationKind, SetCompletePayload, SetCreatePayload, SetDeletePayload,
    SetReorderPayload, SetSegmentPayload, SetUpdatePayload,
};
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord};

use crate::ids::{new_uuid, now_iso};
use crate::storage::StorageError;

/// セットの入力の値。
#[derive(Debug, Clone, PartialEq)]
pub struct SetValues {
    /// セット種別。
    pub kind: SetKind,
    /// 重量 (kg)。自重種目はセットごとの加重である。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
    /// 完了しているか。値が無いときは完了にしない。
    pub completed: bool,
    /// 記録した時点の体重 (kg)。身体情報が無いときは None。
    pub body_weight_kg: Option<f64>,
    /// RPE。
    pub rpe: Option<f64>,
    /// グリップ。選ばない種目は None。
    pub grip: Option<GripSelection>,
}

impl SetValues {
    /// セットに記録する値があるか。
    pub fn has_values(&self) -> bool {
        self.reps > 0 || self.duration_seconds.is_some_and(|duration| duration > 0)
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
    /// セットの段。この版では 1 つだけである。
    pub segment: SetSegmentRecord,
}

/// セットの追加の行と操作を作る。
pub fn create(
    training_id: &str,
    training_exercise_id: &str,
    position: i64,
    values: &SetValues,
) -> Result<(SetRows, Operation), StorageError> {
    let now = now_iso();
    let set_id = new_uuid()?;
    let segment_id = new_uuid()?;
    let segment = segment_record(&segment_id, &set_id, values, &now);
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
        segments: vec![segment_payload(&segment_id, values)],
    };
    let operation = operation(
        training_id,
        OperationKind::SetCreate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )?;
    Ok((SetRows { set, segment }, operation))
}

/// セットの更新の行と操作を作る。段は全置換である。
pub fn update(
    training_id: &str,
    set: &SetRecordRow,
    segment: &SetSegmentRecord,
    values: &SetValues,
) -> Result<(SetRows, Operation), StorageError> {
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
    let updated_segment = SetSegmentRecord {
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
        updated_at: now.clone(),
        ..segment.clone()
    };
    let payload = SetUpdatePayload {
        training_id: training_id.to_owned(),
        set_id: set.id.clone(),
        position: updated_set.position,
        kind: values.kind,
        completed: values.completed_with_values(),
        rpe: values.rpe,
        grip: values.grip,
        body_weight_kg: values.body_weight_kg,
        segments: vec![segment_payload(&segment.id, values)],
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
            segment: updated_segment,
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
    values: &SetValues,
    now: &str,
) -> SetSegmentRecord {
    SetSegmentRecord {
        id: segment_id.to_owned(),
        set_id: set_id.to_owned(),
        position: 1,
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

fn segment_payload(segment_id: &str, values: &SetValues) -> SetSegmentPayload {
    SetSegmentPayload {
        segment_id: segment_id.to_owned(),
        position: 1,
        weight_kg: values.weight_kg,
        reps: values.reps,
        momentum_reps: 0,
        assisted_reps: 0,
        duration_seconds: values.duration_seconds,
        left: None,
        right: None,
    }
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
