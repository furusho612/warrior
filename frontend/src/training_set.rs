//! セットの行と段、送信待ちの列に積む操作の組み立て。
//!
//! セットの追加、更新、削除、並べ替え、完了の操作は端末で作り、サーバーが検証して
//! 保存する (ADR-002)。記録した時点の体重はセットの行に持ち、段の重量は自重種目では
//! セットごとの加重とする。ドロップセットは 1 つのセットに複数の段を持つ。
//! 段は反動と補助の回数を持ち、セットは RPE を持つ。RPE と回数の内訳は
//! `warrior_core::rpe` の規則で検証する。時間で記録する段は時間を持ち、重量と回数を
//! 保存しない。
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

/// セットの 1 つの段の左右の値。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentSideValues {
    /// 重量 (kg)。自重種目は加重である。重量欄が空のときは None。
    pub weight_kg: Option<f64>,
    /// 回数。
    pub reps: u32,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
}

/// セットの 1 つの段の入力の値。
#[derive(Debug, Clone, PartialEq)]
pub struct SetSegmentValues {
    /// 重量 (kg)。自重種目は加重である。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 反動を使った回数。
    pub momentum_reps: u32,
    /// 補助を受けた回数。
    pub assisted_reps: u32,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
    /// 左の値。左右別の種目だけが持つ。
    pub left: Option<SegmentSideValues>,
    /// 右の値。左右別の種目だけが持つ。
    pub right: Option<SegmentSideValues>,
}

impl SetSegmentValues {
    /// 段が完了に必要な値を持つか。
    ///
    /// 左右別の段は、左右の両方に重量と回数、または左右の両方に時間があることを求める。
    pub fn is_complete(&self) -> bool {
        match (self.left.as_ref(), self.right.as_ref()) {
            (Some(left), Some(right)) => {
                (left.weight_kg.is_some()
                    && left.reps > 0
                    && right.weight_kg.is_some()
                    && right.reps > 0)
                    || (left.duration_seconds.is_some_and(|duration| duration > 0)
                        && right.duration_seconds.is_some_and(|duration| duration > 0))
            }
            (None, None) => {
                self.reps > 0 || self.duration_seconds.is_some_and(|duration| duration > 0)
            }
            (Some(_), None) | (None, Some(_)) => false,
        }
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
    /// セットが完了に必要な値を持つか。すべての段が完了していることを求める。
    pub fn is_complete(&self) -> bool {
        !self.segments.is_empty() && self.segments.iter().all(SetSegmentValues::is_complete)
    }

    /// 値を保ったまま完了できるか。
    pub fn completed_with_values(&self) -> bool {
        self.completed && self.is_complete()
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
    validate(values)?;
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
    validate(values)?;
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
    let fields = segment_fields(values);
    SetSegmentRecord {
        id: segment_id.to_owned(),
        set_id: set_id.to_owned(),
        position,
        weight_kg: fields.weight_kg,
        reps: fields.reps,
        momentum_reps: fields.momentum_reps,
        assisted_reps: fields.assisted_reps,
        duration_seconds: fields.duration_seconds,
        left_weight_kg: fields.left_weight_kg,
        left_reps: fields.left_reps,
        left_duration_seconds: fields.left_duration_seconds,
        right_weight_kg: fields.right_weight_kg,
        right_reps: fields.right_reps,
        right_duration_seconds: fields.right_duration_seconds,
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
    let fields = segment_fields(values);
    SetSegmentRecord {
        position,
        weight_kg: fields.weight_kg,
        reps: fields.reps,
        momentum_reps: fields.momentum_reps,
        assisted_reps: fields.assisted_reps,
        duration_seconds: fields.duration_seconds,
        left_weight_kg: fields.left_weight_kg,
        left_reps: fields.left_reps,
        left_duration_seconds: fields.left_duration_seconds,
        right_weight_kg: fields.right_weight_kg,
        right_reps: fields.right_reps,
        right_duration_seconds: fields.right_duration_seconds,
        updated_at: now.to_owned(),
        ..segment.clone()
    }
}

/// 端末に保存する段の値。時間で記録する段は重量と回数を持たない。
struct SegmentFields {
    weight_kg: Option<f64>,
    reps: Option<i64>,
    momentum_reps: i64,
    assisted_reps: i64,
    duration_seconds: Option<i64>,
    left_weight_kg: Option<f64>,
    left_reps: Option<i64>,
    left_duration_seconds: Option<i64>,
    right_weight_kg: Option<f64>,
    right_reps: Option<i64>,
    right_duration_seconds: Option<i64>,
}

fn segment_fields(values: &SetSegmentValues) -> SegmentFields {
    let is_time = is_time_segment(values);
    SegmentFields {
        weight_kg: (!is_time).then_some(values.weight_kg),
        reps: (!is_time).then(|| i64::from(values.reps)),
        momentum_reps: if is_time {
            0
        } else {
            i64::from(values.momentum_reps)
        },
        assisted_reps: if is_time {
            0
        } else {
            i64::from(values.assisted_reps)
        },
        duration_seconds: values.duration_seconds.map(i64::from),
        left_weight_kg: if is_time {
            None
        } else {
            values.left.and_then(|side| side.weight_kg)
        },
        left_reps: if is_time {
            None
        } else {
            values.left.map(|side| i64::from(side.reps))
        },
        left_duration_seconds: values
            .left
            .and_then(|side| side.duration_seconds)
            .map(i64::from),
        right_weight_kg: if is_time {
            None
        } else {
            values.right.and_then(|side| side.weight_kg)
        },
        right_reps: if is_time {
            None
        } else {
            values.right.map(|side| i64::from(side.reps))
        },
        right_duration_seconds: values
            .right
            .and_then(|side| side.duration_seconds)
            .map(i64::from),
    }
}

/// 時間の値を持つ段か。端末の行は時間の段に重量と回数を保存しない。
fn is_time_segment(values: &SetSegmentValues) -> bool {
    values.duration_seconds.is_some()
        || values
            .left
            .is_some_and(|side| side.duration_seconds.is_some())
        || values
            .right
            .is_some_and(|side| side.duration_seconds.is_some())
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
            momentum_reps: values.momentum_reps,
            assisted_reps: values.assisted_reps,
            duration_seconds: values.duration_seconds,
            left: values.left.map(side_payload),
            right: values.right.map(side_payload),
        })
        .collect()
}

fn side_payload(values: SegmentSideValues) -> warrior_core::sync::SetSidePayload {
    warrior_core::sync::SetSidePayload {
        weight_kg: values.weight_kg,
        reps: values.reps,
        duration_seconds: values.duration_seconds,
    }
}

fn position_of(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1)
}

fn validate(values: &SetValues) -> Result<(), StorageError> {
    if values
        .rpe
        .is_some_and(|rpe| !warrior_core::rpe::is_valid(rpe))
    {
        return Err(StorageError::InvalidOperation(
            "the RPE is invalid".to_owned(),
        ));
    }
    for segment in &values.segments {
        let used = u64::from(segment.momentum_reps) + u64::from(segment.assisted_reps);
        let over_reps = match (segment.left.as_ref(), segment.right.as_ref()) {
            (Some(left), Some(right)) => {
                used > u64::from(left.reps) || used > u64::from(right.reps)
            }
            _ => used > u64::from(segment.reps),
        };
        if over_reps {
            return Err(StorageError::InvalidOperation(
                "the breakdown exceeds the reps".to_owned(),
            ));
        }
    }
    Ok(())
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
