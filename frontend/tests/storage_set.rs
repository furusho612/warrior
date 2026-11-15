//! セットと段の端末内の保存、並べ替え、完了、削除のブラウザのテスト。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`、`REQ-SET-003`、`REQ-SET-004`、`REQ-SET-006`、
//! `REQ-SET-007`、`REQ-NFR-003`、`REQ-NFR-005`。
//! セットの追加、更新、削除、並べ替え、完了が送信待ちの列を通り、段がセットの更新で
//! 全置換され、削除でセットと一緒に消えることを確かめる。時間の保存は `time.rs`、
//! 破棄と復元は `restore.rs` が持つ。

use warrior_core::set_calc::{estimated_one_rep_max, RecordMode, SetKind, Side};
use warrior_core::sync::{OperationKind, SetCreatePayload, SetSidePayload, SetUpdatePayload};
use warrior_frontend::storage::Store;
use warrior_frontend::training_set::{self, SegmentSideValues, SetSegmentValues, SetValues};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

#[path = "storage_set/restore.rs"]
mod restore;
mod support;
#[path = "storage_set/time.rs"]
mod time;

use support::set::{values, values_for, CREATE_OPERATION_ID, EXERCISE_ROW_ID, SECOND_SEGMENT_ID};
use support::{create_and_confirm, database_name, open, record_ids, TRAINING_ID};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn unilateral_sets_keep_both_sides_in_the_rows_and_the_payload() {
    let name = database_name("set-unilateral");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let values = SetValues {
        kind: SetKind::Normal,
        mode: RecordMode::WeightAndReps,
        segments: vec![SetSegmentValues {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 1,
            assisted_reps: 1,
            duration_seconds: None,
            left: Some(SegmentSideValues {
                weight_kg: Some(10.0),
                reps: 5,
                duration_seconds: None,
            }),
            right: Some(SegmentSideValues {
                weight_kg: Some(12.0),
                reps: 4,
                duration_seconds: None,
            }),
        }],
        completed: false,
        body_weight_kg: Some(70.0),
        rpe: None,
        grip: None,
    };
    let (rows, operation) =
        training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 1, &values).expect("a unilateral set");
    storage
        .save_set_create(&rows.set, &rows.segments, &operation)
        .await
        .expect("the unilateral set must be saved");

    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].segments[0].left_weight_kg, Some(10.0));
    assert_eq!(stored[0].segments[0].left_reps, Some(5));
    assert_eq!(stored[0].segments[0].right_weight_kg, Some(12.0));
    assert_eq!(stored[0].segments[0].right_reps, Some(4));
    assert_eq!(stored[0].segments[0].momentum_reps, 1);
    assert_eq!(stored[0].segments[0].assisted_reps, 1);

    let payload: SetCreatePayload =
        serde_json::from_value(operation.payload.clone()).expect("the create payload");
    assert_eq!(
        payload.segments[0].left,
        Some(SetSidePayload {
            weight_kg: Some(10.0),
            reps: 5,
            duration_seconds: None,
        })
    );
    assert_eq!(
        payload.segments[0].right,
        Some(SetSidePayload {
            weight_kg: Some(12.0),
            reps: 4,
            duration_seconds: None,
        })
    );
    assert_eq!(payload.segments[0].momentum_reps, 1);
    assert_eq!(payload.segments[0].assisted_reps, 1);
    let record = payload
        .to_record("unilateral", RecordMode::WeightAndReps)
        .expect("a record");
    let segment = &record.segments()[0];
    assert_eq!(segment.side(Side::Left).unwrap().strict_reps(1, 1), 3);
    assert_eq!(segment.side(Side::Right).unwrap().strict_reps(1, 1), 2);
    assert_eq!(record.total_weight_kg(), 76.0);
    assert_eq!(record.strict_total_weight_kg(), 54.0);
    assert_eq!(
        record.estimated_one_rep_max(),
        estimated_one_rep_max(12.0, 2)
    );

    let mut partial = values.clone();
    partial.segments[0].right = Some(SegmentSideValues {
        weight_kg: Some(0.0),
        reps: 0,
        duration_seconds: None,
    });
    assert!(!partial.completed_with_values());

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn set_rows_and_segments_use_the_outbox_and_keep_the_order() {
    let name = database_name("set-tree");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let (first, operation) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the first set");
    storage
        .save_set_create(&first.set, &first.segments, &operation)
        .await
        .expect("the first set must be saved");
    let (second, operation) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        2,
        &values(SetKind::Warmup, 80.0, 6),
    )
    .expect("the second set");
    storage
        .save_set_create(&second.set, &second.segments, &operation)
        .await
        .expect("the second set must be saved");

    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].set.kind, "normal");
    assert_eq!(stored[1].set.kind, "warmup");
    assert_eq!(stored[0].segments.len(), 1);
    assert_eq!(stored[0].segments[0].weight_kg, Some(100.0));
    assert_eq!(stored[0].segments[0].reps, Some(5));
    assert_eq!(stored[0].set.body_weight_kg, Some(70.0));

    let mut second_segment = first.segments[0].clone();
    second_segment.id = SECOND_SEGMENT_ID.to_owned();
    second_segment.position = 2;
    second_segment.weight_kg = Some(80.0);
    second_segment.reps = Some(8);
    let drop_segments = vec![first.segments[0].clone(), second_segment];
    let (updated, operation) = training_set::update(
        TRAINING_ID,
        &first.set,
        &drop_segments,
        &values_for(SetKind::Drop, &[(90.0, 8), (80.0, 8)]),
    )
    .expect("the update with two segments");
    storage
        .save_set_update(&updated.set, &updated.segments, &operation)
        .await
        .expect("the update must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.kind, "drop");
    assert_eq!(stored[0].segments.len(), 2);
    assert_eq!(stored[0].segments[0].weight_kg, Some(90.0));
    assert_eq!(stored[0].segments[1].weight_kg, Some(80.0));

    let (updated, operation) = training_set::update(
        TRAINING_ID,
        &updated.set,
        &updated.segments[..1],
        &values(SetKind::Drop, 90.0, 8),
    )
    .expect("the update after removing a segment");
    storage
        .save_set_update(&updated.set, &updated.segments, &operation)
        .await
        .expect("the update must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].segments.len(), 1);
    assert_eq!(stored[0].segments[0].weight_kg, Some(90.0));
    assert_eq!(stored[0].segments[0].reps, Some(8));

    let (completed, operation) =
        training_set::complete(TRAINING_ID, &second.set, true).expect("the completion");
    storage
        .save_set_complete(&completed, &operation)
        .await
        .expect("the completion must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[1].set.completed, 1);

    let ordered = vec![second.set.id.clone(), first.set.id.clone()];
    let operation =
        training_set::reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered).expect("the reorder");
    storage
        .save_set_reorder(TRAINING_ID, EXERCISE_ROW_ID, &ordered, &operation)
        .await
        .expect("the reorder must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.id, second.set.id);
    assert_eq!(stored[0].set.position, 1);
    assert_eq!(stored[1].set.id, first.set.id);
    assert_eq!(stored[1].set.position, 2);

    let kinds = storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .into_iter()
        .map(|entry| entry.operation.kind)
        .collect::<Vec<_>>();
    for kind in [
        OperationKind::SetCreate,
        OperationKind::SetUpdate,
        OperationKind::SetComplete,
        OperationKind::SetReorder,
    ] {
        assert!(kinds.contains(&kind), "the outbox must contain {kind:?}");
    }

    let operation = training_set::delete(TRAINING_ID, &first.set.id).expect("the delete");
    storage
        .save_set_delete(TRAINING_ID, &first.set.id, &operation)
        .await
        .expect("the delete must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].set.id, second.set.id);
    assert_eq!(stored[0].segments.len(), 1);
    assert!(!record_ids(&storage, Store::SetSegments)
        .await
        .contains(&first.segments[0].id));
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .any(|entry| entry.operation.kind == OperationKind::SetDelete));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn caches_the_body_weight_in_its_own_store() {
    let name = database_name("body-weight-cache");
    let storage = open(&name).await;
    assert_eq!(storage.cached_body_weight().await.expect("the cache"), None);
    storage
        .cache_body_weight(Some(70.0))
        .await
        .expect("the weight must be cached");
    assert_eq!(
        storage.cached_body_weight().await.expect("the cache"),
        Some(70.0)
    );
    let records = storage
        .all_records(Store::BodyWeight)
        .await
        .expect("the cache store");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["key"], "profile_body_weight");
    assert_eq!(records[0]["body_weight_kg"].as_f64(), Some(70.0));
    storage
        .cache_body_weight(None)
        .await
        .expect("the cache must be cleared");
    assert_eq!(storage.cached_body_weight().await.expect("the cache"), None);
    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn req_set_006_and_007_rows_keep_the_breakdown_and_rpe_in_the_outbox() {
    let name = database_name("set-breakdown-rpe");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let mut values = values(SetKind::Normal, 100.0, 10);
    values.rpe = Some(8.0);
    values.segments[0].momentum_reps = 2;
    values.segments[0].assisted_reps = 1;
    let (rows, operation) =
        training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 1, &values).expect("the set");
    assert_eq!(rows.set.rpe, Some(8.0));
    assert_eq!(rows.segments[0].momentum_reps, 2);
    assert_eq!(rows.segments[0].assisted_reps, 1);
    let payload: SetCreatePayload =
        serde_json::from_value(operation.payload.clone()).expect("the create payload");
    assert_eq!(payload.rpe, Some(8.0));
    assert_eq!(payload.segments[0].momentum_reps, 2);
    assert_eq!(payload.segments[0].assisted_reps, 1);
    let record = payload
        .to_record("bench-press", RecordMode::WeightAndReps)
        .expect("a record");
    assert_eq!(record.total_weight_kg(), 900.0);
    assert_eq!(record.strict_total_weight_kg(), 700.0);
    let expected = estimated_one_rep_max(100.0, 7).expect("the estimated 1RM");
    assert!((record.estimated_one_rep_max().expect("the 1RM") - expected).abs() < 1e-9);
    storage
        .save_set_create(&rows.set, &rows.segments, &operation)
        .await
        .expect("the set must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.rpe, Some(8.0));
    assert_eq!(stored[0].segments[0].momentum_reps, 2);
    assert_eq!(stored[0].segments[0].assisted_reps, 1);
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .any(|entry| entry.operation.operation_id == operation.operation_id));

    let mut updated_values = values.clone();
    updated_values.rpe = Some(6.5);
    updated_values.segments[0].momentum_reps = 3;
    updated_values.segments[0].assisted_reps = 2;
    let (updated, update_operation) =
        training_set::update(TRAINING_ID, &rows.set, &rows.segments, &updated_values)
            .expect("the update");
    storage
        .save_set_update(&updated.set, &updated.segments, &update_operation)
        .await
        .expect("the update must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.rpe, Some(6.5));
    assert_eq!(stored[0].segments[0].momentum_reps, 3);
    assert_eq!(stored[0].segments[0].assisted_reps, 2);
    let payload: SetUpdatePayload =
        serde_json::from_value(update_operation.payload.clone()).expect("the update payload");
    assert_eq!(payload.rpe, Some(6.5));
    assert_eq!(payload.segments[0].momentum_reps, 3);
    assert_eq!(payload.segments[0].assisted_reps, 2);

    let mut empty_rpe = updated_values.clone();
    empty_rpe.rpe = None;
    empty_rpe.segments[0].momentum_reps = 0;
    empty_rpe.segments[0].assisted_reps = 0;
    training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 2, &empty_rpe)
        .expect("an empty RPE and an empty breakdown are allowed");

    for rpe in [5.5, 10.5, 7.25] {
        let mut invalid = updated_values.clone();
        invalid.rpe = Some(rpe);
        assert!(
            training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 2, &invalid).is_err(),
            "the RPE {rpe} must be rejected"
        );
    }
    let mut invalid = updated_values.clone();
    invalid.segments[0].momentum_reps = 6;
    invalid.segments[0].assisted_reps = 5;
    assert!(
        training_set::create(TRAINING_ID, EXERCISE_ROW_ID, 2, &invalid).is_err(),
        "the breakdown over the reps must be rejected"
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn set_operations_reject_a_row_that_differs_from_the_payload() {
    let name = database_name("set-operation-mismatch");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let (rows, create_operation) = training_set::create(
        TRAINING_ID,
        EXERCISE_ROW_ID,
        1,
        &values(SetKind::Normal, 100.0, 5),
    )
    .expect("the set");
    let mut other_rpe = rows.set.clone();
    other_rpe.rpe = Some(8.0);
    assert!(
        storage
            .save_set_create(&other_rpe, &rows.segments, &create_operation)
            .await
            .is_err(),
        "a created row with another RPE must be rejected"
    );
    let mut already_completed = rows.set.clone();
    already_completed.completed = 1;
    assert!(
        storage
            .save_set_create(&already_completed, &rows.segments, &create_operation)
            .await
            .is_err(),
        "a created row that is already completed must be rejected"
    );
    assert!(storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable")
        .is_empty());
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .all(|entry| entry.operation.operation_id != create_operation.operation_id));

    storage
        .save_set_create(&rows.set, &rows.segments, &create_operation)
        .await
        .expect("the set must be saved");

    let updated_values = values(SetKind::Normal, 110.0, 6);
    let (updated, update_operation) =
        training_set::update(TRAINING_ID, &rows.set, &rows.segments, &updated_values)
            .expect("the update");
    let mut other_rpe = updated.set.clone();
    other_rpe.rpe = Some(8.0);
    assert!(
        storage
            .save_set_update(&other_rpe, &updated.segments, &update_operation)
            .await
            .is_err(),
        "an updated row with another RPE must be rejected"
    );
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.rpe, None);
    assert_eq!(stored[0].segments[0].weight_kg, Some(100.0));
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .all(|entry| entry.operation.operation_id != update_operation.operation_id));

    let (completed, complete_operation) =
        training_set::complete(TRAINING_ID, &rows.set, true).expect("the completion");
    let mut uncompleted = completed.clone();
    uncompleted.completed = 0;
    assert!(
        storage
            .save_set_complete(&uncompleted, &complete_operation)
            .await
            .is_err(),
        "a completion whose row disagrees with the payload must be rejected"
    );
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.completed, 0);
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .iter()
        .all(|entry| entry.operation.operation_id != complete_operation.operation_id));

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}
