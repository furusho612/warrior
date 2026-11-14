//! IndexedDB の保存、送信待ちの列、編集権を失った記録、サーバーの記録での置き換え、
//! 移行、アカウント削除のブラウザのテスト。
//!
//! 対応する要求: `REQ-NFR-003`、`REQ-NFR-004`、`REQ-NFR-005`、`REQ-NFR-010`。
//! テストは並列に走るため、データベースの名前をテストごとに分ける。
//! 永続ストレージの拒否を強制する API はないため、`request_persistence` の結果に
//! かかわらず保存が続くことで、保存が許可の結果に依存しないことを確かめる。
//! セットと段は `put_record` で保存し、outbox の操作は作らない。0021 にはセットの
//! 同期操作がなく、0025 が型付きの `save_set` を追加する。

use std::cell::Cell;

use js_sys::Promise;
use serde_json::json;
use warrior_core::exercise::Laterality;
use warrior_core::set_calc::RecordMode;
use warrior_core::sync::{
    next_sendable, Operation, OperationKind, OutboxState, SessionCreatePayload, SessionRecord,
    SessionUpdatePayload,
};
use warrior_core::training_record::TrainingRecord;
use warrior_core::training_session::{BestKind, PersonalBestItem, SessionSource};
use warrior_frontend::storage::{
    request_persistence, DiscardOutcome, ReplacePurpose, Storage, StorageError, Store,
};
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
const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4300";
const CREATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4302";
const OTHER_CREATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4306";
const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4303";
const SECOND_UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4304";
const THIRD_UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4305";

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
    session_with_id(TRAINING_ID, memo)
}

fn session_with_id(id: &str, memo: &str) -> SessionRecord {
    SessionRecord {
        id: id.to_owned(),
        date: "2026-11-12".to_owned(),
        started_at: Some("2026-11-12T03:00:00Z".to_owned()),
        memo: memo.to_owned(),
        confirmed_memo: String::new(),
        confirmed_sequence: 0,
        source_kind: SessionSource::Empty,
        routine_id: None,
        program_day_id: None,
        custom_program_day_id: None,
        finished_at: None,
        duration_seconds: None,
        total_weight_kg: None,
        exercise_count: None,
        set_count: None,
        personal_bests: Vec::new(),
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

fn create_operation(operation_id: &str) -> Operation {
    create_operation_with_memo(operation_id, "")
}

fn create_operation_with_memo(operation_id: &str, memo: &str) -> Operation {
    create_operation_for(TRAINING_ID, operation_id, memo)
}

fn create_operation_for(training_id: &str, operation_id: &str, memo: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: training_id.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionCreate,
        parent_operation_id: None,
        payload: serde_json::to_value(SessionCreatePayload {
            id: training_id.to_owned(),
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: memo.to_owned(),
            source_kind: SessionSource::Empty,
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
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

async fn open(name: &str) -> Storage {
    Storage::open_with(name, 2)
        .await
        .expect("the database must open")
}

fn server_record(training_id: &str, memo: &str) -> TrainingRecord {
    serde_json::from_value(json!({
        "session": {
            "id": training_id,
            "date": "2026-11-12",
            "started_at": "2026-11-12T03:00:00Z",
            "memo": memo,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T04:00:00Z"
        },
        "exercises": [{
            "id": "exercise-1",
            "training_id": training_id,
            "exercise_id": "bench-press",
            "snapshot": "{}",
            "position": 1,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }],
        "sets": [{
            "id": "set-1",
            "training_exercise_id": "exercise-1",
            "position": 1,
            "kind": "normal",
            "completed": 1,
            "rpe": 8.0,
            "grip": null,
            "recorded_at": null,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }],
        "segments": [{
            "id": "segment-1",
            "set_id": "set-1",
            "position": 1,
            "weight_kg": 100.0,
            "reps": 5,
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
        }],
        "supersets": [{
            "id": "superset-1",
            "training_id": training_id,
            "position": 1,
            "members": [{
                "superset_id": "superset-1",
                "training_exercise_id": "exercise-1",
                "position": 1
            }],
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }]
    }))
    .expect("the server record must deserialize")
}

/// メモをサーバーが受理した状態のトレーニングを作る。
async fn create_and_confirm(storage: &Storage, memo: &str) {
    storage
        .save_session_create(&session(memo), &create_operation_with_memo(CREATE_ID, memo))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
}

#[wasm_bindgen_test]
async fn saves_a_session_and_its_operations_and_lists_the_outbox_in_order() {
    let name = database_name("order");
    let storage = open(&name).await;

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
async fn active_session_ignores_missing_start_times_and_finished_sessions() {
    let name = database_name("active-session");
    let storage = open(&name).await;

    for (id, started_at, updated_at) in [
        (
            "3f2504e0-4f89-41d3-9a0c-0305e82c4310",
            Some("2026-11-12T03:00:00Z"),
            "2026-11-12T03:00:00Z",
        ),
        (
            "3f2504e0-4f89-41d3-9a0c-0305e82c4311",
            None,
            "2026-11-14T03:00:00Z",
        ),
        (
            "3f2504e0-4f89-41d3-9a0c-0305e82c4312",
            Some("2026-11-11T03:00:00Z"),
            "2026-11-11T03:00:00Z",
        ),
        (
            "3f2504e0-4f89-41d3-9a0c-0305e82c4313",
            Some("2026-11-13T03:00:00Z"),
            "2026-11-13T03:00:00Z",
        ),
    ] {
        storage
            .put_record(
                Store::TrainingSessions,
                &json!({
                    "id": id,
                    "date": "2026-11-12",
                    "started_at": started_at,
                    "memo": id,
                    "confirmed_memo": "",
                    "confirmed_sequence": 0,
                    "created_at": updated_at,
                    "updated_at": updated_at
                }),
            )
            .await
            .expect("the session must be saved");
    }

    storage
        .put_record(
            Store::TrainingSessions,
            &json!({
                "id": "3f2504e0-4f89-41d3-9a0c-0305e82c4314",
                "date": "2026-11-12",
                "started_at": "2026-11-14T03:00:00Z",
                "memo": "終了済み",
                "confirmed_memo": "",
                "confirmed_sequence": 0,
                "finished_at": "2026-11-14T04:00:00Z",
                "created_at": "2026-11-14T03:00:00Z",
                "updated_at": "2026-11-14T04:00:00Z"
            }),
        )
        .await
        .expect("the finished session must be saved");

    let active = storage
        .active_session()
        .await
        .expect("the sessions must be readable")
        .expect("an active session");
    assert_eq!(active.id, "3f2504e0-4f89-41d3-9a0c-0305e82c4313");
    assert_eq!(active.started_at.as_deref(), Some("2026-11-13T03:00:00Z"));
    assert_eq!(active.memo, "3f2504e0-4f89-41d3-9a0c-0305e82c4313");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn saving_a_finished_summary_removes_the_session_from_the_active_session() {
    let name = database_name("finished-summary");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    let mut finished = session("終了");
    finished.finished_at = Some("2026-11-12T04:00:00Z".to_owned());
    finished.duration_seconds = Some(3_600);
    finished.total_weight_kg = Some(1_640.0);
    finished.exercise_count = Some(2);
    finished.set_count = Some(3);
    finished.personal_bests = vec![PersonalBestItem {
        exercise_id: "bench-press".to_owned(),
        exercise_name: Some("ベンチプレス".to_owned()),
        record_mode: RecordMode::WeightAndReps,
        laterality: Laterality::Bilateral,
        grip: None,
        attachment: None,
        kind: BestKind::MaxWeight,
        value: 100.0,
    }];
    storage
        .save_session_summary(&finished)
        .await
        .expect("the summary must be saved");

    assert!(storage
        .active_session()
        .await
        .expect("the sessions must be readable")
        .is_none());
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.finished_at.as_deref(), Some("2026-11-12T04:00:00Z"));
    assert_eq!(stored.duration_seconds, Some(3_600));
    assert_eq!(stored.total_weight_kg, Some(1_640.0));
    assert_eq!(stored.exercise_count, Some(2));
    assert_eq!(stored.set_count, Some(3));
    assert_eq!(stored.personal_bests, finished.personal_bests);
    assert!(
        storage
            .save_session_summary(&session("未終了"))
            .await
            .is_err(),
        "an unfinished session cannot store a summary"
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_duplicate_operation_id_aborts_and_does_not_write_the_session() {
    let name = database_name("duplicate");
    let storage = open(&name).await;

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
async fn a_duplicate_update_operation_id_aborts_and_keeps_the_session() {
    let name = database_name("duplicate-update");
    let storage = open(&name).await;

    storage
        .save_session_create(
            &session("base"),
            &create_operation_with_memo(CREATE_ID, "base"),
        )
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_session_update(&session("first"), &update_operation(UPDATE_ID, "first"))
        .await
        .expect("the first update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the update must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the update must fail");

    let duplicate = storage
        .save_session_update(
            &session("failed draft"),
            &update_operation(UPDATE_ID, "failed draft"),
        )
        .await;
    assert!(duplicate.is_err(), "the duplicate must be rejected");

    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "first");
    assert_eq!(stored.confirmed_memo, "base");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].state, OutboxState::Failed);
    assert_eq!(entries[0].operation.payload["memo"], "first");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn only_an_unattempted_pending_update_is_merged() {
    let name = database_name("reuse");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
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
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation.operation_id, UPDATE_ID);
    assert_eq!(entries[0].operation.sequence, 2);
    assert_eq!(entries[0].operation.payload["memo"], "second");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn an_attempted_update_is_not_merged_and_the_latest_value_is_last() {
    let name = database_name("attempted");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_session_update(&session("x"), &update_operation(UPDATE_ID, "x"))
        .await
        .expect("the first update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the update must be sending");
    storage
        .mark_transient_failure(UPDATE_ID)
        .await
        .expect("the update must fail transiently");

    storage
        .save_session_update(&session("y"), &update_operation(SECOND_UPDATE_ID, "y"))
        .await
        .expect("the second update must be saved");
    storage
        .save_session_update(&session("z"), &update_operation(THIRD_UPDATE_ID, "z"))
        .await
        .expect("the third update must be saved");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].operation.operation_id, UPDATE_ID);
    assert_eq!(entries[0].operation.payload["memo"], "x");
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert!(entries[0].attempted);
    assert_eq!(entries[1].operation.operation_id, SECOND_UPDATE_ID);
    assert_eq!(entries[1].operation.payload["memo"], "z");
    assert_eq!(entries[1].operation.sequence, 3);

    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the attempted operation must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the attempted operation must fail");
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
async fn a_create_parent_blocks_updates_until_it_is_resolved() {
    let name = database_name("parent");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the update must be saved");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries[1].state, OutboxState::Blocked);
    assert_eq!(
        entries[1].operation.parent_operation_id.as_deref(),
        Some(CREATE_ID)
    );

    storage
        .mark_sending(CREATE_ID)
        .await
        .expect("the create must be sending");
    storage
        .mark_permanent_failure(CREATE_ID)
        .await
        .expect("the create must fail");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries[0].state, OutboxState::Failed);
    assert_eq!(entries[1].state, OutboxState::Blocked);

    let outcome = storage
        .discard(CREATE_ID)
        .await
        .expect("the create must be discarded");
    assert_eq!(outcome, DiscardOutcome::SessionDeleted);
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

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_resolved_create_keeps_the_merged_update_sendable() {
    let name = database_name("resolved-parent");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .save_session_update(&session("first"), &update_operation(UPDATE_ID, "first"))
        .await
        .expect("the update must be saved");
    storage
        .mark_sending(CREATE_ID)
        .await
        .expect("the create must be sending");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_session_update(&session("second"), &update_operation(UPDATE_ID, "second"))
        .await
        .expect("the merged update must be saved");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert_eq!(
        entries[0].operation.parent_operation_id.as_deref(),
        Some(CREATE_ID)
    );
    assert_eq!(entries[0].operation.payload["memo"], "second");
    assert_eq!(
        next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
        Some(UPDATE_ID)
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_a_failed_create_removes_the_session_tree_and_outbox() {
    let name = database_name("discard-create");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    storage
        .save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the update must be saved");
    storage
        .put_record(
            Store::TrainingExercises,
            &json!({"id": "exercise-1", "training_id": TRAINING_ID, "position": 0}),
        )
        .await
        .expect("the exercise must be saved");
    storage
        .put_record(
            Store::Sets,
            &json!({"id": "set-1", "training_exercise_id": "exercise-1", "position": 0}),
        )
        .await
        .expect("the set must be saved");
    storage
        .put_record(
            Store::SetSegments,
            &json!({"id": "segment-1", "set_id": "set-1", "position": 0}),
        )
        .await
        .expect("the segment must be saved");
    storage
        .put_record(
            Store::Supersets,
            &json!({"id": "superset-1", "training_id": TRAINING_ID, "position": 0}),
        )
        .await
        .expect("the superset must be saved");
    storage
        .put_record(
            Store::SupersetMembers,
            &json!({
                "superset_id": "superset-1",
                "training_exercise_id": "exercise-1",
                "position": 0
            }),
        )
        .await
        .expect("the member must be saved");
    storage
        .mark_sending(CREATE_ID)
        .await
        .expect("the create must be sending");
    storage
        .mark_permanent_failure(CREATE_ID)
        .await
        .expect("the create must fail");

    let outcome = storage
        .discard(CREATE_ID)
        .await
        .expect("the create must be discarded");
    assert_eq!(outcome, DiscardOutcome::SessionDeleted);
    for store in [
        Store::TrainingSessions,
        Store::TrainingExercises,
        Store::Sets,
        Store::SetSegments,
        Store::Supersets,
        Store::SupersetMembers,
    ] {
        assert!(
            storage
                .all_records(store)
                .await
                .expect("the store must be readable")
                .is_empty(),
            "{store:?} must be empty"
        );
    }
    assert!(storage
        .outbox_entries()
        .await
        .expect("the outbox read must work")
        .is_empty());

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_a_failed_update_reverts_the_memo_to_the_confirmed_value() {
    let name = database_name("discard-update");
    let storage = open(&name).await;

    storage
        .save_session_create(
            &session("base"),
            &create_operation_with_memo(CREATE_ID, "base"),
        )
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the update must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the update must fail");

    let outcome = storage
        .discard(UPDATE_ID)
        .await
        .expect("the update must be discarded");
    let stored = match outcome {
        DiscardOutcome::MemoReverted(session) => session,
        other => panic!("the discarded update must revert the memo: {other:?}"),
    };
    assert_eq!(stored.memo, "base");
    assert_eq!(stored.confirmed_memo, "base");
    assert_ne!(stored.memo, "draft");
    let persisted = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(persisted.memo, "base");
    assert_eq!(persisted.confirmed_memo, "base");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_failed_save_keeps_the_session_and_confirmed_memo() {
    let name = database_name("failed-save");
    let storage = open(&name).await;

    storage
        .save_session_create(
            &session("base"),
            &create_operation_with_memo(CREATE_ID, "base"),
        )
        .await
        .expect("the create must be saved");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    storage
        .save_session_update(&session("first"), &update_operation(UPDATE_ID, "first"))
        .await
        .expect("the first update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the update must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the update must fail");

    let result = storage
        .save_session_update(
            &session("failed draft"),
            &update_operation(UPDATE_ID, "failed draft"),
        )
        .await;
    assert!(result.is_err());
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "first");
    assert_eq!(stored.confirmed_memo, "base");

    storage
        .save_session_update(
            &session("second"),
            &update_operation(SECOND_UPDATE_ID, "second"),
        )
        .await
        .expect("the successful update must be saved");
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "second");
    assert_eq!(stored.confirmed_memo, "base");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn retrying_a_superseded_failed_update_keeps_the_confirmed_memo() {
    let name = database_name("retry-confirmed");
    let storage = open(&name).await;
    create_and_confirm(&storage, "c").await;

    storage
        .save_session_update(&session("x"), &update_operation(UPDATE_ID, "x"))
        .await
        .expect("the older update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the older update must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the older update must fail");

    storage
        .save_session_update(&session("y"), &update_operation(SECOND_UPDATE_ID, "y"))
        .await
        .expect("the newer update must be saved");
    storage
        .mark_sending(SECOND_UPDATE_ID)
        .await
        .expect("the newer update must be sending");
    storage
        .mark_success(SECOND_UPDATE_ID)
        .await
        .expect("the newer update must succeed");

    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "y");
    assert_eq!(stored.confirmed_memo, "y");
    assert_eq!(stored.confirmed_sequence, 3);
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert!(
        !entries
            .iter()
            .any(|entry| entry.operation.operation_id == UPDATE_ID),
        "the superseded failed update must leave the outbox"
    );

    storage
        .retry(UPDATE_ID)
        .await
        .expect("retrying a superseded update must do nothing");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert!(entries.is_empty());
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "y");
    assert_eq!(stored.confirmed_memo, "y");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_confirmed_update_is_not_retried() {
    let name = database_name("retry-guard");
    let storage = open(&name).await;
    create_and_confirm(&storage, "c").await;

    storage
        .save_session_update(&session("x"), &update_operation(UPDATE_ID, "x"))
        .await
        .expect("the older update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the older update must be sending");
    storage
        .save_session_update(&session("y"), &update_operation(SECOND_UPDATE_ID, "y"))
        .await
        .expect("the newer update must be saved");
    storage
        .mark_success(SECOND_UPDATE_ID)
        .await
        .expect("the newer update must succeed out of order");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the older update must fail after the newer success");

    storage
        .retry(UPDATE_ID)
        .await
        .expect("a confirmed update must not be retried");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation.operation_id, UPDATE_ID);
    assert_eq!(entries[0].state, OutboxState::Failed);
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.confirmed_memo, "y");
    assert_eq!(stored.confirmed_sequence, 3);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn discarding_an_older_failed_update_keeps_the_newer_pending_memo() {
    let name = database_name("discard-older");
    let storage = open(&name).await;
    create_and_confirm(&storage, "c").await;

    storage
        .save_session_update(&session("x"), &update_operation(UPDATE_ID, "x"))
        .await
        .expect("the older update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the older update must be sending");
    storage
        .mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the older update must fail");
    storage
        .save_session_update(&session("y"), &update_operation(SECOND_UPDATE_ID, "y"))
        .await
        .expect("the newer update must be saved");

    let outcome = storage
        .discard(UPDATE_ID)
        .await
        .expect("the older update must be discarded");
    assert_eq!(outcome, DiscardOutcome::OperationsRemoved);
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "y");
    assert_eq!(stored.confirmed_memo, "c");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation.operation_id, SECOND_UPDATE_ID);

    let outcome = storage
        .discard(SECOND_UPDATE_ID)
        .await
        .expect("the newer update must be discarded");
    let reverted = match outcome {
        DiscardOutcome::MemoReverted(session) => session,
        other => panic!("the newest update must revert the memo: {other:?}"),
    };
    assert_eq!(reverted.memo, "c");
    assert_eq!(reverted.confirmed_memo, "c");
    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "c");

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn updates_merge_while_the_create_is_pending_and_unblock_after_success() {
    let name = database_name("merge-blocked");
    let storage = open(&name).await;

    storage
        .save_session_create(&session(""), &create_operation(CREATE_ID))
        .await
        .expect("the create must be saved");
    for (operation_id, memo) in [
        (UPDATE_ID, "a"),
        (SECOND_UPDATE_ID, "b"),
        (THIRD_UPDATE_ID, "c"),
    ] {
        storage
            .save_session_update(&session(memo), &update_operation(operation_id, memo))
            .await
            .expect("the update must be saved");
    }

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 2);
    let update = entries
        .iter()
        .find(|entry| entry.operation.kind == OperationKind::TrainingSessionUpdate)
        .expect("the merged update");
    assert_eq!(update.operation.operation_id, UPDATE_ID);
    assert_eq!(update.operation.sequence, 2);
    assert_eq!(update.operation.payload["memo"], "c");
    assert_eq!(update.state, OutboxState::Blocked);
    assert_eq!(
        update.operation.parent_operation_id.as_deref(),
        Some(CREATE_ID)
    );

    storage
        .mark_sending(CREATE_ID)
        .await
        .expect("the create must be sending");
    storage
        .mark_success(CREATE_ID)
        .await
        .expect("the create must succeed");
    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert_eq!(entries[0].operation.payload["memo"], "c");
    assert_eq!(
        next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
        Some(UPDATE_ID)
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn the_latest_memo_survives_two_pending_updates_and_a_transient_failure() {
    let name = database_name("latest-memo");
    let storage = open(&name).await;
    create_and_confirm(&storage, "c").await;

    storage
        .save_session_update(&session("a"), &update_operation(UPDATE_ID, "a"))
        .await
        .expect("the first update must be saved");
    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the first update must be sending");
    storage
        .mark_transient_failure(UPDATE_ID)
        .await
        .expect("the first update must fail transiently");
    storage
        .save_session_update(&session("b"), &update_operation(SECOND_UPDATE_ID, "b"))
        .await
        .expect("the second update must be saved");
    storage
        .save_session_update(&session("c"), &update_operation(THIRD_UPDATE_ID, "c"))
        .await
        .expect("the third update must merge into the second");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].operation.operation_id, UPDATE_ID);
    assert_eq!(entries[0].state, OutboxState::Pending);
    assert!(entries[0].attempted);
    assert_eq!(entries[1].operation.operation_id, SECOND_UPDATE_ID);
    assert_eq!(entries[1].operation.payload["memo"], "c");

    storage
        .mark_sending(UPDATE_ID)
        .await
        .expect("the first update must be retried");
    storage
        .mark_success(UPDATE_ID)
        .await
        .expect("the first update must succeed");
    storage
        .mark_sending(SECOND_UPDATE_ID)
        .await
        .expect("the second update must be sending");
    storage
        .mark_success(SECOND_UPDATE_ID)
        .await
        .expect("the second update must succeed");

    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "c");
    assert_eq!(stored.confirmed_memo, "c");
    assert_eq!(stored.confirmed_sequence, 3);
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
async fn persistence_permission_does_not_gate_saving() {
    let name = database_name("persistence");
    let storage = open(&name).await;

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
async fn a_blocked_migration_keeps_the_data_and_adds_the_new_index() {
    let name = database_name("migration");
    let storage = Storage::open_with(&name, 1)
        .await
        .expect("the version 1 database must open");
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
    assert_eq!(
        migrated
            .outbox_entries()
            .await
            .expect("the outbox must be readable")
            .len(),
        1,
        "the outbox must survive the migration"
    );

    let raw = raw_open(&name).await;
    let transaction = raw
        .transaction_with_str("training_sessions")
        .expect("the session store transaction");
    let store = transaction
        .object_store("training_sessions")
        .expect("the session store");
    assert!(store.index_names().contains("by_date"));
    assert!(store.index_names().contains("by_started_at"));
    raw.close();

    migrated
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn a_version_one_session_gets_the_confirmed_memo_from_its_memo() {
    let name = database_name("migrate-confirmed");
    let v1 = Storage::open_with(&name, 1)
        .await
        .expect("the version 1 database must open");
    v1.put_record(
        Store::TrainingSessions,
        &json!({
            "id": TRAINING_ID,
            "date": "2026-11-12",
            "started_at": "2026-11-12T03:00:00Z",
            "memo": "base",
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }),
    )
    .await
    .expect("the version 1 session must be saved");

    let v2 = Storage::open_with(&name, 2)
        .await
        .expect("the version 2 database must open");
    let stored = v2
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.confirmed_memo, "base");

    v2.save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the update must be saved");
    v2.mark_sending(UPDATE_ID)
        .await
        .expect("the update must be sending");
    v2.mark_permanent_failure(UPDATE_ID)
        .await
        .expect("the update must fail");
    let outcome = v2
        .discard(UPDATE_ID)
        .await
        .expect("the update must be discarded");
    let reverted = match outcome {
        DiscardOutcome::MemoReverted(session) => session,
        other => panic!("the update must revert the memo: {other:?}"),
    };
    assert_eq!(reverted.memo, "base");
    assert_eq!(reverted.confirmed_memo, "base");

    v2.clear_account().await.expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn clear_account_removes_the_database_the_outbox_and_advances_the_generation() {
    let name = database_name("clear");
    let storage = open(&name).await;
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

    let reopened = Storage::open_with(&name, 2)
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
    let storage = open(&name).await;

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

#[wasm_bindgen_test]
async fn records_are_limited_to_record_stores() {
    let name = database_name("stores");
    let storage = open(&name).await;

    for store in [Store::Outbox, Store::Meta] {
        let write = storage.put_record(store, &json!({"key": "x"})).await;
        assert_eq!(
            write,
            Err(StorageError::InvalidOperation(
                "the store is not a record store".to_owned()
            ))
        );
        let read = storage.all_records(store).await;
        assert_eq!(
            read,
            Err(StorageError::InvalidOperation(
                "the store is not a record store".to_owned()
            ))
        );
    }

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn refreshing_confirmed_does_not_change_the_draft() {
    let name = database_name("refresh-confirmed");
    let storage = open(&name).await;
    create_and_confirm(&storage, "base").await;
    storage
        .save_session_update(&session("draft"), &update_operation(UPDATE_ID, "draft"))
        .await
        .expect("the draft must be saved");

    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "draft");
    assert_eq!(stored.confirmed_memo, "base");
    assert_eq!(stored.confirmed_sequence, 1);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn forbidden_trainings_are_sorted_and_only_a_takeover_clears_them() {
    let name = database_name("forbidden");
    let storage = open(&name).await;
    assert!(storage
        .forbidden_trainings()
        .await
        .expect("the meta store must be readable")
        .is_empty());

    storage
        .mark_training_forbidden(TRAINING_ID)
        .await
        .expect("the training must be marked");
    storage
        .mark_training_forbidden(OTHER_TRAINING_ID)
        .await
        .expect("the other training must be marked");
    storage
        .mark_training_forbidden(TRAINING_ID)
        .await
        .expect("marking the same training again must not duplicate it");
    assert_eq!(
        storage
            .forbidden_trainings()
            .await
            .expect("the meta store must be readable"),
        vec![OTHER_TRAINING_ID.to_owned(), TRAINING_ID.to_owned()]
    );

    storage
        .replace_session_tree(
            &server_record(TRAINING_ID, "reload"),
            ReplacePurpose::Reload,
        )
        .await
        .expect("the reload must replace the tree");
    assert_eq!(
        storage
            .forbidden_trainings()
            .await
            .expect("the meta store must be readable"),
        vec![OTHER_TRAINING_ID.to_owned(), TRAINING_ID.to_owned()],
        "a reload must keep the editing right loss"
    );

    storage
        .replace_session_tree(
            &server_record(TRAINING_ID, "takeover"),
            ReplacePurpose::Takeover,
        )
        .await
        .expect("the takeover must replace the tree");
    assert_eq!(
        storage
            .forbidden_trainings()
            .await
            .expect("the meta store must be readable"),
        vec![OTHER_TRAINING_ID.to_owned()],
        "a takeover must clear only the taken training"
    );

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn replacing_a_session_tree_removes_the_old_rows_and_outbox_of_the_training() {
    let name = database_name("replace-tree");
    let storage = open(&name).await;
    storage
        .save_session_create(
            &session("old"),
            &create_operation_with_memo(CREATE_ID, "old"),
        )
        .await
        .expect("the create must be saved");
    storage
        .save_session_create(
            &session_with_id(OTHER_TRAINING_ID, "other"),
            &create_operation_for(OTHER_TRAINING_ID, OTHER_CREATE_ID, "other"),
        )
        .await
        .expect("the other create must be saved");
    for (store, record) in [
        (
            Store::TrainingExercises,
            json!({"id": "old-exercise", "training_id": TRAINING_ID, "position": 0}),
        ),
        (
            Store::Sets,
            json!({"id": "old-set", "training_exercise_id": "old-exercise", "position": 0}),
        ),
        (
            Store::SetSegments,
            json!({"id": "old-segment", "set_id": "old-set", "position": 0}),
        ),
        (
            Store::Supersets,
            json!({"id": "old-superset", "training_id": TRAINING_ID, "position": 0}),
        ),
        (
            Store::SupersetMembers,
            json!({
                "superset_id": "old-superset",
                "training_exercise_id": "old-exercise",
                "position": 0
            }),
        ),
        (
            Store::TrainingExercises,
            json!({"id": "other-exercise", "training_id": OTHER_TRAINING_ID, "position": 0}),
        ),
    ] {
        storage
            .put_record(store, &record)
            .await
            .expect("the row must be saved");
    }

    storage
        .replace_session_tree(
            &server_record(TRAINING_ID, "server"),
            ReplacePurpose::Takeover,
        )
        .await
        .expect("the tree must be replaced");

    let stored = storage
        .session(TRAINING_ID)
        .await
        .expect("the session must be readable")
        .expect("the session must exist");
    assert_eq!(stored.memo, "server");
    assert_eq!(stored.confirmed_memo, "server");
    assert_eq!(stored.confirmed_sequence, 0);
    assert_eq!(
        storage
            .session(OTHER_TRAINING_ID)
            .await
            .expect("the other session must be readable")
            .expect("the other session must exist")
            .memo,
        "other",
        "replacing one training must keep the other training"
    );
    assert_eq!(
        record_ids(&storage, Store::TrainingExercises).await,
        vec!["exercise-1".to_owned(), "other-exercise".to_owned()]
    );
    assert_eq!(
        record_ids(&storage, Store::Sets).await,
        vec!["set-1".to_owned()]
    );
    assert_eq!(
        record_ids(&storage, Store::SetSegments).await,
        vec!["segment-1".to_owned()]
    );
    assert_eq!(
        record_ids(&storage, Store::Supersets).await,
        vec!["superset-1".to_owned()]
    );
    let members = storage
        .all_records(Store::SupersetMembers)
        .await
        .expect("the members must be readable");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["superset_id"], "superset-1");
    assert_eq!(members[0]["training_exercise_id"], "exercise-1");

    let entries = storage
        .outbox_entries()
        .await
        .expect("the outbox must be readable");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].operation.training_id, OTHER_TRAINING_ID);
    assert_eq!(entries[0].operation.operation_id, OTHER_CREATE_ID);

    storage
        .clear_account()
        .await
        .expect("the cleanup must work");
}

async fn record_ids(storage: &Storage, store: Store) -> Vec<String> {
    storage
        .all_records(store)
        .await
        .expect("the store must be readable")
        .iter()
        .map(|record| record["id"].as_str().expect("a record id").to_owned())
        .collect()
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
