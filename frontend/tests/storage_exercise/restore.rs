//! 種目の木の端末内の保存と、失敗した削除の復元のテスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-NFR-005`。
//! 種目の作成、削除、並べ替えが送信待ちの列を通り、削除でセットと段と参照が消えること、
//! 失敗した削除の破棄でサーバーの記録から対象の種目だけを戻せることを確かめる。

use serde_json::json;
use warrior_core::sync::OperationKind;
use warrior_frontend::storage::{DiscardOutcome, RestoreOutcome, Store};
use wasm_bindgen_test::wasm_bindgen_test;

use crate::support::exercise_tree::{
    exercise_create_operation, exercise_delete_operation, exercise_reorder_operation,
    training_exercise_record, two_exercise_server_record, CREATE_EXERCISE_ID, CREATE_ID,
    DELETE_EXERCISE_ID, EXERCISE_ROW_ID, EXERCISE_SEGMENT_ID, EXERCISE_SET_ID,
    EXERCISE_SUPERSET_ID, OTHER_CREATE_EXERCISE_ID, OTHER_EXERCISE_ROW_ID, REORDER_EXERCISE_ID,
};
use crate::support::{create_operation, database_name, open, record_ids, session, TRAINING_ID};

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
            json!({
                "superset_id": EXERCISE_SUPERSET_ID,
                "training_exercise_id": EXERCISE_ROW_ID,
                "position": 1
            }),
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
    assert_eq!(entries.len(), 3);
    assert!(entries
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::TrainingExerciseDelete));
    assert!(entries
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::SupersetDelete));

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
async fn restoring_an_exercise_defers_for_a_newer_reorder() {
    let storage = open(&database_name("restore-exercise-newer")).await;
    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    for (row_id, operation_id, position) in [
        (EXERCISE_ROW_ID, CREATE_EXERCISE_ID, 1),
        (OTHER_EXERCISE_ROW_ID, OTHER_CREATE_EXERCISE_ID, 2),
    ] {
        storage
            .save_training_exercise_create(
                &training_exercise_record(row_id, position),
                &exercise_create_operation(operation_id, row_id, position),
            )
            .await
            .expect("the exercise must be saved");
        storage
            .mark_success(operation_id)
            .await
            .expect("the exercise must succeed");
    }

    let delete = exercise_delete_operation(DELETE_EXERCISE_ID, EXERCISE_ROW_ID);
    storage
        .save_training_exercise_delete(TRAINING_ID, EXERCISE_ROW_ID, &delete)
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

    let ordered = vec![OTHER_EXERCISE_ROW_ID.to_owned()];
    let reorder = exercise_reorder_operation(REORDER_EXERCISE_ID, &ordered);
    storage
        .save_training_exercise_reorder(TRAINING_ID, &ordered, &reorder)
        .await
        .expect("the reorder must be saved");

    assert_eq!(
        storage
            .restore_training_exercise_with_outcome(
                &two_exercise_server_record(),
                EXERCISE_ROW_ID,
                delete.sequence,
            )
            .await
            .expect("the restore"),
        RestoreOutcome::Deferred,
        "a newer reorder must defer the restore"
    );
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].id, OTHER_EXERCISE_ROW_ID);
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .any(|entry| entry.operation.operation_id == reorder.operation_id));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}
