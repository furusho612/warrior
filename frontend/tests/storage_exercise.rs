//! 種目の木の端末内の保存、並べ替え、削除と、失敗した操作の破棄のブラウザのテスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-NFR-005`。
//! 種目の作成、削除、並べ替えが送信待ちの列を通り、削除でセットと段と参照が消えること、
//! 失敗した作成の破棄で対象の行だけが消え、失敗した削除の破棄でサーバーの記録から
//! 対象の種目だけを戻せることを確かめる。

use serde_json::json;
use warrior_core::set_calc::SetKind;
use warrior_core::sync::{Operation, OperationKind, SessionUpdatePayload};
use warrior_core::training_record::{TrainingExerciseRecord, TrainingRecord};
use warrior_frontend::storage::{DiscardOutcome, Store};
use warrior_frontend::training_set::{self, SetValues};
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
const OTHER_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4317";
const OTHER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4318";
const OTHER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4319";
const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4320";

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
async fn discarding_a_failed_exercise_create_removes_only_the_discarded_exercise() {
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
        .save_training_exercise_create(
            &training_exercise_record(OTHER_EXERCISE_ROW_ID, 2),
            &exercise_create_operation(OTHER_CREATE_EXERCISE_ID, OTHER_EXERCISE_ROW_ID, 2),
        )
        .await
        .expect("the other exercise must be saved");
    storage
        .mark_success(OTHER_CREATE_EXERCISE_ID)
        .await
        .expect("the other exercise must succeed");
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
        .put_record(
            Store::Sets,
            &json!({"id": OTHER_SET_ID, "training_exercise_id": OTHER_EXERCISE_ROW_ID}),
        )
        .await
        .expect("the other set must be saved");
    storage
        .put_record(
            Store::SetSegments,
            &json!({"id": OTHER_SEGMENT_ID, "set_id": OTHER_SET_ID}),
        )
        .await
        .expect("the other segment must be saved");
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
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].id, OTHER_EXERCISE_ROW_ID);
    assert_eq!(record_ids(&storage, Store::Sets).await, vec![OTHER_SET_ID]);
    assert_eq!(
        record_ids(&storage, Store::SetSegments).await,
        vec![OTHER_SEGMENT_ID]
    );
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
async fn discarding_a_failed_exercise_create_removes_its_set_operations() {
    let storage = open(&database_name("discard-exercise-create-sets")).await;
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
        .save_training_exercise_create(
            &training_exercise_record(OTHER_EXERCISE_ROW_ID, 2),
            &exercise_create_operation(OTHER_CREATE_EXERCISE_ID, OTHER_EXERCISE_ROW_ID, 2),
        )
        .await
        .expect("the other exercise must be saved");
    storage
        .mark_success(OTHER_CREATE_EXERCISE_ID)
        .await
        .expect("the other exercise must succeed");

    let (set, set_create) =
        training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 1, &set_values(100.0, 5))
            .expect("the set");
    storage
        .save_set_create(&set.set, &set.segment, &set_create)
        .await
        .expect("the set must be saved");
    let (updated, set_update) =
        training_set::update(TRAINING_ID, &set.set, &set.segment, &set_values(120.0, 6))
            .expect("the set update");
    storage
        .save_set_update(&updated.set, &updated.segment, &set_update)
        .await
        .expect("the set update must be saved");
    let (completed, set_complete) =
        training_set::complete(TRAINING_ID, &set.set, true).expect("the set completion");
    storage
        .save_set_complete(&completed, &set_complete)
        .await
        .expect("the set completion must be saved");

    let (other_set, other_set_create) =
        training_set::create(TRAINING_ID, OTHER_EXERCISE_ROW_ID, 1, &set_values(80.0, 8))
            .expect("the other set");
    storage
        .save_set_create(&other_set.set, &other_set.segment, &other_set_create)
        .await
        .expect("the other set must be saved");

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

    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].id, OTHER_EXERCISE_ROW_ID);
    assert_eq!(
        record_ids(&storage, Store::Sets).await,
        vec![other_set.set.id.clone()]
    );
    let entries = storage.outbox_entries().await.expect("the outbox");
    for operation_id in [
        CREATE_EXERCISE_ID,
        set_create.operation_id.as_str(),
        set_update.operation_id.as_str(),
        set_complete.operation_id.as_str(),
    ] {
        assert!(
            !entries
                .iter()
                .any(|entry| entry.operation.operation_id == operation_id),
            "the dependent operation must be removed: {operation_id}"
        );
    }
    assert!(entries
        .iter()
        .any(|entry| entry.operation.operation_id == other_set_create.operation_id));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_a_failed_exercise_delete_restores_only_the_target() {
    let storage = open(&database_name("discard-exercise-delete")).await;
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
            Store::Sets,
            json!({"id": OTHER_SET_ID, "training_exercise_id": OTHER_EXERCISE_ROW_ID}),
        ),
        (
            Store::SetSegments,
            json!({"id": OTHER_SEGMENT_ID, "set_id": OTHER_SET_ID}),
        ),
        (
            Store::Supersets,
            json!({
                "id": EXERCISE_SUPERSET_ID,
                "training_id": TRAINING_ID,
                "position": 1,
                "created_at": "2026-11-12T03:00:00Z",
                "updated_at": "2026-11-12T03:00:00Z"
            }),
        ),
        (
            Store::SupersetMembers,
            json!({
                "superset_id": EXERCISE_SUPERSET_ID,
                "training_exercise_id": EXERCISE_ROW_ID,
                "position": 1
            }),
        ),
        (
            Store::SupersetMembers,
            json!({
                "superset_id": EXERCISE_SUPERSET_ID,
                "training_exercise_id": OTHER_EXERCISE_ROW_ID,
                "position": 2
            }),
        ),
    ] {
        storage
            .put_record(store, &record)
            .await
            .expect("the exercise tree must be saved");
    }
    storage
        .save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the memo update must be saved");
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
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 1);
    assert_eq!(exercises[0].id, OTHER_EXERCISE_ROW_ID);
    assert_eq!(record_ids(&storage, Store::Sets).await, vec![OTHER_SET_ID]);
    assert_eq!(
        record_ids(&storage, Store::SetSegments).await,
        vec![OTHER_SEGMENT_ID]
    );
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].operation.kind,
        OperationKind::TrainingSessionUpdate
    );

    let server = two_exercise_server_record();
    storage
        .restore_training_exercise(&server, EXERCISE_ROW_ID)
        .await
        .expect("the exercise must be restored");
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises.len(), 2);
    assert_eq!(exercises[0].id, EXERCISE_ROW_ID);
    assert_eq!(exercises[1].id, OTHER_EXERCISE_ROW_ID);
    assert!(record_ids(&storage, Store::Sets)
        .await
        .contains(&EXERCISE_SET_ID.to_owned()));
    assert!(record_ids(&storage, Store::Sets)
        .await
        .contains(&OTHER_SET_ID.to_owned()));
    assert!(record_ids(&storage, Store::SetSegments)
        .await
        .contains(&EXERCISE_SEGMENT_ID.to_owned()));
    assert!(record_ids(&storage, Store::SetSegments)
        .await
        .contains(&OTHER_SEGMENT_ID.to_owned()));
    assert_eq!(
        storage
            .all_records(Store::SupersetMembers)
            .await
            .expect("the members must be readable")
            .len(),
        2
    );
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].operation.kind,
        OperationKind::TrainingSessionUpdate
    );
    assert_eq!(
        storage
            .session(TRAINING_ID)
            .await
            .expect("the session must be readable")
            .expect("the session must exist")
            .memo,
        "draft"
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_a_failed_exercise_reorder_restores_positions_and_keeps_other_state() {
    let storage = open(&database_name("discard-exercise-reorder")).await;
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
    storage
        .put_record(
            Store::Sets,
            &json!({"id": OTHER_SET_ID, "training_exercise_id": OTHER_EXERCISE_ROW_ID}),
        )
        .await
        .expect("the other set must be saved");
    storage
        .save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the memo update must be saved");
    let ordered = vec![OTHER_EXERCISE_ROW_ID.to_owned(), EXERCISE_ROW_ID.to_owned()];
    storage
        .save_training_exercise_reorder(
            TRAINING_ID,
            &ordered,
            &exercise_reorder_operation(REORDER_EXERCISE_ID, &ordered),
        )
        .await
        .expect("the reorder must be saved");
    storage
        .mark_sending(REORDER_EXERCISE_ID)
        .await
        .expect("the reorder must be sending");
    storage
        .mark_permanent_failure(REORDER_EXERCISE_ID)
        .await
        .expect("the reorder must fail");
    assert_eq!(
        storage
            .discard(REORDER_EXERCISE_ID)
            .await
            .expect("the discard must work"),
        DiscardOutcome::OperationsRemoved
    );
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises[0].id, OTHER_EXERCISE_ROW_ID);
    assert_eq!(exercises[1].id, EXERCISE_ROW_ID);
    assert_eq!(record_ids(&storage, Store::Sets).await, vec![OTHER_SET_ID]);

    let server = two_exercise_server_record();
    storage
        .restore_training_exercise_positions(&server)
        .await
        .expect("the positions must be restored");
    let exercises = storage
        .training_exercises(TRAINING_ID)
        .await
        .expect("the exercises must be readable");
    assert_eq!(exercises[0].id, EXERCISE_ROW_ID);
    assert_eq!(exercises[0].position, 1);
    assert_eq!(exercises[1].id, OTHER_EXERCISE_ROW_ID);
    assert_eq!(exercises[1].position, 2);
    assert_eq!(record_ids(&storage, Store::Sets).await, vec![OTHER_SET_ID]);
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].operation.kind,
        OperationKind::TrainingSessionUpdate
    );
    assert_eq!(
        storage
            .session(TRAINING_ID)
            .await
            .expect("the session must be readable")
            .expect("the session must exist")
            .memo,
        "draft"
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

fn two_exercise_server_record() -> TrainingRecord {
    let mut value = serde_json::to_value(server_record(TRAINING_ID, "server"))
        .expect("the server record must serialize");
    value["exercises"][0]["id"] = json!(EXERCISE_ROW_ID);
    value["exercises"][0]["exercise_id"] = json!(EXERCISE_ID);
    value["sets"][0]["id"] = json!(EXERCISE_SET_ID);
    value["sets"][0]["training_exercise_id"] = json!(EXERCISE_ROW_ID);
    value["segments"][0]["id"] = json!(EXERCISE_SEGMENT_ID);
    value["segments"][0]["set_id"] = json!(EXERCISE_SET_ID);
    value["supersets"][0]["id"] = json!(EXERCISE_SUPERSET_ID);
    value["supersets"][0]["members"][0]["superset_id"] = json!(EXERCISE_SUPERSET_ID);
    value["supersets"][0]["members"][0]["training_exercise_id"] = json!(EXERCISE_ROW_ID);
    value["exercises"]
        .as_array_mut()
        .expect("the exercises must be an array")
        .push(json!({
            "id": OTHER_EXERCISE_ROW_ID,
            "training_id": TRAINING_ID,
            "exercise_id": OTHER_EXERCISE_ID,
            "snapshot": "{}",
            "position": 2,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }));
    value["sets"]
        .as_array_mut()
        .expect("the sets must be an array")
        .push(json!({
            "id": OTHER_SET_ID,
            "training_exercise_id": OTHER_EXERCISE_ROW_ID,
            "position": 1,
            "kind": "normal",
            "completed": 1,
            "rpe": null,
            "grip": null,
            "recorded_at": null,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }));
    value["segments"]
        .as_array_mut()
        .expect("the segments must be an array")
        .push(json!({
            "id": OTHER_SEGMENT_ID,
            "set_id": OTHER_SET_ID,
            "position": 1,
            "weight_kg": 50.0,
            "reps": 8,
            "momentum_reps": 0,
            "assisted_reps": 0,
            "duration_seconds": null,
            "left_weight_kg": null,
            "left_reps": null,
            "left_duration_seconds": null,
            "right_weight_kg": null,
            "right_reps": null,
            "right_duration_seconds": null,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }));
    value["supersets"][0]["members"]
        .as_array_mut()
        .expect("the members must be an array")
        .push(json!({
            "superset_id": EXERCISE_SUPERSET_ID,
            "training_exercise_id": OTHER_EXERCISE_ROW_ID,
            "position": 2
        }));
    serde_json::from_value(value).expect("the server record must deserialize")
}

fn update_operation(operation_id: &str, memo: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionUpdate,
        parent_operation_id: None,
        payload: serde_json::to_value(SessionUpdatePayload {
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: memo.to_owned(),
        })
        .expect("an update payload"),
    }
}

fn set_values(weight_kg: f64, reps: u32) -> SetValues {
    SetValues {
        kind: SetKind::Normal,
        weight_kg,
        reps,
        duration_seconds: None,
        completed: false,
        body_weight_kg: None,
        rpe: None,
        grip: None,
    }
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
