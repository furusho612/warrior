//! 種目の木の端末内の保存、並べ替え、削除と、失敗した操作の破棄のブラウザのテスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-REC-004`。
//! 種目の作成、削除、並べ替えが送信待ちの列を通り、削除でセットと段と参照が消えること、
//! 失敗した作成の破棄で木が消え、失敗した削除の破棄でサーバーの記録を読み直せることを確かめる。

use serde_json::json;
use warrior_core::sync::{Operation, OperationKind};
use warrior_core::training_record::TrainingExerciseRecord;
use warrior_frontend::storage::{DiscardOutcome, ReplacePurpose, Store};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{
    create_operation, database_name, open, record_ids, server_record, session, TRAINING_ID,
};

const CREATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4302";
const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4307";
const OTHER_EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4308";
const CREATE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4309";
const OTHER_CREATE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4310";
const DELETE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4311";
const REORDER_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4312";
const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4313";
const EXERCISE_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4314";
const EXERCISE_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4315";
const EXERCISE_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4316";

#[wasm_bindgen_test]
async fn an_exercise_creation_and_deletion_use_the_outbox_and_remove_the_tree() {
    let name = database_name("exercise-tree");
    let storage = open(&name).await;
    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");

    let record = training_exercise_record(EXERCISE_ROW_ID, 1);
    storage
        .save_training_exercise_create(
            &record,
            &exercise_create_operation(CREATE_EXERCISE_ID, EXERCISE_ROW_ID, 1),
        )
        .await
        .expect("the exercise must be saved");
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].id, EXERCISE_ROW_ID);
    assert_eq!(exercises[0].position, 1);
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].operation.kind,
        OperationKind::TrainingExerciseCreate
    );
    assert_eq!(entries[0].operation.training_id, TRAINING_ID);

    for (store, record) in [
        (
            Store::Sets,
            json!({"id": EXERCISE_SET_ID, "training_exercise_id": EXERCISE_ROW_ID}),
        ),
        (
            Store::SetSegments,
            json!({"id": EXERCISE_SEGMENT_ID, "set_id": EXERCISE_SET_ID}),
        ),
        (
            Store::SupersetMembers,
            json!({"superset_id": EXERCISE_SUPERSET_ID, "training_exercise_id": EXERCISE_ROW_ID}),
        ),
    ] {
        storage
            .put_record(store, &record)
            .await
            .expect("the exercise tree must be saved");
    }

    storage
        .save_training_exercise_delete(
            TRAINING_ID,
            EXERCISE_ROW_ID,
            &exercise_delete_operation(DELETE_EXERCISE_ID, EXERCISE_ROW_ID),
        )
        .await
        .expect("the exercise must be deleted");
    assert!(storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable")
        .is_empty());
    assert!(record_ids(&storage, Store::Sets).await.is_empty());
    assert!(record_ids(&storage, Store::SetSegments).await.is_empty());
    assert!(storage
        .all_records(Store::SupersetMembers)
        .await
        .expect("the members must be readable")
        .is_empty());
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 2);
    assert!(entries
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::TrainingExerciseDelete));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn an_exercise_reorder_updates_the_local_positions_and_the_outbox() {
    let name = database_name("exercise-reorder");
    let storage = open(&name).await;
    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");

    for (record, operation) in [
        (
            training_exercise_record(EXERCISE_ROW_ID, 1),
            exercise_create_operation(CREATE_EXERCISE_ID, EXERCISE_ROW_ID, 1),
        ),
        (
            training_exercise_record(OTHER_EXERCISE_ROW_ID, 2),
            exercise_create_operation(OTHER_CREATE_EXERCISE_ID, OTHER_EXERCISE_ROW_ID, 2),
        ),
    ] {
        storage
            .save_training_exercise_create(&record, &operation)
            .await
            .expect("the exercise must be saved");
    }

    let ordered = vec![OTHER_EXERCISE_ROW_ID.to_owned(), EXERCISE_ROW_ID.to_owned()];
    storage
        .save_training_exercise_reorder(
            TRAINING_ID,
            &ordered,
            &exercise_reorder_operation(REORDER_EXERCISE_ID, &ordered),
        )
        .await
        .expect("the reorder must be saved");
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(
        exercises
            .iter()
            .map(|exercise| (exercise.id.as_str(), exercise.position))
            .collect::<Vec<_>>(),
        vec![(OTHER_EXERCISE_ROW_ID, 1), (EXERCISE_ROW_ID, 2)]
    );
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert!(entries
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::TrainingExerciseReorder));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_a_failed_exercise_create_removes_the_local_tree() {
    let storage = open(&database_name("discard-exercise-create")).await;
    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_training_exercise_create(
            &training_exercise_record(EXERCISE_ROW_ID, 1),
            &exercise_create_operation(CREATE_EXERCISE_ID, EXERCISE_ROW_ID, 1),
        )
        .await
        .expect("the exercise must be saved");
    storage
        .put_record(
            Store::Sets,
            &json!({"id": EXERCISE_SET_ID, "training_exercise_id": EXERCISE_ROW_ID}),
        )
        .await
        .expect("the set must be saved");
    storage
        .put_record(
            Store::SetSegments,
            &json!({"id": EXERCISE_SEGMENT_ID, "set_id": EXERCISE_SET_ID}),
        )
        .await
        .expect("the segment must be saved");
    storage
        .mark_sending(CREATE_EXERCISE_ID)
        .await
        .expect("the exercise must be sending");
    storage
        .mark_permanent_failure(CREATE_EXERCISE_ID)
        .await
        .expect("the exercise must fail");

    assert_eq!(
        storage
            .discard(CREATE_EXERCISE_ID)
            .await
            .expect("the discard must work"),
        DiscardOutcome::ExerciseDeleted
    );
    assert!(storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable")
        .is_empty());
    assert!(record_ids(&storage, Store::Sets).await.is_empty());
    assert!(record_ids(&storage, Store::SetSegments).await.is_empty());
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable")
        .is_empty());

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_a_failed_exercise_delete_reloads_the_server_tree() {
    let storage = open(&database_name("discard-exercise-delete")).await;
    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_training_exercise_create(
            &training_exercise_record(EXERCISE_ROW_ID, 1),
            &exercise_create_operation(CREATE_EXERCISE_ID, EXERCISE_ROW_ID, 1),
        )
        .await
        .expect("the exercise must be saved");
    storage
        .put_record(
            Store::Sets,
            &json!({"id": EXERCISE_SET_ID, "training_exercise_id": EXERCISE_ROW_ID}),
        )
        .await
        .expect("the set must be saved");
    storage
        .put_record(
            Store::SetSegments,
            &json!({"id": EXERCISE_SEGMENT_ID, "set_id": EXERCISE_SET_ID}),
        )
        .await
        .expect("the segment must be saved");
    storage
        .save_training_exercise_delete(
            TRAINING_ID,
            EXERCISE_ROW_ID,
            &exercise_delete_operation(DELETE_EXERCISE_ID, EXERCISE_ROW_ID),
        )
        .await
        .expect("the delete must be saved");
    storage
        .mark_sending(DELETE_EXERCISE_ID)
        .await
        .expect("the delete must be sending");
    storage
        .mark_permanent_failure(DELETE_EXERCISE_ID)
        .await
        .expect("the delete must fail");
    assert_eq!(
        storage
            .discard(DELETE_EXERCISE_ID)
            .await
            .expect("the discard must work"),
        DiscardOutcome::OperationsRemoved
    );
    assert!(storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable")
        .is_empty());

    storage
        .replace_session_tree(&server_record(TRAINING_ID, ""), ReplacePurpose::Reload)
        .await
        .expect("the server record must restore the tree");
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].id, "exercise-1");
    assert_eq!(record_ids(&storage, Store::Sets).await, vec!["set-1"]);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

fn exercise_snapshot() -> serde_json::Value {
    json!({
        "exercise_id": EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
}

fn training_exercise_record(id: &str, position: i64) -> TrainingExerciseRecord {
    TrainingExerciseRecord {
        id: id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        exercise_id: EXERCISE_ID.to_owned(),
        snapshot: exercise_snapshot().to_string(),
        position,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

fn exercise_create_operation(operation_id: &str, row_id: &str, position: i64) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseCreate,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "training_exercise_id": row_id,
            "exercise_id": EXERCISE_ID,
            "snapshot": exercise_snapshot(),
            "position": position
        }),
    }
}

fn exercise_delete_operation(operation_id: &str, row_id: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseDelete,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "training_exercise_id": row_id
        }),
    }
}

fn exercise_reorder_operation(operation_id: &str, ordered_ids: &[String]) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseReorder,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "ordered_ids": ordered_ids
        }),
    }
}
