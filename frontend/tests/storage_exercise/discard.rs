//! 失敗した種目の操作の破棄のテスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-NFR-005`、`REQ-SET-005`。
//! 失敗した作成の破棄で対象の行だけが消え、失敗した削除の破棄でサーバーの記録から
//! 対象の種目だけを戻せること、失敗した並べ替えの破棄で位置と組を戻すことを確かめる。

use serde_json::json;
use warrior_core::superset::SupersetMembers;
use warrior_core::sync::OperationKind;
use warrior_frontend::storage::{DiscardOutcome, Store};
use warrior_frontend::superset;
use warrior_frontend::training_set;
use wasm_bindgen_test::wasm_bindgen_test;

use crate::support::exercise_tree::{
    exercise_create_operation, exercise_delete_operation, exercise_reorder_operation, set_values,
    training_exercise_record, two_exercise_server_record, update_operation, CREATE_EXERCISE_ID,
    CREATE_ID, DELETE_EXERCISE_ID, EXERCISE_ROW_ID, EXERCISE_SEGMENT_ID, EXERCISE_SET_ID,
    EXERCISE_SUPERSET_ID, OTHER_CREATE_EXERCISE_ID, OTHER_EXERCISE_ROW_ID, OTHER_SEGMENT_ID,
    OTHER_SET_ID, REORDER_EXERCISE_ID, UPDATE_ID,
};
use crate::support::{create_operation, database_name, open, record_ids, session, TRAINING_ID};

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
        .save_set_create(&set.set, &set.segments, &set_create)
        .await
        .expect("the set must be saved");
    let (updated, set_update) =
        training_set::update(TRAINING_ID, &set.set, &set.segments, &set_values(120.0, 6))
            .expect("the set update");
    storage
        .save_set_update(&updated.set, &updated.segments, &set_update)
        .await
        .expect("the set update must be saved");
    let (completed, set_complete) =
        training_set::complete(TRAINING_ID, &set.set, true).expect("the set completion");
    storage
        .save_set_complete(&completed, &set_complete)
        .await
        .expect("the set completion must be saved");
    let ordered_sets = vec![set.set.id.clone()];
    let set_reorder = training_set::reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered_sets)
        .expect("the set reorder");
    storage
        .save_set_reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered_sets, &set_reorder)
        .await
        .expect("the set reorder must be saved");
    let set_delete = training_set::delete(TRAINING_ID, &set.set.id).expect("the set delete");
    storage
        .save_set_delete(TRAINING_ID, &set.set.id, &set_delete)
        .await
        .expect("the set delete must be saved");

    let (other_set, other_set_create) =
        training_set::create(TRAINING_ID, OTHER_EXERCISE_ROW_ID, 1, &set_values(80.0, 8))
            .expect("the other set");
    storage
        .save_set_create(&other_set.set, &other_set.segments, &other_set_create)
        .await
        .expect("the other set must be saved");
    let ordered_exercises = vec![OTHER_EXERCISE_ROW_ID.to_owned(), EXERCISE_ROW_ID.to_owned()];
    storage
        .save_training_exercise_reorder(
            TRAINING_ID,
            &ordered_exercises,
            &exercise_reorder_operation(REORDER_EXERCISE_ID, &ordered_exercises),
        )
        .await
        .expect("the exercise reorder must be saved");

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
        set_reorder.operation_id.as_str(),
        set_delete.operation_id.as_str(),
        REORDER_EXERCISE_ID,
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
    let members = SupersetMembers::new(vec![
        EXERCISE_ROW_ID.to_owned(),
        OTHER_EXERCISE_ROW_ID.to_owned(),
    ])
    .expect("a group of two");
    let (record, operation) =
        superset::create(TRAINING_ID, 1, &members).expect("the group creation");
    storage
        .save_superset_create(&record, &operation)
        .await
        .expect("the group must be saved");
    storage
        .mark_success(&operation.operation_id)
        .await
        .expect("the group creation must succeed");
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
    let groups = storage
        .training_supersets(TRAINING_ID)
        .await
        .expect("the groups must be readable");
    assert_eq!(
        groups[0]
            .members
            .iter()
            .map(|member| member.training_exercise_id.clone())
            .collect::<Vec<_>>(),
        vec![OTHER_EXERCISE_ROW_ID.to_owned(), EXERCISE_ROW_ID.to_owned()]
    );
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
    let groups = storage
        .training_supersets(TRAINING_ID)
        .await
        .expect("the groups must be readable");
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, EXERCISE_SUPERSET_ID);
    assert_eq!(
        groups[0]
            .members
            .iter()
            .map(|member| member.training_exercise_id.clone())
            .collect::<Vec<_>>(),
        vec![EXERCISE_ROW_ID.to_owned(), OTHER_EXERCISE_ROW_ID.to_owned()]
    );
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
