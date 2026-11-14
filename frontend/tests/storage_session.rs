//! セッションと送信待ちの列の保存、更新の統合、失敗した操作の破棄のブラウザのテスト。
//!
//! 対応する要求: `REQ-NFR-003`、`REQ-NFR-005`。
//! テストは並列に走るため、データベースの名前をテストごとに分ける。

use serde_json::json;
use warrior_core::exercise::Laterality;
use warrior_core::set_calc::RecordMode;
use warrior_core::sync::{
    next_sendable, Operation, OperationKind, OutboxState, SessionUpdatePayload,
};
use warrior_core::training_session::{BestKind, PersonalBestItem};
use warrior_frontend::storage::{DiscardOutcome, Store};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{
    create_and_confirm, create_operation, create_operation_with_memo, database_name, open, session,
    TRAINING_ID,
};

const CREATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4302";
const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4303";
const SECOND_UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4304";
const THIRD_UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4305";

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

/// メモをサーバーが受理した状態のトレーニングを作る。
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
    create_and_confirm(&storage, CREATE_ID, "c").await;

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
