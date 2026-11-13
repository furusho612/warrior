//! IndexedDB の保存、送信待ちの列、移行、アカウント削除のブラウザのテスト。
//!
//! 対応する要求: `REQ-NFR-003`、`REQ-NFR-004`、`REQ-NFR-005`、`REQ-NFR-010`。
//! テストは並列に走るため、データベースの名前をテストごとに分ける。

use std::cell::Cell;

use js_sys::Promise;
use serde_json::json;
use warrior_core::sync::{
    Operation, OperationKind, OutboxState, SessionCreatePayload, SessionRecord,
    SessionUpdatePayload,
};
use warrior_frontend::storage::{request_persistence, Storage, Store};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static SEQUENCE: Cell<u32> = const { Cell::new(0) };
}

const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4301";
const CREATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4302";
const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4303";
const SECOND_UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4304";

fn database_name(test: &str) -> String {
    let sequence = SEQUENCE.with(|sequence| {
        let next = sequence.get() + 1;
        sequence.set(next);
        next
    });
    format!(
        "warrior-test-{test}-{}-{sequence}",
        js_sys::Date::now() as u64
    )
}

fn session(memo: &str) -> SessionRecord {
    SessionRecord {
        id: TRAINING_ID.to_owned(),
        date: "2026-11-12".to_owned(),
        started_at: Some("2026-11-12T03:00:00Z".to_owned()),
        memo: memo.to_owned(),
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

fn create_operation(operation_id: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionCreate,
        parent_operation_id: None,
        payload: serde_json::to_value(SessionCreatePayload {
            id: TRAINING_ID.to_owned(),
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: String::new(),
        })
        .expect("a create payload"),
    }
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

#[wasm_bindgen_test]
async fn saves_a_session_and_its_operations_and_lists_the_outbox_in_order() {
    let name = database_name("order");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .save_session_update(&session("更新"), &update_operation(UPDATE_ID, "更新"))
        .await
        .expect("the update must be saved");

    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "更新");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].operation.sequence, 1);
    assert_eq!(entries[1].operation.sequence, 2);
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert_eq!(entries[1].operation.operation_id, UPDATE_ID);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_duplicate_operation_id_aborts_and_does_not_write_the_session() {
    let name = database_name("duplicate");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    let duplicate = storage
        .save_session_create(&session("changed"), &create_operation(CREATE_ID))
        .await;
    assert!(duplicate.is_err(), "the duplicate must be rejected");

    let sessions = storage
        .all_records(Store::TrainingSessions)
        .await
        .expect("the sessions must be readable");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["memo"], "");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn reuses_only_a_pending_update_of_the_same_training() {
    let name = database_name("reuse");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .save_session_update(&session("first"), &update_operation(UPDATE_ID, "first"))
        .await
        .expect("the first update must be saved");
    storage
        .save_session_update(
            &session("second"),
            &update_operation(SECOND_UPDATE_ID, "second"),
        )
        .await
        .expect("the second update must be saved");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].operation.operation_id, UPDATE_ID);
    assert_eq!(entries[1].operation.sequence, 2);
    assert_eq!(entries[1].operation.payload["memo"], "second");

    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the update must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the update must fail");
    storage
        .save_session_update(
            &session("third"),
            &update_operation(SECOND_UPDATE_ID, "third"),
        )
        .await
        .expect("the third update must be saved");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[1].state, OutboxState::Failed);
    assert_eq!(entries[2].operation.operation_id, SECOND_UPDATE_ID);
    assert_eq!(entries[2].operation.sequence, 3);
    assert_eq!(entries[2].operation.payload["memo"], "third");

    storage
        .retry(UPDATE_ID)
        .await
        .expect("the failed operation must be retryable");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    let retried = entries
        .iter()
        .find(|entry| entry.operation.operation_id == UPDATE_ID)
        .expect("the retried operation");
    assert_eq!(retried.state, OutboxState::Pending);
    assert_eq!(retried.attempts, 0);

    storage
        .discard(UPDATE_ID)
        .await
        .expect("the operation must be discardable");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert!(
        !entries
            .iter()
            .any(|entry| entry.operation.operation_id == UPDATE_ID),
        "the discarded operation must leave the outbox"
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn saving_does_not_depend_on_the_persistence_permission() {
    let name = database_name("persistence");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");

    // ブラウザの判断は制御できない。結果が false でも保存を続けることを、
    // 要求の結果を待った後に保存できることで確かめる。
    let _ = request_persistence().await;
    storage
        .save_session_create(&session("saved"), &create_operation(CREATE_ID))
        .await
        .expect("the save must not depend on the persistence permission");
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "saved");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_blocked_migration_keeps_the_data_and_can_be_retried() {
    let name = database_name("migration");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");
    storage
        .save_session_create(&session("keep"), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");

    let raw = raw_open(&name).await;
    let blocked = Storage::open_with(&name, 2).await;
    let error = blocked.err().expect("the blocked upgrade must fail");
    assert!(
        error.to_string().contains("blocked"),
        "the error must explain the blocked upgrade: {error}"
    );

    raw.close();
    let migrated = Storage::open_with(&name, 2)
        .await
        .expect("the retry must open the database");
    let stored = migrated
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must survive the failed migration");
    assert_eq!(stored.memo, "keep");

    migrated
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn clear_account_removes_the_database_the_outbox_and_advances_the_generation() {
    let name = database_name("clear");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");
    storage
        .save_session_create(&session("delete"), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    let generation = warrior_frontend::storage::generation();

    storage.clear_account().await.expect("the clear must work");
    assert!(warrior_frontend::storage::generation() > generation);
    assert_eq!(
        storage
            .session(TRAINING_ID)
            .await
            .expect("the session read must work"),
        None
    );
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox read must work")
        .is_empty());

    let reopened = Storage::open_with(&name, 1)
        .await
        .expect("the database must reopen");
    assert_eq!(
        reopened
            .session(TRAINING_ID)
            .await
            .expect("the session read must work"),
        None
    );
    assert!(reopened
        .outbox_entries()
        .await
        .expect("the outbox read must work")
        .is_empty());
    reopened
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn saves_and_restores_sets_and_segments() {
    let name = database_name("sets");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the database must open");

    let exercise = json!({
        "id": "exercise-1",
        "training_id": TRAINING_ID,
        "exercise_id": "master-1",
        "snapshot": "{}",
        "position": 0
    });
    let set = json!({
        "id": "set-1",
        "training_exercise_id": "exercise-1",
        "position": 0,
        "kind": "normal",
        "completed": true
    });
    let segment = json!({
        "id": "segment-1",
        "set_id": "set-1",
        "position": 0,
        "weight_kg": 100.0,
        "reps": 5
    });
    storage
        .put_record(Store::TrainingExercises, &exercise)
        .await
        .expect("the exercise must be saved");
    storage
        .put_record(Store::Sets, &set)
        .await
        .expect("the set must be saved");
    storage
        .put_record(Store::SetSegments, &segment)
        .await
        .expect("the segment must be saved");

    assert_eq!(
        storage
            .all_records(Store::TrainingExercises)
            .await
            .expect("the exercises must be readable"),
        vec![exercise]
    );
    assert_eq!(
        storage
            .all_records(Store::Sets)
            .await
            .expect("the sets must be readable"),
        vec![set]
    );
    let segments = storage
        .all_records(Store::SetSegments)
        .await
        .expect("the segments must be readable");
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0]["id"], "segment-1");
    assert_eq!(segments[0]["weight_kg"].as_f64(), Some(100.0));
    assert_eq!(segments[0]["reps"], 5);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

async fn raw_open(name: &str) -> web_sys::IdbDatabase {
    let factory = web_sys::window()
        .expect("a window")
        .indexed_db()
        .expect("IndexedDB")
        .expect("an IndexedDB factory");
    let request = factory.open(name).expect("the raw open request");
    let promise = Promise::new(&mut |resolve, reject| {
        let success_reject = reject.clone();
        let success = Closure::once_into_js(move |event: web_sys::Event| {
            let request: web_sys::IdbRequest = event
                .target()
                .expect("the request event target")
                .unchecked_into();
            match request.result() {
                Ok(value) => {
                    let _ = resolve.call1(&JsValue::NULL, &value);
                }
                Err(error) => {
                    let _ = success_reject.call1(&JsValue::NULL, &error);
                }
            }
        });
        request.set_onsuccess(Some(success.unchecked_ref()));
        let error = Closure::once_into_js(move |_event: web_sys::Event| {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("the raw open failed"));
        });
        request.set_onerror(Some(error.unchecked_ref()));
    });
    JsFuture::from(promise)
        .await
        .expect("the raw connection must open")
        .dyn_into()
        .expect("the raw connection is a database")
}
