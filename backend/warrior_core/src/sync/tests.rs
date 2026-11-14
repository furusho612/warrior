//! 端末の送信待ちの列と同期の操作の検証の例示テスト。
//!
//! 対応する要求: `REQ-NFR-005`。

use serde_json::json;

use super::{
    discard, mark_permanent_failure, mark_sending, mark_success, mark_transient_failure,
    next_sendable, pending_update, recover_interrupted, retry, unresolved_create, unsaved_count,
    validate_operation, Operation, OperationKind, OutboxEntry, OutboxState, SessionRecord,
    SyncError, MEMO_MAX_CHARS,
};

const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";
const PARENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3304";
const CHILD_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3305";

fn create_operation() -> Operation {
    Operation {
        operation_id: OPERATION_ID.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 1,
        kind: OperationKind::TrainingSessionCreate,
        parent_operation_id: None,
        payload: json!({
            "id": TRAINING_ID,
            "date": "2026-11-12",
            "started_at": "2026-11-12T03:00:00Z",
            "memo": ""
        }),
    }
}

fn update_operation(operation_id: &str, sequence: u64) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence,
        kind: OperationKind::TrainingSessionUpdate,
        parent_operation_id: None,
        payload: json!({
            "date": "2026-11-12",
            "started_at": "2026-11-12T03:00:00Z",
            "memo": "更新"
        }),
    }
}

fn entry(operation: Operation, state: OutboxState) -> OutboxEntry {
    OutboxEntry {
        operation,
        state,
        attempts: 0,
        attempted: false,
    }
}

#[test]
fn validates_a_create_operation() {
    let validated = validate_operation(&create_operation()).expect("valid create");
    assert_eq!(validated.operation.operation_id, OPERATION_ID);
}

#[test]
fn rejects_a_create_with_a_different_payload_id() {
    let mut operation = create_operation();
    operation.payload["id"] = json!(UPDATE_ID);
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::PayloadIdMismatch)
    );
}

#[test]
fn rejects_a_date_that_does_not_match_the_started_at() {
    let mut operation = create_operation();
    operation.payload["date"] = json!("2026-11-13");
    assert_eq!(validate_operation(&operation), Err(SyncError::DateMismatch));
}

#[test]
fn rejects_a_memo_over_the_limit() {
    let mut operation = create_operation();
    operation.payload["memo"] = json!("あ".repeat(MEMO_MAX_CHARS + 1));
    assert_eq!(validate_operation(&operation), Err(SyncError::MemoTooLong));
}

#[test]
fn accepts_an_empty_source_without_ids() {
    let mut operation = create_operation();
    operation.payload["source_kind"] = json!("empty");
    operation.payload["routine_id"] = json!(null);
    operation.payload["program_day_id"] = json!(null);
    operation.payload["custom_program_day_id"] = json!(null);
    assert!(validate_operation(&operation).is_ok());
}

#[test]
fn rejects_an_empty_source_with_a_routine_id() {
    let mut operation = create_operation();
    operation.payload["source_kind"] = json!("empty");
    operation.payload["routine_id"] = json!(PARENT_ID);
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::SourceIdUnexpected)
    );
}

#[test]
fn rejects_an_unknown_source_kind() {
    let mut operation = create_operation();
    operation.payload["source_kind"] = json!("unknown");
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidPayload)
    );
}

#[test]
fn a_legacy_session_record_deserializes_with_the_new_fields_defaulted() {
    let record: SessionRecord = serde_json::from_value(json!({
        "id": TRAINING_ID,
        "date": "2026-11-12",
        "started_at": "2026-11-12T03:00:00Z",
        "memo": "旧い記録",
        "created_at": "2026-11-12T03:00:00Z",
        "updated_at": "2026-11-12T03:00:00Z"
    }))
    .expect("a legacy record must deserialize");
    assert_eq!(record.source_kind, super::SessionSource::Empty);
    assert_eq!(record.routine_id, None);
    assert_eq!(record.finished_at, None);
    assert_eq!(record.duration_seconds, None);
    assert_eq!(record.total_weight_kg, None);
    assert_eq!(record.exercise_count, None);
    assert_eq!(record.set_count, None);
    assert!(record.personal_bests.is_empty());
}

#[test]
fn rejects_a_sequence_below_one() {
    let mut operation = create_operation();
    operation.sequence = 0;
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidSequence)
    );
}

#[test]
fn rejects_an_operation_id_that_is_not_a_uuid_v4() {
    let mut operation = create_operation();
    operation.operation_id = "not-a-uuid".to_owned();
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidOperationId)
    );
}

#[test]
fn rejects_a_training_id_that_is_not_a_uuid_v4() {
    let mut operation = create_operation();
    operation.training_id = "not-a-uuid".to_owned();
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidTrainingId)
    );
}

#[test]
fn rejects_an_operation_that_names_itself_as_the_parent() {
    let mut operation = create_operation();
    operation.parent_operation_id = Some(OPERATION_ID.to_owned());
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidParent)
    );
}

#[test]
fn rejects_a_started_at_that_is_not_an_iso8601_utc_time() {
    let mut operation = create_operation();
    operation.payload["started_at"] = json!("2026-11-12 03:00:00");
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidStartedAt)
    );
}

#[test]
fn accepts_a_memo_at_the_limit() {
    let mut operation = create_operation();
    operation.payload["memo"] = json!("あ".repeat(MEMO_MAX_CHARS));
    assert!(validate_operation(&operation).is_ok());
}

#[test]
fn rejects_an_unknown_payload_field() {
    let mut operation = create_operation();
    operation.payload["unexpected"] = json!(true);
    assert_eq!(
        validate_operation(&operation),
        Err(SyncError::InvalidPayload)
    );
}

#[test]
fn sends_the_oldest_pending_operation_of_a_training() {
    let entries = vec![
        entry(update_operation(UPDATE_ID, 2), OutboxState::Pending),
        entry(create_operation(), OutboxState::Pending),
    ];
    assert_eq!(
        next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
        Some(OPERATION_ID)
    );
}

#[test]
fn blocks_a_child_until_the_parent_leaves_the_outbox() {
    let mut child = update_operation(CHILD_ID, 2);
    child.parent_operation_id = Some(OPERATION_ID.to_owned());
    let mut entries = vec![
        entry(create_operation(), OutboxState::Pending),
        entry(child, OutboxState::Pending),
    ];
    assert_eq!(
        next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
        Some(OPERATION_ID)
    );

    mark_sending(&mut entries, OPERATION_ID);
    assert_eq!(entries[1].state, OutboxState::Blocked);
    assert!(mark_success(&mut entries, OPERATION_ID));
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert_eq!(
        next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
        Some(CHILD_ID)
    );
}

#[test]
fn a_failed_operation_does_not_stop_later_operations() {
    let entries = vec![
        entry(create_operation(), OutboxState::Failed),
        entry(update_operation(UPDATE_ID, 2), OutboxState::Pending),
    ];
    assert_eq!(
        next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
        Some(UPDATE_ID)
    );
}

#[test]
fn discarding_a_parent_removes_descendants() {
    let mut child = update_operation(CHILD_ID, 2);
    child.parent_operation_id = Some(OPERATION_ID.to_owned());
    let mut grandchild = update_operation(PARENT_ID, 3);
    grandchild.parent_operation_id = Some(CHILD_ID.to_owned());
    let mut entries = vec![
        entry(create_operation(), OutboxState::Failed),
        entry(child, OutboxState::Blocked),
        entry(grandchild, OutboxState::Blocked),
    ];

    let removed = discard(&mut entries, OPERATION_ID);
    assert_eq!(removed.len(), 3);
    assert!(entries.is_empty());
}

#[test]
fn pending_update_uses_only_the_latest_pending_or_blocked_update() {
    let older = entry(update_operation(UPDATE_ID, 2), OutboxState::Pending);
    let mut attempted = entry(update_operation(PARENT_ID, 3), OutboxState::Pending);
    attempted.attempted = true;
    let newer = entry(update_operation(CHILD_ID, 4), OutboxState::Blocked);
    let entries = vec![older.clone(), attempted.clone(), newer];
    assert_eq!(
        pending_update(&entries, TRAINING_ID).map(|entry| entry.operation.operation_id.as_str()),
        Some(CHILD_ID)
    );

    let entries = vec![older, attempted];
    assert_eq!(pending_update(&entries, TRAINING_ID), None);
}

#[test]
fn an_unresolved_create_is_found_even_after_it_fails() {
    let mut create = entry(create_operation(), OutboxState::Failed);
    create.attempted = true;
    let entries = vec![create];
    assert_eq!(
        unresolved_create(&entries, TRAINING_ID).map(|entry| entry.operation.operation_id.as_str()),
        Some(OPERATION_ID)
    );
}

#[test]
fn recovering_an_interrupted_send_keeps_the_attempt_mark() {
    let mut entries = vec![entry(create_operation(), OutboxState::Pending)];
    assert!(mark_sending(&mut entries, OPERATION_ID));
    assert!(recover_interrupted(&mut entries));
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert!(entries[0].attempted);
    assert!(!recover_interrupted(&mut entries));
}

#[test]
fn a_transient_failure_returns_to_pending_and_counts_an_attempt() {
    let mut entries = vec![entry(create_operation(), OutboxState::Pending)];
    assert!(mark_sending(&mut entries, OPERATION_ID));
    assert!(mark_transient_failure(&mut entries, OPERATION_ID));
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert_eq!(entries[0].attempts, 1);
}

#[test]
fn a_permanent_failure_can_be_retried_or_discarded() {
    let mut entries = vec![entry(create_operation(), OutboxState::Pending)];
    assert!(mark_sending(&mut entries, OPERATION_ID));
    assert!(mark_permanent_failure(&mut entries, OPERATION_ID));
    assert_eq!(entries[0].state, OutboxState::Failed);
    assert!(retry(&mut entries, OPERATION_ID));
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert_eq!(entries[0].attempts, 0);
}

#[test]
fn unsaved_counts_exclude_failed_operations() {
    let entries = vec![
        entry(create_operation(), OutboxState::Pending),
        entry(update_operation(UPDATE_ID, 2), OutboxState::Failed),
    ];
    assert_eq!(unsaved_count(&entries), 1);
    assert_eq!(super::failed_count(&entries), 1);
}
