//! セットと段の端末内の保存、並べ替え、完了、削除のブラウザのテスト。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`、`REQ-SET-003`、`REQ-SET-006`、`REQ-SET-007`、
//! `REQ-NFR-003`、`REQ-NFR-005`。
//! セットの追加、更新、削除、並べ替え、完了が送信待ちの列を通り、段がセットの更新で
//! 全置換され、削除でセットと一緒に消えることを確かめる。

use warrior_core::set_calc::{estimated_one_rep_max, RecordMode, SetKind, SideValue};
use warrior_core::sync::{
    Operation, OperationKind, SessionUpdatePayload, SetCreatePayload, SetUpdatePayload,
};
use warrior_core::training_record::{SetRecordRow, SetSegmentRecord, TrainingRecord};
use warrior_frontend::storage::{DiscardOutcome, Store};
use warrior_frontend::training_set::{self, SegmentSideValues, SetSegmentValues, SetValues};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{
    create_and_confirm, database_name, open, record_ids, server_record, session, TRAINING_ID,
};

const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5401";
const SECOND_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5410";
const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5402";
const MEMO_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5403";

fn memo_operation() -> Operation {
    let payload = SessionUpdatePayload {
        date: "2026-11-12".to_owned(),
        started_at: "2026-11-12T03:00:00Z".to_owned(),
        memo: "pending memo".to_owned(),
    };
    Operation {
        operation_id: MEMO_OPERATION_ID.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionUpdate,
        parent_operation_id: None,
        payload: serde_json::to_value(payload).expect("a memo payload"),
    }
}

fn server_set(
    set_id: &str,
    segment_id: &str,
    position: i64,
    weight_kg: f64,
    reps: i64,
) -> TrainingRecord {
    let mut record = server_record(TRAINING_ID, "");
    record.sets = vec![SetRecordRow {
        id: set_id.to_owned(),
        training_exercise_id: EXERCISE_ROW_ID.to_owned(),
        position,
        kind: "normal".to_owned(),
        completed: 0,
        rpe: None,
        grip: None,
        body_weight_kg: Some(70.0),
        recorded_at: None,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }];
    record.segments = vec![SetSegmentRecord {
        id: segment_id.to_owned(),
        set_id: set_id.to_owned(),
        position: 1,
        weight_kg: Some(weight_kg),
        reps: Some(reps),
        momentum_reps: 0,
        assisted_reps: 0,
        duration_seconds: None,
        left_weight_kg: None,
        left_reps: None,
        left_duration_seconds: None,
        right_weight_kg: None,
        right_reps: None,
        right_duration_seconds: None,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }];
    record
}

fn values(kind: SetKind, weight_kg: f64, reps: u32) -> SetValues {
    values_for(kind, &[(weight_kg, reps)])
}

fn values_for(kind: SetKind, segments: &[(f64, u32)]) -> SetValues {
    SetValues {
        kind,
        segments: segments
            .iter()
            .map(|(weight_kg, reps)| SetSegmentValues {
                weight_kg: *weight_kg,
                reps: *reps,
                momentum_reps: 0,
                assisted_reps: 0,
                duration_seconds: None,
                left: None,
                right: None,
            })
            .collect(),
        completed: false,
        body_weight_kg: Some(70.0),
        rpe: None,
        grip: None,
    }
}

#[wasm_bindgen_test]
async fn unilateral_sets_keep_both_sides_in_the_rows_and_the_payload() {
    let name = database_name("set-unilateral");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;

    let values = SetValues {
        kind: SetKind::Normal,
        segments: vec![SetSegmentValues {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left: Some(SegmentSideValues {
                weight_kg: 10.0,
                reps: 5,
            }),
            right: Some(SegmentSideValues {
                weight_kg: 12.0,
                reps: 4,
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

    let payload: SetCreatePayload =
        serde_json::from_value(operation.payload.clone()).expect("the create payload");
    assert_eq!(
        payload.segments[0].left,
        Some(SideValue {
            weight_kg: 10.0,
            reps: 5,
            duration_seconds: None,
        })
    );
    assert_eq!(
        payload.segments[0].right,
        Some(SideValue {
            weight_kg: 12.0,
            reps: 4,
            duration_seconds: None,
        })
    );
    let record = payload
        .to_record("unilateral", RecordMode::WeightAndReps)
        .expect("a record");
    assert_eq!(record.total_weight_kg(), 50.0 + 48.0);

    let mut partial = values.clone();
    partial.segments[0].right = Some(SegmentSideValues {
        weight_kg: 0.0,
        reps: 0,
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
async fn caches_the_body_weight_in_the_meta_store() {
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
    assert_eq!(
        storage
            .discard(&third_op.operation_id)
            .await
            .expect("the discard"),
        DiscardOutcome::SetDeleted
    );
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored.len(), 2);
    assert!(!stored.iter().any(|set| set.set.id == third.set.id));
    assert!(!record_ids(&storage, Store::SetSegments)
        .await
        .contains(&third.segments[0].id));
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox")
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
