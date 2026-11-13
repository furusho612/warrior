//! 同期の送信待ちの列の状態遷移の性質テスト。
//!
//! 対応する要求: `REQ-NFR-005`、`REQ-NFR-007`。
//! 同じ操作の再送で列と状態が変わらず、親が解決するまで子を送らず、
//! 親の破棄で子孫が消えることを確かめる。

use proptest::prelude::*;
use serde_json::json;
use warrior_core::sync::{
    discard, mark_sending, mark_success, mark_transient_failure, next_sendable, pending_update,
    recover_interrupted, Operation, OperationKind, OutboxEntry, OutboxState,
};

fn state() -> impl Strategy<Value = OutboxState> {
    prop_oneof![
        Just(OutboxState::Pending),
        Just(OutboxState::Sending),
        Just(OutboxState::Blocked),
        Just(OutboxState::Failed),
    ]
}

fn training_selector() -> impl Strategy<Value = u8> {
    0u8..2
}

fn generated_entries() -> impl Strategy<Value = Vec<OutboxEntry>> {
    prop::collection::vec(
        (
            training_selector(),
            1u64..6,
            prop::option::of(0usize..8),
            state(),
            any::<bool>(),
        ),
        1..8,
    )
    .prop_map(|rows| {
        let count = rows.len();
        rows.into_iter()
            .enumerate()
            .map(
                |(index, (training, sequence, parent, state, attempted))| OutboxEntry {
                    operation: Operation {
                        operation_id: format!("operation-{index}"),
                        training_id: format!("training-{training}"),
                        sequence,
                        kind: OperationKind::TrainingSessionUpdate,
                        parent_operation_id: parent
                            .filter(|parent| *parent != index)
                            .map(|parent| format!("operation-{}", parent % count)),
                        payload: json!({}),
                    },
                    state,
                    attempts: 0,
                    attempted: attempted
                        || matches!(state, OutboxState::Sending | OutboxState::Failed),
                },
            )
            .collect()
    })
}

fn update_entry(operation_id: &str, sequence: u64, attempted: bool) -> OutboxEntry {
    OutboxEntry {
        operation: Operation {
            operation_id: operation_id.to_owned(),
            training_id: "training-1".to_owned(),
            sequence,
            kind: OperationKind::TrainingSessionUpdate,
            parent_operation_id: None,
            payload: json!({}),
        },
        state: OutboxState::Pending,
        attempts: 0,
        attempted,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_missing_operation_cannot_be_marked_successful(entries in generated_entries()) {
        let mut outbox = entries.clone();
        prop_assert!(!mark_success(&mut outbox, "missing-operation"));
        prop_assert_eq!(outbox, entries);
    }

    #[test]
    fn replaying_a_successful_operation_does_not_change_the_outbox(
        entries in generated_entries(),
        candidate in prop_oneof![
            (0usize..8).prop_map(Some),
            Just(None),
        ],
    ) {
        let operation_id = match candidate {
            Some(index) => entries[index % entries.len()].operation.operation_id.clone(),
            None => "missing-operation".to_owned(),
        };
        let present = entries
            .iter()
            .any(|entry| entry.operation.operation_id == operation_id);
        let mut outbox = entries;
        let changed = mark_success(&mut outbox, &operation_id);
        let after_first = outbox.clone();
        let changed_again = mark_success(&mut outbox, &operation_id);
        prop_assert!(!changed_again);
        prop_assert_eq!(outbox, after_first);
        prop_assert_eq!(changed, present);
    }

    #[test]
    fn a_child_is_not_sendable_while_its_parent_is_in_the_outbox(
        parent_state in state(),
        sequence in 1u64..5,
    ) {
        let parent = OutboxEntry {
            operation: Operation {
                operation_id: "parent".to_owned(),
                training_id: "training-1".to_owned(),
                sequence,
                kind: OperationKind::TrainingSessionCreate,
                parent_operation_id: None,
                payload: json!({}),
            },
            state: parent_state,
            attempts: 0,
            attempted: matches!(parent_state, OutboxState::Sending | OutboxState::Failed),
        };
        let child = OutboxEntry {
            operation: Operation {
                operation_id: "child".to_owned(),
                training_id: "training-1".to_owned(),
                sequence: sequence + 1,
                kind: OperationKind::TrainingSessionUpdate,
                parent_operation_id: Some("parent".to_owned()),
                payload: json!({}),
            },
            state: OutboxState::Pending,
            attempts: 0,
            attempted: false,
        };
        let entries = vec![parent, child];
        let sendable = next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str());
        prop_assert_ne!(sendable, Some("child"));
    }

    #[test]
    fn a_failed_parent_keeps_its_child_blocked(sequence in 1u64..5) {
        let mut parent = update_entry("parent", sequence, true);
        parent.operation.kind = OperationKind::TrainingSessionCreate;
        parent.state = OutboxState::Failed;
        let mut child = update_entry("child", sequence + 1, false);
        child.operation.parent_operation_id = Some("parent".to_owned());
        let entries = vec![parent, child];
        prop_assert_eq!(next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()), None);
    }

    #[test]
    fn discarding_a_parent_removes_all_descendants(length in 2usize..6) {
        let entries = (0..length)
            .map(|index| OutboxEntry {
                operation: Operation {
                    operation_id: format!("operation-{index}"),
                    training_id: "training-1".to_owned(),
                    sequence: index as u64 + 1,
                    kind: OperationKind::TrainingSessionUpdate,
                    parent_operation_id: (index > 0).then(|| format!("operation-{}", index - 1)),
                    payload: json!({}),
                },
                state: OutboxState::Pending,
                attempts: 0,
                attempted: false,
            })
            .collect::<Vec<_>>();
        let mut outbox = entries;
        let removed = discard(&mut outbox, "operation-0");
        prop_assert_eq!(removed.len(), length);
        prop_assert!(outbox.is_empty());
    }

    #[test]
    fn the_next_operation_does_not_skip_an_earlier_pending_operation_of_the_training(
        entries in generated_entries(),
    ) {
        let Some(sendable) = next_sendable(&entries) else {
            return Ok(());
        };
        for entry in &entries {
            if entry.operation.training_id == sendable.operation.training_id
                && entry.operation.sequence < sendable.operation.sequence
            {
                prop_assert!(!matches!(
                    entry.state,
                    OutboxState::Pending | OutboxState::Sending
                ));
            }
        }
    }

    #[test]
    fn a_failed_operation_does_not_stop_a_later_independent_operation(
        sequence in 1u64..5,
    ) {
        let failed = OutboxEntry {
            operation: Operation {
                operation_id: "failed".to_owned(),
                training_id: "training-1".to_owned(),
                sequence,
                kind: OperationKind::TrainingSessionCreate,
                parent_operation_id: None,
                payload: json!({}),
            },
            state: OutboxState::Failed,
            attempts: 0,
            attempted: true,
        };
        let later = OutboxEntry {
            operation: Operation {
                operation_id: "later".to_owned(),
                training_id: "training-1".to_owned(),
                sequence: sequence + 1,
                kind: OperationKind::TrainingSessionUpdate,
                parent_operation_id: None,
                payload: json!({}),
            },
            state: OutboxState::Pending,
            attempts: 0,
            attempted: false,
        };
        let entries = vec![failed, later];
        prop_assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some("later")
        );
    }

    #[test]
    fn success_unblocks_the_direct_child(sequence in 1u64..5) {
        let mut parent = update_entry("parent", sequence, false);
        parent.operation.kind = OperationKind::TrainingSessionCreate;
        let mut child = update_entry("child", sequence + 1, false);
        child.operation.parent_operation_id = Some("parent".to_owned());
        let mut entries = vec![parent, child];
        prop_assert!(mark_sending(&mut entries, "parent"));
        prop_assert_eq!(entries[1].state, OutboxState::Blocked);
        prop_assert!(mark_success(&mut entries, "parent"));
        prop_assert_eq!(entries[0].state, OutboxState::Pending);
        prop_assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some("child")
        );
    }

    #[test]
    fn only_the_latest_unattempted_pending_update_is_merged(
        older_sequence in 1u64..5,
        older_attempted in any::<bool>(),
        newer_attempted in any::<bool>(),
    ) {
        let older = update_entry("older", older_sequence, older_attempted);
        let newer = update_entry("newer", older_sequence + 1, newer_attempted);
        let entries = vec![older, newer];
        let merged = pending_update(&entries, "training-1")
            .map(|entry| entry.operation.operation_id.as_str());
        let expected = if !newer_attempted {
            Some("newer")
        } else if !older_attempted {
            Some("older")
        } else {
            None
        };
        prop_assert_eq!(merged, expected);
    }

    #[test]
    fn a_transient_failure_can_be_recovered_and_sent_again(sequence in 1u64..5) {
        let mut entries = vec![update_entry("operation", sequence, false)];
        prop_assert!(mark_sending(&mut entries, "operation"));
        prop_assert!(mark_transient_failure(&mut entries, "operation"));
        prop_assert_eq!(entries[0].state, OutboxState::Pending);
        prop_assert!(entries[0].attempted);
        prop_assert_eq!(entries[0].attempts, 1);
        prop_assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some("operation")
        );
        prop_assert!(mark_sending(&mut entries, "operation"));
        prop_assert!(recover_interrupted(&mut entries));
        prop_assert_eq!(entries[0].state, OutboxState::Pending);
        prop_assert!(entries[0].attempted);
        prop_assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some("operation")
        );
    }
}
