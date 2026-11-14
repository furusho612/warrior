//! スーパーセットの組の行と、送信待ちの列に積む操作の組み立て。
//!
//! 対応する要求: `REQ-SET-005`。
//! 組は種目の行の ID を順序付きで持ち、端末で作った UUID v4 を ID にする (ADR-002)。
//! 操作の順序は保存のときに `frontend/src/storage` が決める。

use warrior_core::superset::SupersetMembers;
use warrior_core::sync::{
    Operation, OperationKind, SupersetCreatePayload, SupersetDeletePayload, SupersetReorderPayload,
};
use warrior_core::training_record::{SupersetMemberRecord, SupersetRecord};

use crate::ids::{new_uuid, now_iso};
use crate::storage::StorageError;

/// 組の作成の行と操作を作る。
pub fn create(
    training_id: &str,
    position: i64,
    members: &SupersetMembers,
) -> Result<(SupersetRecord, Operation), StorageError> {
    let now = now_iso();
    let superset_id = new_uuid()?;
    let record = SupersetRecord {
        id: superset_id.clone(),
        training_id: training_id.to_owned(),
        position,
        members: member_records(&superset_id, members),
        created_at: now.clone(),
        updated_at: now,
    };
    let payload = SupersetCreatePayload {
        training_id: training_id.to_owned(),
        superset_id,
        member_ids: members.as_slice().to_vec(),
    };
    let operation = operation(
        training_id,
        OperationKind::SupersetCreate,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )?;
    Ok((record, operation))
}

/// 組の削除の操作を作る。
pub fn delete(training_id: &str, superset_id: &str) -> Result<Operation, StorageError> {
    let payload = SupersetDeletePayload {
        training_id: training_id.to_owned(),
        superset_id: superset_id.to_owned(),
    };
    operation(
        training_id,
        OperationKind::SupersetDelete,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

/// 組の種目の並べ替えの操作を作る。
pub fn reorder(
    training_id: &str,
    superset_id: &str,
    members: &SupersetMembers,
) -> Result<Operation, StorageError> {
    let payload = SupersetReorderPayload {
        training_id: training_id.to_owned(),
        superset_id: superset_id.to_owned(),
        member_ids: members.as_slice().to_vec(),
    };
    operation(
        training_id,
        OperationKind::SupersetReorder,
        serde_json::to_value(payload)
            .map_err(|error| StorageError::Serialize(error.to_string()))?,
    )
}

fn member_records(superset_id: &str, members: &SupersetMembers) -> Vec<SupersetMemberRecord> {
    members
        .as_slice()
        .iter()
        .enumerate()
        .map(|(index, training_exercise_id)| SupersetMemberRecord {
            superset_id: superset_id.to_owned(),
            training_exercise_id: training_exercise_id.clone(),
            position: position_of(index),
        })
        .collect()
}

fn position_of(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1)
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
