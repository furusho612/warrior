//! セットの操作の SQL の組み立ての例示テスト。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`。

use serde_json::json;
use warrior_core::set_calc::SetKind;
use warrior_core::sql::SqlValue;
use warrior_core::sync::{
    Operation, OperationKind, SetCompletePayload, SetCreatePayload, SetDeletePayload,
    SetReorderPayload, SetSegmentPayload, SetSidePayload, SetUpdatePayload, ValidatedOperation,
    ValidatedPayload,
};

use super::bindings::{applied_exists, ownership_condition_placeholder, segment_bindings};
use super::{
    delete_set_statement, same_set_ids, set_statements, update_set_statement, ExerciseOwnership,
};

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5301";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5302";
const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5303";
const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5304";
const SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5305";
const OTHER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5306";
const SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5307";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5308";
const NOW: &str = "2026-11-12T00:00:00Z";

fn operation(kind: OperationKind, payload: serde_json::Value) -> Operation {
    Operation {
        operation_id: OPERATION_ID.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 2,
        kind,
        parent_operation_id: None,
        payload,
    }
}

fn segment() -> SetSegmentPayload {
    SetSegmentPayload {
        segment_id: SEGMENT_ID.to_owned(),
        position: 1,
        weight_kg: 100.0,
        reps: 5,
        momentum_reps: 0,
        assisted_reps: 0,
        duration_seconds: None,
        left: None,
        right: None,
    }
}

fn create_payload() -> SetCreatePayload {
    SetCreatePayload {
        training_id: TRAINING_ID.to_owned(),
        training_exercise_id: EXERCISE_ROW_ID.to_owned(),
        set_id: SET_ID.to_owned(),
        position: 1,
        kind: SetKind::Normal,
        rpe: None,
        grip: None,
        body_weight_kg: Some(70.0),
        segments: vec![segment()],
    }
}

fn validated(kind: OperationKind, payload: ValidatedPayload) -> ValidatedOperation {
    ValidatedOperation {
        operation: operation(kind, json!({})),
        payload,
    }
}

#[test]
fn the_create_statements_scope_the_set_and_the_segments() {
    let validated = validated(
        OperationKind::SetCreate,
        ValidatedPayload::SetCreate(create_payload()),
    );
    let data = set_statements(
        USER_ID,
        &validated,
        ExerciseOwnership::Terminal(TERMINAL_ID),
        NOW,
        None,
    )
    .expect("data statements");
    assert!(data
        .main
        .sql()
        .contains("WHERE EXISTS (SELECT 1 FROM training_exercises te"));
    assert!(data.main.sql().contains("ON CONFLICT(id) DO UPDATE"));
    assert!(data.main.sql().contains("sets.user_id = excluded.user_id"));
    assert!(data
        .main
        .sql()
        .contains("sets.training_exercise_id = excluded.training_exercise_id"));
    assert!(data
        .main
        .sql()
        .contains("EXISTS (SELECT 1 FROM training_sessions ts"));
    assert_eq!(data.main.bindings()[0], SqlValue::Text(SET_ID.to_owned()));
    assert_eq!(data.main.bindings()[1], SqlValue::Text(USER_ID.to_owned()));
    assert_eq!(
        data.main.bindings()[2],
        SqlValue::Text(EXERCISE_ROW_ID.to_owned())
    );
    assert_eq!(data.main.bindings()[5], SqlValue::Null);
    assert_eq!(data.main.bindings()[6], SqlValue::Null);
    assert_eq!(data.main.bindings()[7], SqlValue::Real(70.0));
    assert_eq!(data.children.len(), 2);
    assert!(data.children[0].sql().contains("DELETE FROM set_segments"));
    assert!(data.children[0]
        .sql()
        .contains("set_id = ? AND user_id = ?"));
    assert!(data.children[0]
        .sql()
        .contains("EXISTS (SELECT 1 FROM applied_operations"));
    assert!(data.children[1].sql().contains("INSERT INTO set_segments"));
    assert!(data.children[1].sql().contains("ON CONFLICT(id) DO UPDATE"));
    assert_eq!(
        data.children[1].bindings()[3],
        SqlValue::Integer(1),
        "the segment position is the fourth value"
    );
    assert_eq!(data.children[1].bindings()[4], SqlValue::Real(100.0));
    assert_eq!(data.children[1].bindings()[5], SqlValue::Integer(5));
}

#[test]
fn the_update_and_delete_statements_keep_the_training_scope() {
    let update = SetUpdatePayload {
        training_id: TRAINING_ID.to_owned(),
        set_id: SET_ID.to_owned(),
        position: 2,
        kind: SetKind::Drop,
        completed: false,
        rpe: Some(8.0),
        grip: None,
        body_weight_kg: None,
        segments: vec![segment()],
    };
    let statement = update_set_statement(
        USER_ID,
        TRAINING_ID,
        &update,
        ExerciseOwnership::Finished,
        NOW,
        None,
    );
    assert!(statement
        .sql()
        .contains("UPDATE sets SET position = ?, kind = ?, completed = ?"));
    assert!(statement
        .sql()
        .contains("training_exercises WHERE training_id = ?"));
    assert!(statement.sql().contains("ts.finished_at IS NOT NULL"));
    assert_eq!(statement.bindings()[0], SqlValue::Integer(2));
    assert_eq!(statement.bindings()[1], SqlValue::Text("drop".to_owned()));
    assert_eq!(statement.bindings()[2], SqlValue::Integer(0));
    assert_eq!(statement.bindings()[3], SqlValue::Real(8.0));

    let delete = delete_set_statement(
        USER_ID,
        TRAINING_ID,
        &SetDeletePayload {
            training_id: TRAINING_ID.to_owned(),
            set_id: SET_ID.to_owned(),
        },
        ExerciseOwnership::Terminal(TERMINAL_ID),
    );
    assert!(delete.sql().contains("DELETE FROM sets"));
    assert!(delete
        .sql()
        .contains("EXISTS (SELECT 1 FROM training_sessions ts"));
    assert_eq!(delete.bindings()[0], SqlValue::Text(SET_ID.to_owned()));
}

#[test]
fn every_set_statement_uses_the_applied_operation_condition() {
    let create = validated(
        OperationKind::SetCreate,
        ValidatedPayload::SetCreate(create_payload()),
    );
    let data = set_statements(
        USER_ID,
        &create,
        ExerciseOwnership::Terminal(TERMINAL_ID),
        NOW,
        None,
    )
    .expect("data statements");
    for child in &data.children {
        assert!(child.sql().contains("applied_operations"));
        assert!(child
            .bindings()
            .contains(&SqlValue::Text(OPERATION_ID.to_owned())));
    }

    let delete = validated(
        OperationKind::SetDelete,
        ValidatedPayload::SetDelete(SetDeletePayload {
            training_id: TRAINING_ID.to_owned(),
            set_id: SET_ID.to_owned(),
        }),
    );
    let data = set_statements(USER_ID, &delete, ExerciseOwnership::Finished, NOW, None)
        .expect("data statements");
    assert_eq!(data.children.len(), 1);
    assert!(data.children[0]
        .sql()
        .contains("DELETE FROM set_segments WHERE set_id = ? AND user_id = ?"));
}

#[test]
fn the_reorder_statement_numbers_the_positions_from_one() {
    let payload = SetReorderPayload {
        training_id: TRAINING_ID.to_owned(),
        training_exercise_id: EXERCISE_ROW_ID.to_owned(),
        ordered_ids: vec![OTHER_SET_ID.to_owned(), SET_ID.to_owned()],
    };
    let validated = validated(
        OperationKind::SetReorder,
        ValidatedPayload::SetReorder(payload),
    );
    let data = set_statements(
        USER_ID,
        &validated,
        ExerciseOwnership::Terminal(TERMINAL_ID),
        NOW,
        None,
    )
    .expect("data statements");
    assert!(data.main.sql().contains("position = CASE id"));
    assert!(data.main.sql().contains("WHERE id IN (?, ?)"));
    assert!(data
        .main
        .sql()
        .contains("EXISTS (SELECT 1 FROM training_sessions ts"));
    assert_eq!(
        data.main.bindings()[0],
        SqlValue::Text(OTHER_SET_ID.to_owned())
    );
    assert_eq!(data.main.bindings()[1], SqlValue::Integer(1));
    assert_eq!(data.main.bindings()[2], SqlValue::Text(SET_ID.to_owned()));
    assert_eq!(data.main.bindings()[3], SqlValue::Integer(2));
}

#[test]
fn the_complete_statement_sets_the_completed_flag() {
    let payload = SetCompletePayload {
        training_id: TRAINING_ID.to_owned(),
        set_id: SET_ID.to_owned(),
        completed: true,
    };
    let validated = validated(
        OperationKind::SetComplete,
        ValidatedPayload::SetComplete(payload),
    );
    let data = set_statements(USER_ID, &validated, ExerciseOwnership::Finished, NOW, None)
        .expect("data statements");
    assert!(data.main.sql().contains("UPDATE sets SET completed = ?"));
    assert_eq!(data.main.bindings()[0], SqlValue::Integer(1));
    assert_eq!(data.main.bindings()[2], SqlValue::Text(SET_ID.to_owned()));
}

#[test]
fn set_ids_match_regardless_of_the_input_order() {
    assert!(same_set_ids(
        &[SET_ID.to_owned(), OTHER_SET_ID.to_owned()],
        &[OTHER_SET_ID.to_owned(), SET_ID.to_owned()]
    ));
    assert!(!same_set_ids(
        &[SET_ID.to_owned()],
        &[OTHER_SET_ID.to_owned()]
    ));
}

#[test]
fn segment_bindings_keep_the_sides_and_the_durations() {
    let mut segment = segment();
    segment.duration_seconds = Some(30);
    segment.left = Some(SetSidePayload {
        weight_kg: Some(10.0),
        reps: 4,
        duration_seconds: Some(20),
    });
    let bindings = segment_bindings(USER_ID, SET_ID, &segment, NOW);
    assert_eq!(bindings[8], SqlValue::Integer(30));
    assert_eq!(bindings[9], SqlValue::Real(10.0));
    assert_eq!(bindings[10], SqlValue::Integer(4));
    assert_eq!(bindings[11], SqlValue::Integer(20));
    assert_eq!(bindings[12], SqlValue::Null);
    assert_eq!(bindings[13], SqlValue::Null);
    assert_eq!(bindings[14], SqlValue::Null);
    assert_eq!(bindings[15], SqlValue::Text(NOW.to_owned()));
}

#[test]
fn the_placeholder_ownership_condition_keeps_the_edit_right() {
    let applied = applied_exists(USER_ID, OPERATION_ID);
    assert!(applied.sql.contains("operation_id = ?"));
    assert_eq!(applied.bindings[0], SqlValue::Text(OPERATION_ID.to_owned()));
    let (sql, bindings) =
        ownership_condition_placeholder(USER_ID, ExerciseOwnership::Terminal(TERMINAL_ID));
    assert!(sql.contains("ts.owner_terminal_id = ?"));
    assert_eq!(bindings.len(), 2);
    let (sql, bindings) = ownership_condition_placeholder(USER_ID, ExerciseOwnership::Finished);
    assert!(sql.contains("ts.finished_at IS NOT NULL"));
    assert_eq!(bindings.len(), 1);
}
