//! セットと段の端末内の保存、並べ替え、完了、削除のブラウザのテスト。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`、`REQ-NFR-003`、`REQ-NFR-005`。
//! セットの追加、更新、削除、並べ替え、完了が送信待ちの列を通り、段がセットの更新で
//! 全置換され、削除でセットと一緒に消えることを確かめる。

use warrior_core::set_calc::SetKind;
use warrior_core::sync::OperationKind;
use warrior_frontend::storage::Store;
use warrior_frontend::training_set::{self, SetValues};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{create_and_confirm, database_name, open, record_ids, TRAINING_ID};

const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5401";
const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5402";

fn values(kind: SetKind, weight_kg: f64, reps: u32) -> SetValues {
    SetValues {
        kind,
        weight_kg,
        reps,
        duration_seconds: None,
        body_weight_kg: Some(70.0),
        rpe: None,
        grip: None,
    }
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
        .save_set_create(&first.set, &first.segment, &operation)
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
        .save_set_create(&second.set, &second.segment, &operation)
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

    let (updated, operation) = training_set::update(
        TRAINING_ID,
        &first.set,
        &first.segment,
        &values(SetKind::Drop, 90.0, 8),
    )
    .expect("the update");
    storage
        .save_set_update(&updated.set, &updated.segment, &operation)
        .await
        .expect("the update must be saved");
    let stored = storage
        .training_sets(EXERCISE_ROW_ID)
        .await
        .expect("the sets must be readable");
    assert_eq!(stored[0].set.kind, "drop");
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
        .contains(&first.segment.id));
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
