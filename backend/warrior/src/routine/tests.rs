//! ルーティンの SQL と開始のセットの検証。
//!
//! 対応する要求: `REQ-RTN-001`、`REQ-RTN-003`、`REQ-RTN-004`。
//! SQL の文が利用者の条件を持つことと、開始のセットが目標値を持つことを確かめる。

use warrior_core::exercise::{
    BodyPart, ExerciseSnapshot, GripMode, Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
};
use warrior_core::routine::RoutineTarget;
use warrior_core::set_calc::RecordMode;
use warrior_core::sync::{
    Operation, OperationKind, RoutineCopyPayload, RoutineCreatePayload, RoutineDeletePayload,
    RoutineExercisePayload, RoutineUpdatePayload,
};

use super::{
    copy_statements, create_statements, delete_statements, initial_segment_statement,
    update_statements,
};

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5201";
const ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5202";
const NEW_ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5203";
const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5204";
const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5205";
const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5206";
const SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5207";
const SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5208";
const NOW: &str = "2026-11-12T00:00:00Z";

#[test]
fn req_rtn_001_create_statements_scope_the_routine_and_the_exercises_to_the_user() {
    let operation = operation(OperationKind::RoutineCreate, ROUTINE_ID);
    let statements = create_statements(USER_ID, &operation, &create_payload(), NOW);
    assert!(statements[0].sql().contains("ON CONFLICT(id)"));
    assert!(statements[0]
        .sql()
        .contains("WHERE routines.user_id = excluded.user_id"));
    assert!(statements[1].sql().contains("applied_operations"));
    let exercise = statements
        .iter()
        .find(|statement| statement.sql().contains("INSERT INTO routine_exercises"))
        .expect("an exercise insert");
    assert!(exercise
        .sql()
        .contains("WHERE EXISTS (SELECT 1 FROM routines WHERE id = ? AND user_id = ?)"));
    let target = statements
        .iter()
        .find(|statement| statement.sql().contains("routine_exercise_targets"))
        .expect("a target insert");
    assert!(target
        .sql()
        .contains("WHERE EXISTS (SELECT 1 FROM routine_exercises"));
    assert_eq!(target.bindings().len(), 12);
}

#[test]
fn req_rtn_003_update_replaces_the_exercises_of_the_owned_routine() {
    let operation = operation(OperationKind::RoutineUpdate, ROUTINE_ID);
    let payload = RoutineUpdatePayload {
        routine_id: ROUTINE_ID.to_owned(),
        name: "脚の日".to_owned(),
        position: 2,
        exercises: vec![exercise_payload()],
    };
    let statements = update_statements(USER_ID, &operation, &payload, NOW);
    assert!(statements[0]
        .sql()
        .contains("UPDATE routines SET name = ?, position = ?, updated_at = ?"));
    assert!(statements[0].sql().contains("WHERE id = ? AND user_id = ?"));
    assert!(statements
        .iter()
        .any(|statement| statement.sql().contains("DELETE FROM routine_exercises")));
}

#[test]
fn req_rtn_003_delete_keeps_the_trainings_and_clears_their_routine_id() {
    let operation = operation(OperationKind::RoutineDelete, ROUTINE_ID);
    let statements = delete_statements(
        USER_ID,
        &operation,
        &RoutineDeletePayload {
            routine_id: ROUTINE_ID.to_owned(),
        },
        NOW,
    );
    assert!(statements[0]
        .sql()
        .contains("DELETE FROM routines WHERE id = ? AND user_id = ?"));
    let training = statements
        .iter()
        .find(|statement| statement.sql().contains("UPDATE training_sessions"))
        .expect("a training update");
    assert!(training.sql().contains("SET routine_id = NULL"));
    assert!(training
        .sql()
        .contains("WHERE user_id = ? AND routine_id = ?"));
    assert!(!statements
        .iter()
        .any(|statement| statement.sql().contains("DELETE FROM training_sessions")));
}

#[test]
fn req_rtn_003_copy_writes_the_new_routine_and_the_copied_exercises() {
    let operation = operation(OperationKind::RoutineCopy, NEW_ROUTINE_ID);
    let statements = copy_statements(USER_ID, &operation, &copy_payload(), NOW);
    assert!(statements[0].sql().contains("INSERT INTO routines"));
    assert_eq!(
        statements[0].bindings()[0],
        warrior_core::sql::SqlValue::Text(NEW_ROUTINE_ID.to_owned())
    );
    assert!(statements
        .iter()
        .any(|statement| statement.sql().contains("INSERT INTO routine_exercises")));
}

#[test]
fn req_rtn_004_a_bilateral_start_set_has_the_target_weight_and_reps() {
    let snapshot = snapshot(
        RecordMode::WeightAndReps,
        Laterality::Bilateral,
        GripMode::None,
    );
    let statement =
        initial_segment_statement(USER_ID, SET_ID, SEGMENT_ID, &snapshot, &target(), NOW);
    assert_eq!(
        statement.bindings()[3],
        warrior_core::sql::SqlValue::Real(100.0)
    );
    assert_eq!(
        statement.bindings()[4],
        warrior_core::sql::SqlValue::Integer(5)
    );
    assert_eq!(statement.bindings()[5], warrior_core::sql::SqlValue::Null);
    assert_eq!(statement.bindings()[6], warrior_core::sql::SqlValue::Null);
}

#[test]
fn req_rtn_004_a_unilateral_start_set_has_both_sides() {
    let snapshot = snapshot(
        RecordMode::WeightAndReps,
        Laterality::Unilateral,
        GripMode::None,
    );
    let statement =
        initial_segment_statement(USER_ID, SET_ID, SEGMENT_ID, &snapshot, &target(), NOW);
    assert_eq!(
        statement.bindings()[6],
        warrior_core::sql::SqlValue::Real(100.0)
    );
    assert_eq!(
        statement.bindings()[7],
        warrior_core::sql::SqlValue::Integer(5)
    );
    assert_eq!(
        statement.bindings()[9],
        warrior_core::sql::SqlValue::Real(100.0)
    );
    assert_eq!(
        statement.bindings()[10],
        warrior_core::sql::SqlValue::Integer(5)
    );
}

#[test]
fn req_rtn_004_a_time_start_set_has_the_target_duration() {
    let snapshot = snapshot(RecordMode::Time, Laterality::Bilateral, GripMode::None);
    let mut target = target();
    target.reps = None;
    target.weight_kg = None;
    target.duration_seconds = Some(90);
    let statement = initial_segment_statement(USER_ID, SET_ID, SEGMENT_ID, &snapshot, &target, NOW);
    assert_eq!(
        statement.bindings()[5],
        warrior_core::sql::SqlValue::Integer(90)
    );
}

fn operation(kind: OperationKind, routine_id: &str) -> Operation {
    Operation {
        operation_id: OPERATION_ID.to_owned(),
        training_id: routine_id.to_owned(),
        sequence: 1,
        kind,
        parent_operation_id: None,
        payload: serde_json::json!({}),
    }
}

fn create_payload() -> RoutineCreatePayload {
    RoutineCreatePayload {
        routine_id: ROUTINE_ID.to_owned(),
        name: "胸の日".to_owned(),
        position: 1,
        exercises: vec![exercise_payload()],
    }
}

fn copy_payload() -> RoutineCopyPayload {
    RoutineCopyPayload {
        routine_id: ROUTINE_ID.to_owned(),
        new_routine_id: NEW_ROUTINE_ID.to_owned(),
        name: "胸の日".to_owned(),
        position: 2,
        exercises: vec![exercise_payload()],
    }
}

fn exercise_payload() -> RoutineExercisePayload {
    RoutineExercisePayload {
        routine_exercise_id: EXERCISE_ROW_ID.to_owned(),
        exercise_id: EXERCISE_ID.to_owned(),
        snapshot: snapshot(
            RecordMode::WeightAndReps,
            Laterality::Bilateral,
            GripMode::None,
        ),
        position: 1,
        target: target(),
    }
}

fn target() -> RoutineTarget {
    RoutineTarget {
        set_count: 3,
        weight_kg: Some(100.0),
        reps: Some(5),
        duration_seconds: None,
        grip: None,
        rest_seconds: Some(120),
    }
}

fn snapshot(
    record_mode: RecordMode,
    laterality: Laterality,
    grip_mode: GripMode,
) -> ExerciseSnapshot {
    ExerciseSnapshot {
        exercise_id: EXERCISE_ID.to_owned(),
        name: "ベンチプレス".to_owned(),
        body_part: BodyPart::Chest,
        muscles: vec![MuscleAssignment {
            muscle: MuscleUnit::PectoralisMajorMiddle,
            role: MuscleRole::Primary,
        }],
        record_mode,
        laterality,
        grip_mode,
        grip_options: Vec::new(),
    }
}
