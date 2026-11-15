//! ルーティンの端末内の保存と送信待ちの列のブラウザのテスト。
//!
//! 対応する要求: `REQ-RTN-001`、`REQ-RTN-003`。
//! 保存、編集、複製、削除が端末内の行と送信待ちの操作を同じトランザクションで書き、
//! 送信待ちの間はサーバーの一覧で端末の行を上書きしないことを確かめる。

mod support;

use support::{database_name, open};
use warrior_core::exercise::{
    BodyPart, ExerciseSnapshot, GripMode, Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
};
use warrior_core::routine::{Routine, RoutineExercise, RoutineTarget};
use warrior_core::set_calc::RecordMode;
use warrior_core::sync::OperationKind;
use warrior_frontend::routine;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

const ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5701";
const COPY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5702";
const ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5703";
const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5704";
const NOW: &str = "2026-11-12T00:00:00Z";

#[wasm_bindgen_test]
async fn routine_changes_are_saved_locally_and_enqueued() {
    let storage = open(&database_name("routine-outbox")).await;
    let source = routine_with("胸の日", 3, 100.0, 5);

    let create = routine::create(&source).expect("a create operation");
    storage
        .save_routine_create(&source, &create)
        .await
        .expect("the routine must be saved");
    let stored = storage.routines().await.expect("the stored routines");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "胸の日");
    assert_eq!(stored[0].exercises[0].target.set_count, 3);
    assert_eq!(stored[0].exercises[0].target.weight_kg, Some(100.0));
    assert_eq!(stored[0].exercises[0].target.reps, Some(5));
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation.kind, OperationKind::RoutineCreate);
    assert_eq!(entries[0].operation.training_id, ROUTINE_ID);

    let mut edited = source.clone();
    edited.name = "胸と体幹".to_owned();
    edited.exercises[0].target.reps = Some(8);
    edited.updated_at = "2026-11-12T01:00:00Z".to_owned();
    let update = routine::update(&edited).expect("an update operation");
    storage
        .save_routine_update(&edited, &update)
        .await
        .expect("the update must be saved");
    let stored = storage.routines().await.expect("the updated routines");
    assert_eq!(stored[0].name, "胸と体幹");
    assert_eq!(stored[0].exercises[0].target.reps, Some(8));
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.operation.kind == OperationKind::RoutineUpdate)
            .count(),
        1
    );

    let (copy, copy_operation) =
        routine::copy(&edited, COPY_ID, 2, "2026-11-12T02:00:00Z").expect("a copy");
    storage
        .save_routine_copy(&copy, &copy_operation)
        .await
        .expect("the copy must be saved");
    let stored = storage.routines().await.expect("the copied routines");
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[1].name, edited.name);
    assert_eq!(stored[1].exercises[0].target, edited.exercises[0].target);
    assert_ne!(stored[1].exercises[0].id, edited.exercises[0].id);
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.operation.kind == OperationKind::RoutineCopy)
            .count(),
        1
    );

    let delete = routine::delete(ROUTINE_ID).expect("a delete operation");
    storage
        .delete_routine(ROUTINE_ID, &delete)
        .await
        .expect("the routine must be deleted");
    let stored = storage.routines().await.expect("the remaining routines");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, COPY_ID);
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.operation.kind == OperationKind::RoutineDelete)
            .count(),
        1
    );
}

#[wasm_bindgen_test]
async fn a_pending_routine_is_not_replaced_by_the_server_list() {
    let storage = open(&database_name("routine-pending")).await;
    let source = routine_with("胸の日", 3, 100.0, 5);
    let create = routine::create(&source).expect("a create operation");
    storage
        .save_routine_create(&source, &create)
        .await
        .expect("the routine must be saved");

    let mut server = source.clone();
    server.name = "サーバーの胸の日".to_owned();
    storage
        .replace_routines(std::slice::from_ref(&server))
        .await
        .expect("the server list must be applied");
    let stored = storage.routines().await.expect("the stored routines");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "胸の日");
}

fn routine_with(name: &str, set_count: u32, weight_kg: f64, reps: u32) -> Routine {
    Routine {
        id: ROUTINE_ID.to_owned(),
        name: name.to_owned(),
        position: 1,
        exercises: vec![RoutineExercise {
            id: ROW_ID.to_owned(),
            routine_id: ROUTINE_ID.to_owned(),
            exercise_id: EXERCISE_ID.to_owned(),
            snapshot: snapshot(),
            position: 1,
            target: RoutineTarget {
                set_count,
                weight_kg: Some(weight_kg),
                reps: Some(reps),
                duration_seconds: None,
                grip: None,
                rest_seconds: Some(120),
            },
            created_at: NOW.to_owned(),
            updated_at: NOW.to_owned(),
        }],
        created_at: NOW.to_owned(),
        updated_at: NOW.to_owned(),
    }
}

fn snapshot() -> ExerciseSnapshot {
    ExerciseSnapshot {
        exercise_id: EXERCISE_ID.to_owned(),
        name: "ベンチプレス".to_owned(),
        body_part: BodyPart::Chest,
        muscles: vec![MuscleAssignment {
            muscle: MuscleUnit::PectoralisMajorMiddle,
            role: MuscleRole::Primary,
        }],
        record_mode: RecordMode::WeightAndReps,
        laterality: Laterality::Bilateral,
        grip_mode: GripMode::None,
        grip_options: Vec::new(),
    }
}
