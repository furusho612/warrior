//! スーパーセットの組の端末内の保存、読み出し、種目の削除と並べ替えへの追随。
//!
//! 対応する要求: `REQ-SET-005`。
//! 組の作成、種目の追加、並べ替え、削除が送信待ちの列を通り、種目の削除で組が
//! 解散し、種目の並べ替えで組の中の順序が変わることを確かめる。

use serde_json::json;
use warrior_core::superset::SupersetMembers;
use warrior_core::sync::{Operation, OperationKind};
use warrior_core::training_record::{SupersetRecord, TrainingExerciseRecord};
use warrior_frontend::storage::Store;
use warrior_frontend::{superset, training_exercise};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{create_and_confirm, database_name, open, record_ids, TRAINING_ID};

const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5701";
const FIRST_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5702";
const SECOND_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5703";
const THIRD_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5704";
const FIRST_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5705";
const SECOND_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5706";
const THIRD_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5707";

fn exercise_row(id: &str, exercise_id: &str, position: i64) -> TrainingExerciseRecord {
    TrainingExerciseRecord {
        id: id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        exercise_id: exercise_id.to_owned(),
        snapshot: "{}".to_owned(),
        position,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

fn create_exercise_operation(id: &str) -> Operation {
    Operation {
        operation_id: format!("3f2504e0-4f89-41d3-9a0c-0305e82c57{}", &id[35..36]),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseCreate,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "training_exercise_id": id,
            "exercise_id": "bench-press",
            "snapshot": {
                "exercise_id": "bench-press",
                "name": "ベンチプレス",
                "body_part": "chest",
                "muscles": [],
                "record_mode": "weight_and_reps",
                "laterality": "bilateral",
                "grip_mode": "none",
                "grip_options": []
            },
            "position": 1
        }),
    }
}

fn delete_exercise_operation(id: &str) -> Operation {
    let payload = warrior_core::sync::TrainingExerciseDeletePayload {
        training_id: TRAINING_ID.to_owned(),
        training_exercise_id: id.to_owned(),
    };
    Operation {
        operation_id: format!("3f2504e0-4f89-41d3-9a0c-0305e82c58{}", &id[35..36]),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseDelete,
        parent_operation_id: None,
        payload: serde_json::to_value(payload).expect("a delete payload"),
    }
}

async fn save_exercise(storage: &warrior_frontend::storage::Storage, row: TrainingExerciseRecord) {
    let operation = create_exercise_operation(&row.id);
    storage
        .save_training_exercise_create(&row, &operation)
        .await
        .expect("the exercise must be saved");
    storage
        .mark_success(&operation.operation_id)
        .await
        .expect("the exercise creation must succeed");
}

async fn outbox_kinds(storage: &warrior_frontend::storage::Storage) -> Vec<OperationKind> {
    storage
        .outbox_entries()
        .await
        .expect("the outbox")
        .into_iter()
        .map(|entry| entry.operation.kind)
        .collect()
}

#[wasm_bindgen_test]
async fn a_group_keeps_its_members_and_follows_the_exercise_order() {
    let name = database_name("superset-order");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;
    save_exercise(&storage, exercise_row(FIRST_ROW_ID, FIRST_EXERCISE_ID, 1)).await;
    save_exercise(&storage, exercise_row(SECOND_ROW_ID, SECOND_EXERCISE_ID, 2)).await;
    save_exercise(&storage, exercise_row(THIRD_ROW_ID, THIRD_EXERCISE_ID, 3)).await;

    let members = SupersetMembers::new(vec![FIRST_ROW_ID.to_owned(), SECOND_ROW_ID.to_owned()])
        .expect("a group of two");
    let (record, operation) =
        superset::create(TRAINING_ID, 1, &members).expect("the group creation");
    storage
        .save_superset_create(&record, &operation)
        .await
        .expect("the group must be saved");
    let groups = storage
        .training_supersets(TRAINING_ID)
        .await
        .expect("the groups must be readable");
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0]
            .members
            .iter()
            .map(|member| member.training_exercise_id.clone())
            .collect::<Vec<_>>(),
        vec![FIRST_ROW_ID.to_owned(), SECOND_ROW_ID.to_owned()]
    );

    let extended = SupersetMembers::new(vec![
        FIRST_ROW_ID.to_owned(),
        SECOND_ROW_ID.to_owned(),
        THIRD_ROW_ID.to_owned(),
    ])
    .expect("a group of three");
    let operation =
        superset::reorder(TRAINING_ID, &record.id, &extended).expect("the group reorder");
    storage
        .save_superset_reorder(TRAINING_ID, &record.id, extended.as_slice(), &operation)
        .await
        .expect("the group must be reordered");
    let groups = storage
        .training_supersets(TRAINING_ID)
        .await
        .expect("the groups must be readable");
    assert_eq!(groups[0].members.len(), 3);
    assert!(outbox_kinds(&storage)
        .await
        .contains(&OperationKind::SupersetReorder));

    let reordered = vec![
        THIRD_ROW_ID.to_owned(),
        SECOND_ROW_ID.to_owned(),
        FIRST_ROW_ID.to_owned(),
    ];
    let operation = training_exercise::reorder(TRAINING_ID, &reordered).expect("the reorder");
    storage
        .save_training_exercise_reorder(TRAINING_ID, &reordered, &operation)
        .await
        .expect("the exercises must be reordered");
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
        reordered
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn deleting_an_exercise_updates_the_group_and_dissolves_the_last_two() {
    let name = database_name("superset-delete");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;
    save_exercise(&storage, exercise_row(FIRST_ROW_ID, FIRST_EXERCISE_ID, 1)).await;
    save_exercise(&storage, exercise_row(SECOND_ROW_ID, SECOND_EXERCISE_ID, 2)).await;
    save_exercise(&storage, exercise_row(THIRD_ROW_ID, THIRD_EXERCISE_ID, 3)).await;

    let members = SupersetMembers::new(vec![
        FIRST_ROW_ID.to_owned(),
        SECOND_ROW_ID.to_owned(),
        THIRD_ROW_ID.to_owned(),
    ])
    .expect("a group of three");
    let (record, operation) =
        superset::create(TRAINING_ID, 1, &members).expect("the group creation");
    storage
        .save_superset_create(&record, &operation)
        .await
        .expect("the group must be saved");

    let operation = delete_exercise_operation(SECOND_ROW_ID);
    storage
        .save_training_exercise_delete(TRAINING_ID, SECOND_ROW_ID, &operation)
        .await
        .expect("the exercise must be deleted");
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
        vec![FIRST_ROW_ID.to_owned(), THIRD_ROW_ID.to_owned()]
    );

    let operation = delete_exercise_operation(FIRST_ROW_ID);
    storage
        .save_training_exercise_delete(TRAINING_ID, FIRST_ROW_ID, &operation)
        .await
        .expect("the exercise must be deleted");
    assert!(storage
        .training_supersets(TRAINING_ID)
        .await
        .expect("the groups must be readable")
        .is_empty());
    assert!(outbox_kinds(&storage)
        .await
        .contains(&OperationKind::SupersetDelete));
    assert_eq!(record_ids(&storage, Store::Supersets).await.len(), 0);
    assert_eq!(record_ids(&storage, Store::SupersetMembers).await.len(), 0);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn restoring_a_group_replaces_the_local_members() {
    let name = database_name("superset-restore");
    let storage = open(&name).await;
    create_and_confirm(&storage, CREATE_OPERATION_ID, "").await;
    let members = SupersetMembers::new(vec![FIRST_ROW_ID.to_owned(), SECOND_ROW_ID.to_owned()])
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

    let mut server = support::server_record(TRAINING_ID, "");
    server.supersets = vec![SupersetRecord {
        id: record.id.clone(),
        training_id: TRAINING_ID.to_owned(),
        position: 1,
        members: vec![
            warrior_core::training_record::SupersetMemberRecord {
                superset_id: record.id.clone(),
                training_exercise_id: SECOND_ROW_ID.to_owned(),
                position: 1,
            },
            warrior_core::training_record::SupersetMemberRecord {
                superset_id: record.id.clone(),
                training_exercise_id: THIRD_ROW_ID.to_owned(),
                position: 2,
            },
        ],
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }];
    storage
        .restore_superset(&server, &record.id)
        .await
        .expect("the group must be restored");
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
        vec![SECOND_ROW_ID.to_owned(), THIRD_ROW_ID.to_owned()]
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}
