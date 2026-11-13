//! 同期の操作の適用の結合テスト。
//!
//! 対応する要求: `REQ-NFR-005`、`REQ-NFR-007`。
//! 同じ操作 ID の再送で D1 が変わらず、操作の適用と操作 ID の保存が同じ batch で行われることを確かめる。

mod support;

use serde_json::{json, Value};
use support::DevServer;
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;

const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4401";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4402";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4403";
const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4404";
const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4405";
const UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4406";

const DATE: &str = "2026-11-12";
const STARTED_AT: &str = "2026-11-12T03:00:00Z";
const SESSION_SELECT: &str =
    "SELECT date, started_at, memo, created_at, updated_at FROM training_sessions WHERE id = '";

#[test]
fn sync_operations_are_idempotent_scoped_to_the_user_and_atomic() {
    let server = DevServer::start(8801);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[9u8; 32]);
    seed(&server, &token);
    let cookie = format!("warrior_session={token}");

    let without_auth = post(&server, "", &create_operation(CREATE_OPERATION_ID, "first"));
    assert_eq!(without_auth.status, 401);

    create_is_applied(&server, &cookie);
    replayed_create_does_not_change_d1(&server, &cookie);
    update_is_applied(&server, &cookie);
    replayed_update_does_not_change_d1(&server, &cookie);
    rejects_foreign_training(&server, &cookie);
    rejects_invalid_operations(&server, &cookie);
    rejects_missing_training(&server, &cookie);
}

fn create_is_applied(server: &DevServer, cookie: &str) {
    let created = post(
        server,
        cookie,
        &create_operation(CREATE_OPERATION_ID, "first"),
    );
    assert_eq!(created.status, 200, "{}", created.body);
    assert_eq!(
        serde_json::from_str::<Value>(&created.body).expect("the response is JSON"),
        json!({ "status": "applied" })
    );
    let row = first_row(server, &format!("{SESSION_SELECT}{TRAINING_ID}'"));
    assert_eq!(row["date"], DATE);
    assert_eq!(row["started_at"], STARTED_AT);
    assert_eq!(row["memo"], "first");
    assert_eq!(applied_count(server, CREATE_OPERATION_ID), 1);
}

fn replayed_create_does_not_change_d1(server: &DevServer, cookie: &str) {
    let before = rows(server, &format!("{SESSION_SELECT}{TRAINING_ID}'"));
    let replayed = post(
        server,
        cookie,
        &create_operation(CREATE_OPERATION_ID, "changed"),
    );
    assert_eq!(replayed.status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&replayed.body).expect("the response is JSON"),
        json!({ "status": "already_applied" })
    );
    let after = rows(server, &format!("{SESSION_SELECT}{TRAINING_ID}'"));
    assert_eq!(before, after, "a replayed operation must not change D1");
}

fn update_is_applied(server: &DevServer, cookie: &str) {
    let updated = post(
        server,
        cookie,
        &update_operation(UPDATE_OPERATION_ID, "second"),
    );
    assert_eq!(updated.status, 200, "{}", updated.body);
    assert_eq!(
        serde_json::from_str::<Value>(&updated.body).expect("the response is JSON"),
        json!({ "status": "applied" })
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT memo FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["memo"],
        "second"
    );
    assert_eq!(applied_count(server, UPDATE_OPERATION_ID), 1);
}

fn replayed_update_does_not_change_d1(server: &DevServer, cookie: &str) {
    let before = rows(server, &format!("{SESSION_SELECT}{TRAINING_ID}'"));
    assert_eq!(
        post(
            server,
            cookie,
            &update_operation(UPDATE_OPERATION_ID, "changed")
        )
        .status,
        200
    );
    let after = rows(server, &format!("{SESSION_SELECT}{TRAINING_ID}'"));
    assert_eq!(before, after, "a replayed update must not change D1");
}

fn rejects_foreign_training(server: &DevServer, cookie: &str) {
    let operation_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4407";
    let mut foreign = update_operation(operation_id, "other");
    foreign["training_id"] = json!(OTHER_TRAINING_ID);
    assert_eq!(post(server, cookie, &foreign).status, 404);
    assert_eq!(applied_count(server, operation_id), 0);

    let mut foreign_create = create_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4412", "foreign");
    foreign_create["training_id"] = json!(OTHER_TRAINING_ID);
    foreign_create["payload"]["id"] = json!(OTHER_TRAINING_ID);
    assert_eq!(post(server, cookie, &foreign_create).status, 404);
}

fn rejects_invalid_operations(server: &DevServer, cookie: &str) {
    let mismatch_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4408";
    let mut mismatched = create_operation(mismatch_id, "date");
    mismatched["payload"]["date"] = json!("2026-11-13");
    assert_eq!(post(server, cookie, &mismatched).status, 400);
    assert_eq!(applied_count(server, mismatch_id), 0);

    let long_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4409";
    let mut too_long = create_operation(long_id, "");
    too_long["payload"]["memo"] = json!("あ".repeat(2001));
    assert_eq!(post(server, cookie, &too_long).status, 400);
    assert_eq!(applied_count(server, long_id), 0);
}

fn rejects_missing_training(server: &DevServer, cookie: &str) {
    let operation_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4410";
    let mut missing = update_operation(operation_id, "missing");
    missing["training_id"] = json!("3f2504e0-4f89-41d3-9a0c-0305e82c4411");
    assert_eq!(post(server, cookie, &missing).status, 404);
    assert_eq!(
        applied_count(server, operation_id),
        0,
        "a missing training must not leave an applied operation"
    );
}

fn post(server: &DevServer, cookie: &str, operation: &Value) -> support::HttpResponse {
    server.request_with_body(
        "POST",
        "/api/sync/operations",
        &[
            ("Origin", ORIGIN),
            ("Content-Type", "application/json"),
            ("Cookie", cookie),
        ],
        &serde_json::to_string(operation).expect("the operation is JSON"),
    )
}

fn create_operation(operation_id: &str, memo: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": TRAINING_ID,
        "sequence": 1,
        "kind": "training_session_create",
        "parent_operation_id": null,
        "payload": {
            "id": TRAINING_ID,
            "date": DATE,
            "started_at": STARTED_AT,
            "memo": memo
        }
    })
}

fn update_operation(operation_id: &str, memo: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": TRAINING_ID,
        "sequence": 2,
        "kind": "training_session_update",
        "parent_operation_id": null,
        "payload": {
            "date": DATE,
            "started_at": STARTED_AT,
            "memo": memo
        }
    })
}

fn seed(server: &DevServer, token: &str) {
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-sync', 'sync@example.test', '同期の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-other', 'other@example.test', '別の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4413', '{USER_ID}', '{}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
            hash_session_token(token)
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{OTHER_TRAINING_ID}', '{OTHER_USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '別の記録', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the user rows must be seeded");
    }
}

fn first_row(server: &DevServer, sql: &str) -> Value {
    rows(server, sql).get(0).cloned().expect("a row")
}

fn rows(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .cloned()
        .expect("a result set")
}

fn count(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}

fn applied_count(server: &DevServer, operation_id: &str) -> i64 {
    count(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations WHERE operation_id = '{operation_id}'"
        ),
    )
}
