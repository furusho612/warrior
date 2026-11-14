//! 編集権と引き継ぎの結合テストが共有する準備。

#![allow(dead_code)]

use serde_json::{json, Value};
use warrior_core::auth::{hash_session_token, hash_terminal_token};
use warrior_core::base64url;

use super::{DevServer, HttpResponse};

pub const ORIGIN: &str = "http://localhost:8787";
pub const PORT: u16 = 8805;
pub const RACE_UPDATE_PORT: u16 = 8807;
pub const RACE_FINISH_PORT: u16 = 8808;
pub const RESEND_PORT: u16 = 8809;
pub const RACE_CREATE_PORT: u16 = 8817;
pub const RACE_CLAIM_PORT: u16 = 8818;
pub const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4601";
pub const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4602";
pub const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4610";
pub const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4611";
pub const MISSING_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4612";
pub const OTHER_RECORDING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4613";
pub const LEGACY_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4614";
pub const TERMINAL_A_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4620";
pub const TERMINAL_B_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4621";
pub const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4630";
pub const SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4631";
pub const SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4632";
pub const SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4633";

pub const DATE: &str = "2026-11-12";
pub const STARTED_AT: &str = "2026-11-12T03:00:00Z";
pub fn seed_a_recording_session_without_an_owner(server: &DevServer, training_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{training_id}', '{USER_ID}', '{DATE}', '{STARTED_AT}', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the recording session must be seeded");
}

pub fn start_server() -> DevServer {
    start_server_at(PORT)
}

pub fn start_server_at(port: u16) -> DevServer {
    let server = DevServer::start(port);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    server
}

pub fn seed(server: &DevServer) -> (String, String, String, String) {
    let token_a = base64url::encode(&[21u8; 32]);
    let token_b = base64url::encode(&[22u8; 32]);
    let session_token = base64url::encode(&[23u8; 32]);
    let other_session_token = base64url::encode(&[24u8; 32]);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-ownership', 'ownership@example.test', '編集権の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-ownership-other', 'ownership-other@example.test', '別の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4622', '{USER_ID}', '{}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
            hash_session_token(&session_token)
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4623', '{OTHER_USER_ID}', '{}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
            hash_session_token(&other_session_token)
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{TERMINAL_A_ID}', '{USER_ID}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            hash_terminal_token(&token_a)
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{TERMINAL_B_ID}', '{USER_ID}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            hash_terminal_token(&token_b)
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{OTHER_TRAINING_ID}', '{OTHER_USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '別の記録', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{OTHER_RECORDING_ID}', '{OTHER_USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '記録中', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the rows must be seeded");
    }
    (session_token, other_session_token, token_a, token_b)
}

pub fn terminal_cookie(session_token: &str, terminal_token: &str) -> String {
    format!("warrior_session={session_token}; warrior_terminal={terminal_token}")
}

pub fn create_as_a(server: &DevServer, cookie: &str) {
    let response = post_sync(
        server,
        cookie,
        &create_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4650", ""),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status_of(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT owner_terminal_id FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["owner_terminal_id"],
        json!(TERMINAL_A_ID)
    );
}

pub fn check_stored_hashes(server: &DevServer, token_a: &str, token_b: &str) {
    let stored = rows(
        server,
        &format!("SELECT token_hash FROM terminal_tokens WHERE user_id = '{USER_ID}' ORDER BY id"),
    );
    assert_eq!(stored.as_array().map(Vec::len), Some(2));
    assert_eq!(stored[0]["token_hash"], json!(hash_terminal_token(token_a)));
    assert_eq!(stored[1]["token_hash"], json!(hash_terminal_token(token_b)));
    for token in [token_a, token_b] {
        assert_ne!(stored[0]["token_hash"], json!(token));
        assert_ne!(stored[1]["token_hash"], json!(token));
    }
}

pub fn seed_sets(server: &DevServer) {
    let snapshot = json!({
        "exercise_id": "3f2504e0-4f89-41d3-9a0c-0305e82c4660",
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    for sql in [
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{EXERCISE_ROW_ID}', '{USER_ID}', '{TRAINING_ID}', '3f2504e0-4f89-41d3-9a0c-0305e82c4660', '{}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
            snapshot.to_string().replace('\'', "''")
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{SET_ID}', '{USER_ID}', '{EXERCISE_ROW_ID}', 1, 'normal', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, created_at, updated_at) \
             VALUES ('{SEGMENT_ID}', '{USER_ID}', '{SET_ID}', 1, 100.0, 5, 0, 0, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO supersets (id, user_id, training_id, position, created_at, updated_at) \
             VALUES ('{SUPERSET_ID}', '{USER_ID}', '{TRAINING_ID}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO superset_members (user_id, superset_id, training_exercise_id, position) \
             VALUES ('{USER_ID}', '{SUPERSET_ID}', '{EXERCISE_ROW_ID}', 1)"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the sets must be seeded");
    }
}

pub fn rejects_the_other_terminal(server: &DevServer, cookie: &str, operation_id: &str) {
    let response = post_sync(server, cookie, &update_operation(operation_id, 2, "from-b"));
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(
        serde_json::from_str::<Value>(&response.body).expect("an error body")["error"]["code"],
        json!("forbidden")
    );
    assert_eq!(applied_count(server, operation_id), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT memo FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["memo"],
        json!(""),
        "a rejected write must not change the memo"
    );
}

/// 記録中の行への作成も、更新と同じ編集権の検査を通る。
pub fn rejects_a_create_from_the_other_terminal(server: &DevServer, cookie: &str) {
    let operation_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4642";
    let response = post_sync(server, cookie, &create_operation(operation_id, "from-b"));
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(applied_count(server, operation_id), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT memo FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["memo"],
        json!("")
    );
}

pub fn check_record(server: &DevServer, cookie: &str) {
    let response = get(server, cookie, &format!("/api/trainings/{TRAINING_ID}"));
    assert_eq!(response.status, 200, "{}", response.body);
    let record: Value = serde_json::from_str(&response.body).expect("the record is JSON");
    let keys = record
        .as_object()
        .expect("the record is an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        vec!["session", "exercises", "sets", "segments", "supersets"]
    );
    assert_eq!(record["session"]["id"], json!(TRAINING_ID));
    assert_eq!(record["session"]["memo"], json!(""));
    assert_eq!(record["exercises"][0]["id"], json!(EXERCISE_ROW_ID));
    let snapshot: Value = serde_json::from_str(
        record["exercises"][0]["snapshot"]
            .as_str()
            .expect("a stored snapshot"),
    )
    .expect("the snapshot is JSON");
    assert_eq!(snapshot["name"], json!("ベンチプレス"));
    assert_eq!(record["sets"][0]["id"], json!(SET_ID));
    assert_eq!(record["sets"][0]["completed"], json!(1));
    assert_eq!(record["segments"][0]["id"], json!(SEGMENT_ID));
    assert_eq!(record["segments"][0]["weight_kg"], json!(100.0));
    assert_eq!(record["supersets"][0]["id"], json!(SUPERSET_ID));
    assert_eq!(
        record["supersets"][0]["members"][0]["training_exercise_id"],
        json!(EXERCISE_ROW_ID)
    );
}

pub fn takeover_as_b(server: &DevServer, cookie: &str) {
    let response = post(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/takeover"),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let record: Value = serde_json::from_str(&response.body).expect("the record is JSON");
    assert_eq!(record["session"]["id"], json!(TRAINING_ID));
    assert_eq!(
        first_row(
            server,
            &format!("SELECT owner_terminal_id FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["owner_terminal_id"],
        json!(TERMINAL_B_ID)
    );
}

pub fn rejects_the_original_terminal(server: &DevServer, cookie: &str, operation_id: &str) {
    let response = post_sync(server, cookie, &update_operation(operation_id, 3, "from-a"));
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(applied_count(server, operation_id), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT memo FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["memo"],
        json!("")
    );
}

pub fn reject_the_finish_of_the_other_terminal(server: &DevServer, cookie: &str) {
    let response = post(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/finish"),
    );
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT finished_at FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["finished_at"],
        Value::Null
    );
}

pub fn finish_as_b(server: &DevServer, cookie: &str) {
    let response = post(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/finish"),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let session = first_row(
        server,
        &format!(
            "SELECT finished_at, owner_terminal_id FROM training_sessions WHERE id = '{TRAINING_ID}'"
        ),
    );
    assert!(
        session["finished_at"].as_str().is_some(),
        "the finish must store the finish time"
    );
    assert_eq!(
        session["owner_terminal_id"],
        Value::Null,
        "the finish must release the editing right"
    );
}

pub fn reject_the_takeover_of_the_finished_training(server: &DevServer, cookie: &str) {
    let response = post(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/takeover"),
    );
    assert_eq!(response.status, 409, "{}", response.body);
}

pub fn check_the_rejected_takeover_did_not_change_d1(server: &DevServer) {
    assert_eq!(
        first_row(
            server,
            &format!("SELECT owner_terminal_id FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["owner_terminal_id"],
        Value::Null
    );
}

pub fn edit_after_the_finish(server: &DevServer, cookie_a: &str, cookie_b: &str) {
    let before = now_iso();
    let a = post_sync(
        server,
        cookie_a,
        &update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4651", 4, "from-a"),
    );
    assert_eq!(a.status, 200, "{}", a.body);
    let b = post_sync(
        server,
        cookie_b,
        &update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4652", 5, "from-b"),
    );
    assert_eq!(b.status, 200, "{}", b.body);
    let stored = first_row(
        server,
        &format!("SELECT memo, updated_at FROM training_sessions WHERE id = '{TRAINING_ID}'"),
    );
    assert_eq!(
        stored["memo"],
        json!("from-b"),
        "the last accepted write must stay"
    );
    let updated_at = stored["updated_at"].as_str().expect("a stored time");
    assert!(
        updated_at >= before.as_str(),
        "the stored time must be the server's time, not the terminal's: {updated_at} < {before}"
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT owner_terminal_id FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["owner_terminal_id"],
        Value::Null,
        "edits after the finish must not restore the owner"
    );

    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            post_sync(
                server,
                cookie_a,
                &update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4653", 6, "race-a"),
            )
        });
        let second = scope.spawn(|| {
            post_sync(
                server,
                cookie_b,
                &update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4654", 7, "race-b"),
            )
        });
        (
            first.join().expect("the first write must finish"),
            second.join().expect("the second write must finish"),
        )
    });
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(second.status, 200, "{}", second.body);
    let raced = first_row(
        server,
        &format!("SELECT memo FROM training_sessions WHERE id = '{TRAINING_ID}'"),
    );
    let raced_memo = raced["memo"].as_str().expect("a stored memo");
    assert!(
        raced_memo == "race-a" || raced_memo == "race-b",
        "one of the concurrent writes must stay: {raced_memo}"
    );

    let final_write = post_sync(
        server,
        cookie_a,
        &update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4655", 8, "final-a"),
    );
    assert_eq!(final_write.status, 200, "{}", final_write.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT memo FROM training_sessions WHERE id = '{TRAINING_ID}'")
        )["memo"],
        json!("final-a")
    );
}

/// 所有者の無い既存の記録中の行は、最初に書き込んだ端末が所有者になる。
pub fn claim_a_recording_session_without_an_owner(
    server: &DevServer,
    cookie_a: &str,
    cookie_b: &str,
) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{LEGACY_TRAINING_ID}', '{USER_ID}', '{DATE}', '{STARTED_AT}', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the legacy session must be seeded");
    let mut operation = update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4657", 2, "legacy");
    operation["training_id"] = json!(LEGACY_TRAINING_ID);
    let claimed = post_sync(server, cookie_a, &operation);
    assert_eq!(claimed.status, 200, "{}", claimed.body);
    let row = first_row(
        server,
        &format!(
            "SELECT memo, owner_terminal_id FROM training_sessions WHERE id = '{LEGACY_TRAINING_ID}'"
        ),
    );
    assert_eq!(row["memo"], json!("legacy"));
    assert_eq!(row["owner_terminal_id"], json!(TERMINAL_A_ID));

    let mut rejected = update_operation("3f2504e0-4f89-41d3-9a0c-0305e82c4658", 3, "other");
    rejected["training_id"] = json!(LEGACY_TRAINING_ID);
    assert_eq!(
        post_sync(server, cookie_b, &rejected).status,
        403,
        "the claiming terminal must keep the editing right"
    );
}

pub fn reject_other_users(server: &DevServer, cookie: &str) {
    let response = get(
        server,
        cookie,
        &format!("/api/trainings/{OTHER_TRAINING_ID}"),
    );
    assert_eq!(response.status, 404, "{}", response.body);
    let takeover = post(
        server,
        cookie,
        &format!("/api/trainings/{OTHER_TRAINING_ID}/takeover"),
    );
    assert_eq!(takeover.status, 404, "{}", takeover.body);
}

pub fn reject_a_missing_training(server: &DevServer, cookie: &str) {
    let response = get(
        server,
        cookie,
        &format!("/api/trainings/{MISSING_TRAINING_ID}"),
    );
    assert_eq!(response.status, 404, "{}", response.body);
}

/// 別のアカウントに切り替えた端末の古いトークンは、新しいアカウントの書き込みに使えない。
pub fn reject_a_reused_terminal_of_a_switched_account(
    server: &DevServer,
    other_session_token: &str,
    terminal_a: &str,
) {
    let operation_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4656";
    let mut operation = update_operation(operation_id, 2, "switched");
    operation["training_id"] = json!(OTHER_RECORDING_ID);
    let cookie = terminal_cookie(other_session_token, terminal_a);
    let response = post_sync(server, &cookie, &operation);
    assert_eq!(
        response.status, 403,
        "a terminal of the previous user must not edit: {}",
        response.body
    );
    assert_eq!(applied_count(server, operation_id), 0);
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT memo, owner_terminal_id FROM training_sessions WHERE id = '{OTHER_RECORDING_ID}'"
            )
        )["memo"],
        json!("記録中")
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT owner_terminal_id FROM training_sessions WHERE id = '{OTHER_RECORDING_ID}'"
            )
        )["owner_terminal_id"],
        Value::Null
    );
}

pub fn post_sync(server: &DevServer, cookie: &str, operation: &Value) -> HttpResponse {
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

pub fn post(server: &DevServer, cookie: &str, path: &str) -> HttpResponse {
    server.request_with_body("POST", path, &[("Origin", ORIGIN), ("Cookie", cookie)], "")
}

pub fn get(server: &DevServer, cookie: &str, path: &str) -> HttpResponse {
    server.request("GET", path, &[("Origin", ORIGIN), ("Cookie", cookie)])
}

pub fn create_operation(operation_id: &str, memo: &str) -> Value {
    create_operation_for(TRAINING_ID, operation_id, memo)
}

pub fn create_operation_for(training_id: &str, operation_id: &str, memo: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 1,
        "kind": "training_session_create",
        "parent_operation_id": null,
        "payload": {
            "id": training_id,
            "date": DATE,
            "started_at": STARTED_AT,
            "memo": memo,
            "source_kind": "empty",
            "routine_id": null,
            "program_day_id": null,
            "custom_program_day_id": null
        }
    })
}

pub fn update_operation(operation_id: &str, sequence: u64, memo: &str) -> Value {
    update_operation_for(TRAINING_ID, operation_id, sequence, memo)
}

pub fn update_operation_for(
    training_id: &str,
    operation_id: &str,
    sequence: u64,
    memo: &str,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": sequence,
        "kind": "training_session_update",
        "parent_operation_id": null,
        "payload": {
            "date": DATE,
            "started_at": STARTED_AT,
            "memo": memo
        }
    })
}

pub fn exercise_uuid(prefix: &str, number: u32) -> String {
    format!("3f2504e0-4f89-41d3-9a0c-{prefix}{number:08x}")
}

pub fn exercise_snapshot() -> Value {
    json!({
        "exercise_id": "3f2504e0-4f89-41d3-9a0c-0305e82c4e01",
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
}

pub fn seed_exercise_row(server: &DevServer, training_id: &str, row_id: &str, position: i64) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{row_id}', '{USER_ID}', '{training_id}', \
              '3f2504e0-4f89-41d3-9a0c-0305e82c4e01', '{}', {position}, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            exercise_snapshot().to_string().replace('\'', "''")
        ))
        .expect("the exercise row must be seeded");
}

pub fn seed_exercise_set(server: &DevServer, row_id: &str, set_id: &str, segment_id: &str) {
    for sql in [
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{set_id}', '{USER_ID}', '{row_id}', 1, 'normal', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, created_at, updated_at) \
             VALUES ('{segment_id}', '{USER_ID}', '{set_id}', 1, 100.0, 5, 0, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the exercise set must be seeded");
    }
}

pub fn exercise_create_operation(
    training_id: &str,
    operation_id: &str,
    row_id: &str,
    position: i64,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "training_exercise_create",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": row_id,
            "exercise_id": "3f2504e0-4f89-41d3-9a0c-0305e82c4e01",
            "snapshot": exercise_snapshot(),
            "position": position
        }
    })
}

pub fn exercise_delete_operation(training_id: &str, operation_id: &str, row_id: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "training_exercise_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": row_id
        }
    })
}

pub fn exercise_reorder_operation(
    training_id: &str,
    operation_id: &str,
    ordered_ids: &[String],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "training_exercise_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "ordered_ids": ordered_ids
        }
    })
}

pub fn row_count(server: &DevServer, table: &str, id: &str) -> i64 {
    count(
        server,
        &format!("SELECT COUNT(*) AS count FROM {table} WHERE id = '{id}'"),
    )
}

pub fn position(server: &DevServer, row_id: &str) -> i64 {
    first_row(
        server,
        &format!("SELECT position FROM training_exercises WHERE id = '{row_id}'"),
    )["position"]
        .as_i64()
        .expect("a position")
}

pub fn status_of(response: &HttpResponse) -> String {
    serde_json::from_str::<Value>(&response.body).expect("a status body")["status"]
        .as_str()
        .expect("a status")
        .to_owned()
}

pub fn now_iso() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_secs() as i64;
    warrior_core::datetime::format_utc(seconds)
}

pub fn applied_count(server: &DevServer, operation_id: &str) -> i64 {
    count(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations WHERE operation_id = '{operation_id}'"
        ),
    )
}

pub fn applied_training_count(server: &DevServer, training_id: &str) -> i64 {
    count(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations WHERE training_id = '{training_id}'"
        ),
    )
}

pub fn d1_memo(server: &DevServer, training_id: &str) -> String {
    first_row(
        server,
        &format!("SELECT memo FROM training_sessions WHERE id = '{training_id}'"),
    )["memo"]
        .as_str()
        .expect("a stored memo")
        .to_owned()
}

pub fn first_row(server: &DevServer, sql: &str) -> Value {
    rows(server, sql).get(0).cloned().expect("a row")
}

pub fn rows(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .cloned()
        .expect("a result set")
}

pub fn count(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}
