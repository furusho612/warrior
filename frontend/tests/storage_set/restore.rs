//! セットの破棄と復元のブラウザのテスト。
//!
//! 対応する要求: `REQ-SET-002`、`REQ-SET-003`、`REQ-NFR-003`、`REQ-NFR-005`。
//! 失敗したセットの操作を破棄したときに、対象の行だけをサーバーの記録へ戻し、
//! より新しい送信待ちの操作があるときは復元を見送ることを確かめる。

use warrior_core::set_calc::SetKind;
use warrior_core::sync::OperationKind;
use warrior_frontend::storage::{DiscardOutcome, RestoreOutcome, Store};
use warrior_frontend::training_set;
use wasm_bindgen_test::wasm_bindgen_test;

use crate::support::set::{
    memo_operation, server_set, values, CREATE_OPERATION_ID, EXERCISE_ROW_ID,
};
use crate::support::{create_and_confirm, database_name, open, record_ids, session, TRAINING_ID};

#[wasm_bindgen_test]
async fn discarding_a_set_update_restores_the_set_and_keeps_the_memo() {
    let name = database_name("set-discard-update");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;
    storage
        .save_session_update(&session("pending memo"), &memo_operation())
        .await
        .expect("the memo must be saved");

    let (first, first_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the first set");
    storage
        .save_set_create(&first.set, &first.segments, &first_op)
        .await
        .expect("the first set must be saved");
    storage
        .mark_success(&first_op.operation_id)
        .await
        .expect("the first create must succeed");
    let (second, second_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        2,
        &values(SetKind::Normal, 80.0, 8),
    )
    .expect("the second set");
    storage
        .save_set_create(&second.set, &second.segments, &second_op)
        .await
        .expect("the second set must be saved");
    storage
        .mark_success(&second_op.operation_id)
        .await
        .expect("the second create must succeed");

    let (updated, update_op) = training_set::update(
        TRAINING_ID,
        &first.set,
        &first.segments,
        &values(SetKind::Normal, 120.0, 6),
    )
    .expect("the update");
    storage
        .save_set_update(&updated.set, &updated.segments, &update_op)
        .await
        .expect("the update must be saved");
    assert_eq!(
        storage
            .discard(&update_op.operation_id)
            .await
            .expect("the discard"),
        DiscardOutcome::OperationsRemoved
    );
    storage
        .restore_set(
            &server_set(&first.set.id, &first.segments[0].id, 1, 100.0, 5),
            &first.set.id,
            0,
        )
        .await
        .expect("the set must be restored");

    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 2);
    let restored = stored
        .iter()
        .find(|set| set.set.id == first.set.id)
        .expect("the restored set");
    assert_eq!(restored.segments[0].weight_kg, Some(100.0));
    assert_eq!(restored.segments[0].reps, Some(5));
    let other = stored
        .iter()
        .find(|set| set.set.id == second.set.id)
        .expect("the other set");
    assert_eq!(other.segments[0].weight_kg, Some(80.0));
    let entries = storage.outbox_entries().await.expect("the outbox");
    assert!(entries
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::TrainingSessionUpdate));
    assert!(!entries
        .iter()
        .any(|entry| entry.operation.operation_id == update_op.operation_id));
    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn restoring_a_set_keeps_a_pending_update() {
    let name = database_name("set-restore-pending");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let (set, create_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the set");
    storage
        .save_set_create(&set.set, &set.segments, &create_op)
        .await
        .expect("the set must be saved");
    storage
        .mark_success(&create_op.operation_id)
        .await
        .expect("the set create must succeed");

    let (failed_rows, failed_op) = training_set::update(
        TRAINING_ID,
        &set.set,
        &set.segments,
        &values(SetKind::Normal, 110.0, 6),
    )
    .expect("the failed update");
    storage
        .save_set_update(&failed_rows.set, &failed_rows.segments, &failed_op)
        .await
        .expect("the failed update must be saved");
    storage
        .mark_sending(&failed_op.operation_id)
        .await
        .expect("the failed update must be sending");
    storage
        .mark_permanent_failure(&failed_op.operation_id)
        .await
        .expect("the failed update must fail");
    assert_eq!(
        storage
            .discard(&failed_op.operation_id)
            .await
            .expect("the discard"),
        DiscardOutcome::OperationsRemoved
    );

    let (pending_rows, pending_op) = training_set::update(
        TRAINING_ID,
        &failed_rows.set,
        &failed_rows.segments,
        &values(SetKind::Normal, 120.0, 7),
    )
    .expect("the pending update");
    storage
        .save_set_update(&pending_rows.set, &pending_rows.segments, &pending_op)
        .await
        .expect("the pending update must be saved");

    storage
        .restore_set(
            &server_set(&set.set.id, &set.segments[0].id, 1, 100.0, 5),
            &set.set.id,
            failed_op.sequence,
        )
        .await
        .expect("the restore must not fail");

    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].segments[0].weight_kg, Some(120.0));
    assert_eq!(stored[0].segments[0].reps, Some(7));
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .any(|entry| entry.operation.operation_id == pending_op.operation_id));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn restoring_a_set_defers_for_a_newer_failed_update_and_a_reorder() {
    let name = database_name("set-restore-newer");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let (set, create_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the set");
    storage
        .save_set_create(&set.set, &set.segments, &create_op)
        .await
        .expect("the set must be saved");
    storage
        .mark_success(&create_op.operation_id)
        .await
        .expect("the set create must succeed");

    let (failed_a, op_a) = training_set::update(
        TRAINING_ID,
        &set.set,
        &set.segments,
        &values(SetKind::Normal, 110.0, 6),
    )
    .expect("the first failed update");
    storage
        .save_set_update(&failed_a.set, &failed_a.segments, &op_a)
        .await
        .expect("the first failed update must be saved");
    storage
        .mark_sending(&op_a.operation_id)
        .await
        .expect("the first failed update must be sending");
    storage
        .mark_permanent_failure(&op_a.operation_id)
        .await
        .expect("the first failed update must fail");
    assert_eq!(
        storage
            .discard(&op_a.operation_id)
            .await
            .expect("the discard"),
        DiscardOutcome::OperationsRemoved
    );

    let (failed_b, op_b) = training_set::update(
        TRAINING_ID,
        &failed_a.set,
        &failed_a.segments,
        &values(SetKind::Normal, 120.0, 7),
    )
    .expect("the second failed update");
    storage
        .save_set_update(&failed_b.set, &failed_b.segments, &op_b)
        .await
        .expect("the second failed update must be saved");
    storage
        .mark_sending(&op_b.operation_id)
        .await
        .expect("the second failed update must be sending");
    storage
        .mark_permanent_failure(&op_b.operation_id)
        .await
        .expect("the second failed update must fail");

    let server = server_set(&set.set.id, &set.segments[0].id, 1, 100.0, 5);
    assert_eq!(
        storage
            .restore_set_with_outcome(&server, &set.set.id, op_a.sequence)
            .await
            .expect("the restore"),
        RestoreOutcome::Deferred,
        "a newer failed update must defer the restore"
    );
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].segments[0].weight_kg, Some(120.0));
    assert_eq!(stored[0].segments[0].reps, Some(7));

    let ordered = vec![set.set.id.clone()];
    let reorder =
        training_set::reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered).expect("the pending reorder");
    storage
        .save_set_reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered, &reorder)
        .await
        .expect("the pending reorder must be saved");
    storage
        .discard(&op_b.operation_id)
        .await
        .expect("the second discard");
    assert_eq!(
        storage
            .restore_set_with_outcome(&server, &set.set.id, op_b.sequence)
            .await
            .expect("the second restore"),
        RestoreOutcome::Deferred,
        "a newer pending reorder must defer the restore"
    );
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].segments[0].weight_kg, Some(120.0));
    assert_eq!(stored[0].segments[0].reps, Some(7));
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

#[wasm_bindgen_test]
async fn discarding_a_set_create_removes_only_that_set_and_restores_positions() {
    let name = database_name("set-discard-create");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;
    storage
        .save_session_update(&session("pending memo"), &memo_operation())
        .await
        .expect("the memo must be saved");

    let (first, first_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the first set");
    storage
        .save_set_create(&first.set, &first.segments, &first_op)
        .await
        .expect("the first set must be saved");
    storage
        .mark_success(&first_op.operation_id)
        .await
        .expect("the first create must succeed");
    let (second, second_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        2,
        &values(SetKind::Normal, 80.0, 8),
    )
    .expect("the second set");
    storage
        .save_set_create(&second.set, &second.segments, &second_op)
        .await
        .expect("the second set must be saved");
    storage
        .mark_success(&second_op.operation_id)
        .await
        .expect("the second create must succeed");

    let ordered = vec![second.set.id.clone(), first.set.id.clone()];
    let reorder =
        training_set::reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered).expect("the reorder");
    storage
        .save_set_reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered, &reorder)
        .await
        .expect("the reorder must be saved");
    assert_eq!(
        storage
            .discard(&reorder.operation_id)
            .await
            .expect("the discard"),
        DiscardOutcome::OperationsRemoved
    );
    let mut server = server_set(&first.set.id, &first.segments[0].id, 1, 100.0, 5);
    let mut second_server = server_set(&second.set.id, &second.segments[0].id, 2, 80.0, 8);
    server.sets.append(&mut second_server.sets);
    server.segments.append(&mut second_server.segments);
    storage
        .restore_set_positions(&server)
        .await
        .expect("the positions must be restored");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.id, first.set.id);
    assert_eq!(stored[0].set.position, 1);
    assert_eq!(stored[1].set.id, second.set.id);
    assert_eq!(stored[1].set.position, 2);

    let (third, third_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        3,
        &values(SetKind::Drop, 60.0, 4),
    )
    .expect("the third set");
    storage
        .save_set_create(&third.set, &third.segments, &third_op)
        .await
        .expect("the third set must be saved");
    let (third_updated, third_update) = training_set::update(
        TRAINING_ID,
        &third.set,
        &third.segments,
        &values(SetKind::Drop, 70.0, 5),
    )
    .expect("the third update");
    storage
        .save_set_update(&third_updated.set, &third_updated.segments, &third_update)
        .await
        .expect("the third update must be saved");
    let (third_completed, third_complete) =
        training_set::complete(TRAINING_ID, &third.set, true).expect("the third completion");
    storage
        .save_set_complete(&third_completed, &third_complete)
        .await
        .expect("the third completion must be saved");
    let ordered_third = vec![
        second.set.id.clone(),
        third.set.id.clone(),
        first.set.id.clone(),
    ];
    let third_reorder = training_set::reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered_third)
        .expect("the third reorder");
    storage
        .save_set_reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered_third, &third_reorder)
        .await
        .expect("the third reorder must be saved");
    let third_delete = training_set::delete(TRAINING_ID, &third.set.id).expect("the third delete");
    storage
        .save_set_delete(TRAINING_ID, &third.set.id, &third_delete)
        .await
        .expect("the third delete must be saved");
    assert_eq!(
        storage
            .discard(&third_op.operation_id)
            .await
            .expect("the discard"),
        DiscardOutcome::SetDeleted
    );
    let entries = storage.outbox_entries().await.expect("the outbox");
    for operation_id in [
        third_update.operation_id.as_str(),
        third_complete.operation_id.as_str(),
        third_reorder.operation_id.as_str(),
        third_delete.operation_id.as_str(),
    ] {
        assert!(
            !entries
                .iter()
                .any(|entry| entry.operation.operation_id == operation_id),
            "the dependent operation must be removed: {operation_id}"
        );
    }
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 2);
    assert!(!stored.iter().any(|set| set.set.id == third.set.id));
    assert!(!record_ids(&storage, Store::SetSegments)
        .await
        .contains(&third.segments[0].id));
    assert!(entries
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::TrainingSessionUpdate));
    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_saved_update_does_not_recreate_a_deleted_set() {
    let name = database_name("set-update-after-delete");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let (set, create_op) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the set");
    storage
        .save_set_create(&set.set, &set.segments, &create_op)
        .await
        .expect("the set must be saved");
    let (updated, update_op) = training_set::update(
        TRAINING_ID,
        &set.set,
        &set.segments,
        &values(SetKind::Normal, 120.0, 6),
    )
    .expect("the update");
    let delete_op = training_set::delete(TRAINING_ID, &set.set.id).expect("the delete");
    storage
        .save_set_delete(TRAINING_ID, &set.set.id, &delete_op)
        .await
        .expect("the delete must be saved");
    storage
        .save_set_update(&updated.set, &updated.segments, &update_op)
        .await
        .expect("the late update must be ignored");

    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert!(
        stored.is_empty(),
        "the late update must not recreate the set"
    );
    assert!(!record_ids(&storage, Store::SetSegments)
        .await
        .contains(&set.segments[0].id));
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .all(|entry| entry.operation.operation_id != update_op.operation_id));
    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}
