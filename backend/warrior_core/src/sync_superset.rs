//! スーパーセットの組の作成、削除、並べ替えの本文の検証。
//!
//! 対応する要求: `REQ-SET-005`。
//! `warrior_core::sync` の操作の検証から呼ぶ。組の ID が UUID v4 であること、
//! 種目の並びが 2 つ以上であること、同じ種目を含まないことを確かめる。組の種目が
//! 同じトレーニングに属するかは、サーバーが保存の前に確かめる。

use crate::ids::is_valid_uuid_v4;
use crate::sync::{
    Operation, SupersetCreatePayload, SupersetDeletePayload, SupersetReorderPayload, SyncError,
    ValidatedPayload,
};

/// スーパーセットの組の作成の本文を検証する。
pub(crate) fn validate_create(
    operation: &Operation,
    payload: SupersetCreatePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.superset_id) {
        return Err(SyncError::InvalidSupersetId);
    }
    validate_member_ids(&payload.member_ids)?;
    Ok(ValidatedPayload::SupersetCreate(payload))
}

/// スーパーセットの組の削除の本文を検証する。
pub(crate) fn validate_delete(
    operation: &Operation,
    payload: SupersetDeletePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.superset_id) {
        return Err(SyncError::InvalidSupersetId);
    }
    Ok(ValidatedPayload::SupersetDelete(payload))
}

/// スーパーセットの組の種目の並べ替えの本文を検証する。
pub(crate) fn validate_reorder(
    operation: &Operation,
    payload: SupersetReorderPayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.superset_id) {
        return Err(SyncError::InvalidSupersetId);
    }
    validate_member_ids(&payload.member_ids)?;
    Ok(ValidatedPayload::SupersetReorder(payload))
}

fn validate_member_ids(member_ids: &[String]) -> Result<(), SyncError> {
    if member_ids.len() < 2 {
        return Err(SyncError::EmptySuperset);
    }
    let mut seen = Vec::with_capacity(member_ids.len());
    for id in member_ids {
        if !is_valid_uuid_v4(id) || seen.contains(id) {
            return Err(SyncError::InvalidSupersetMembers);
        }
        seen.push(id.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{validate_create, validate_delete, validate_reorder};
    use crate::sync::{Operation, OperationKind, SyncError, ValidatedPayload};

    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
    const SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";
    const FIRST_MEMBER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3304";
    const SECOND_MEMBER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3305";
    const OTHER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3306";

    fn operation(kind: OperationKind, payload: Value) -> Operation {
        Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 2,
            kind,
            parent_operation_id: None,
            payload,
        }
    }

    fn create_operation(member_ids: &[&str]) -> Operation {
        operation(
            OperationKind::SupersetCreate,
            json!({
                "training_id": TRAINING_ID,
                "superset_id": SUPERSET_ID,
                "member_ids": member_ids
            }),
        )
    }

    fn create_payload(operation: &Operation) -> crate::sync::SupersetCreatePayload {
        serde_json::from_value(operation.payload.clone()).expect("a create payload")
    }

    fn delete_operation() -> Operation {
        operation(
            OperationKind::SupersetDelete,
            json!({
                "training_id": TRAINING_ID,
                "superset_id": SUPERSET_ID
            }),
        )
    }

    fn delete_payload(operation: &Operation) -> crate::sync::SupersetDeletePayload {
        serde_json::from_value(operation.payload.clone()).expect("a delete payload")
    }

    fn reorder_operation(member_ids: &[&str]) -> Operation {
        operation(
            OperationKind::SupersetReorder,
            json!({
                "training_id": TRAINING_ID,
                "superset_id": SUPERSET_ID,
                "member_ids": member_ids
            }),
        )
    }

    fn reorder_payload(operation: &Operation) -> crate::sync::SupersetReorderPayload {
        serde_json::from_value(operation.payload.clone()).expect("a reorder payload")
    }

    #[test]
    fn a_valid_create_keeps_the_members_in_order() {
        let operation = create_operation(&[FIRST_MEMBER_ID, SECOND_MEMBER_ID]);
        let validated = validate_create(&operation, create_payload(&operation))
            .expect("a valid superset create");
        let ValidatedPayload::SupersetCreate(payload) = validated else {
            panic!("the payload must be a superset create");
        };
        assert_eq!(payload.superset_id, SUPERSET_ID);
        assert_eq!(
            payload.member_ids,
            vec![FIRST_MEMBER_ID.to_owned(), SECOND_MEMBER_ID.to_owned()]
        );
    }

    #[test]
    fn a_group_with_fewer_than_two_members_is_rejected() {
        let operation = create_operation(&[]);
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::EmptySuperset)
        );

        let operation = create_operation(&[FIRST_MEMBER_ID]);
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::EmptySuperset)
        );
    }

    #[test]
    fn a_duplicate_or_non_uuid_member_is_rejected() {
        let duplicated = create_operation(&[FIRST_MEMBER_ID, FIRST_MEMBER_ID]);
        assert_eq!(
            validate_create(&duplicated, create_payload(&duplicated)),
            Err(SyncError::InvalidSupersetMembers)
        );

        let named = create_operation(&[FIRST_MEMBER_ID, "bench-press"]);
        assert_eq!(
            validate_create(&named, create_payload(&named)),
            Err(SyncError::InvalidSupersetMembers)
        );
    }

    #[test]
    fn a_superset_id_must_be_a_uuid_v4() {
        let mut operation = create_operation(&[FIRST_MEMBER_ID, SECOND_MEMBER_ID]);
        operation.payload["superset_id"] = json!("superset-1");
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::InvalidSupersetId)
        );
    }

    #[test]
    fn a_payload_with_a_different_training_is_rejected() {
        let mut operation = create_operation(&[FIRST_MEMBER_ID, SECOND_MEMBER_ID]);
        operation.payload["training_id"] = json!(OTHER_ID);
        assert_eq!(
            validate_create(&operation, create_payload(&operation)),
            Err(SyncError::PayloadIdMismatch)
        );
    }

    #[test]
    fn a_valid_delete_passes() {
        let operation = delete_operation();
        let validated =
            validate_delete(&operation, delete_payload(&operation)).expect("a valid delete");
        let ValidatedPayload::SupersetDelete(payload) = validated else {
            panic!("the payload must be a superset delete");
        };
        assert_eq!(payload.superset_id, SUPERSET_ID);
    }

    #[test]
    fn a_valid_reorder_keeps_the_new_order() {
        let operation = reorder_operation(&[SECOND_MEMBER_ID, FIRST_MEMBER_ID]);
        let validated =
            validate_reorder(&operation, reorder_payload(&operation)).expect("a valid reorder");
        let ValidatedPayload::SupersetReorder(payload) = validated else {
            panic!("the payload must be a superset reorder");
        };
        assert_eq!(
            payload.member_ids,
            vec![SECOND_MEMBER_ID.to_owned(), FIRST_MEMBER_ID.to_owned()]
        );
    }

    #[test]
    fn a_reorder_with_fewer_than_two_members_is_rejected() {
        let operation = reorder_operation(&[]);
        assert_eq!(
            validate_reorder(&operation, reorder_payload(&operation)),
            Err(SyncError::EmptySuperset)
        );

        let operation = reorder_operation(&[FIRST_MEMBER_ID]);
        assert_eq!(
            validate_reorder(&operation, reorder_payload(&operation)),
            Err(SyncError::EmptySuperset)
        );
    }
}
