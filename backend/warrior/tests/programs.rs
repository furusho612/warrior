//! プログラムの推奨、選択、進行と、独自のプログラムの結合テスト。
//!
//! 対応する要求: `REQ-PRG-002`、`REQ-PRG-005`、`REQ-PRG-019`、`REQ-PRG-021`、`REQ-PRG-022`。

mod support;

use serde_json::{json, Value};
use support::record_common::{cookie, post_operation, seed_auth_session, seed_terminal, seed_user};
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8835;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b01";
const SESSION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b10";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b20";
const ROUTINE_A: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b30";
const ROUTINE_B: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b31";
const ROUTINE_EXERCISE_A: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b40";
const ROUTINE_EXERCISE_B: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b41";
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5b90";
const SESSION_SEED: u8 = 81;
const TERMINAL_SEED: u8 = 82;

#[test]
fn a_program_is_recommended_selected_started_and_advanced() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed(&server);
    let cookie_a = cookie(
        &base64url::encode(&[SESSION_SEED; 32]),
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );

    let programs = get(&server, "/api/programs", &cookie_a);
    assert_eq!(programs.status, 200, "{}", programs.body);
    let programs: Value = serde_json::from_str(&programs.body).expect("the programs are JSON");
    assert_eq!(
        programs["recommended"][0]["id"],
        "program-hypertrophy-intermediate"
    );
    assert_eq!(programs["recommended"][0]["days_per_week"], 4);
    assert_eq!(programs["current"], Value::Null);

    let selected = post(
        &server,
        "/api/programs/program-hypertrophy-intermediate/select",
        &cookie_a,
    );
    assert_eq!(selected.status, 200, "{}", selected.body);
    let current: Value = serde_json::from_str(&selected.body).expect("the current is JSON");
    assert_eq!(current["program"]["id"], "program-hypertrophy-intermediate");
    assert_eq!(current["day_position"], 1);
    assert_eq!(current["total_days"], 4);
    assert!(!current["day"]["exercises"]
        .as_array()
        .expect("the day exercises")
        .is_empty());
    let routine_id = current["routine_id"]
        .as_str()
        .expect("the routine id")
        .to_owned();
    assert_eq!(count(&server, "routines"), 4);

    let started = post(
        &server,
        &format!("/api/routines/{routine_id}/start"),
        &cookie_a,
    );
    assert_eq!(started.status, 200, "{}", started.body);
    let started: Value = serde_json::from_str(&started.body).expect("the record is JSON");
    assert_eq!(started["session"]["source_kind"], "program");
    assert!(started["session"]["program_day_id"].is_string());
    let training_id = started["session"]["id"].as_str().expect("the training id");
    let finished = post(
        &server,
        &format!("/api/trainings/{training_id}/finish"),
        &cookie_a,
    );
    assert_eq!(finished.status, 200, "{}", finished.body);

    let current = get(&server, "/api/programs/current", &cookie_a);
    let current: Value = serde_json::from_str(&current.body).expect("the current is JSON");
    assert_eq!(current["day_position"], 2);

    let stopped = server.request(
        "DELETE",
        "/api/programs/current",
        &[("Origin", ORIGIN), ("Cookie", cookie_a.as_str())],
    );
    assert_eq!(stopped.status, 204);
    let current = get(&server, "/api/programs/current", &cookie_a);
    assert_eq!(
        serde_json::from_str::<Value>(&current.body).expect("the null current"),
        Value::Null
    );
}

#[test]
fn a_custom_program_is_created_updated_selected_and_deleted() {
    let server = DevServer::start(PORT + 1);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed(&server);
    let cookie_a = cookie(
        &base64url::encode(&[SESSION_SEED; 32]),
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );
    seed_routine(&server, ROUTINE_A, ROUTINE_EXERCISE_A, 1);
    seed_routine(&server, ROUTINE_B, ROUTINE_EXERCISE_B, 2);

    let created = post_json(
        &server,
        "/api/custom-programs",
        &cookie_a,
        &json!({ "name": "自分の 2 日", "routine_ids": [ROUTINE_A, ROUTINE_B] }),
    );
    assert_eq!(created.status, 201, "{}", created.body);
    let created: Value = serde_json::from_str(&created.body).expect("the custom program is JSON");
    let custom_id = created["id"].as_str().expect("the custom program id");
    let listed = get(&server, "/api/custom-programs", &cookie_a);
    let listed: Value = serde_json::from_str(&listed.body).expect("the list is JSON");
    assert_eq!(
        listed["programs"].as_array().expect("the programs").len(),
        1
    );

    let updated = server.request_with_body(
        "PUT",
        &format!("/api/custom-programs/{custom_id}"),
        &[("Origin", ORIGIN), ("Cookie", cookie_a.as_str())],
        &json!({ "name": "自分の 2 日 (更新)", "routine_ids": [ROUTINE_B, ROUTINE_A] }).to_string(),
    );
    assert_eq!(updated.status, 200, "{}", updated.body);

    let selected = post(
        &server,
        &format!("/api/custom-programs/{custom_id}/select"),
        &cookie_a,
    );
    assert_eq!(selected.status, 200, "{}", selected.body);
    let current: Value = serde_json::from_str(&selected.body).expect("the current is JSON");
    assert_eq!(current["program"]["id"], custom_id);
    assert_eq!(current["total_days"], 2);

    let first_routine = current["routine_id"]
        .as_str()
        .expect("the first routine")
        .to_owned();
    let started = post(
        &server,
        &format!("/api/routines/{first_routine}/start"),
        &cookie_a,
    );
    assert_eq!(started.status, 200, "{}", started.body);
    let started: Value = serde_json::from_str(&started.body).expect("the record is JSON");
    assert_eq!(started["session"]["source_kind"], "custom_program");
    assert!(started["session"]["custom_program_day_id"].is_string());
    let training_id = started["session"]["id"].as_str().expect("the training id");
    let finished = post(
        &server,
        &format!("/api/trainings/{training_id}/finish"),
        &cookie_a,
    );
    assert_eq!(finished.status, 200, "{}", finished.body);
    let current = get(&server, "/api/programs/current", &cookie_a);
    let current: Value = serde_json::from_str(&current.body).expect("the current is JSON");
    assert_eq!(current["day_position"], 2);

    let second_routine = current["routine_id"]
        .as_str()
        .expect("the second routine")
        .to_owned();
    let started = post(
        &server,
        &format!("/api/routines/{second_routine}/start"),
        &cookie_a,
    );
    assert_eq!(started.status, 200, "{}", started.body);
    let started: Value = serde_json::from_str(&started.body).expect("the record is JSON");
    let training_id = started["session"]["id"].as_str().expect("the training id");
    let finished = post(
        &server,
        &format!("/api/trainings/{training_id}/finish"),
        &cookie_a,
    );
    assert_eq!(finished.status, 200, "{}", finished.body);
    let current = get(&server, "/api/programs/current", &cookie_a);
    let current: Value = serde_json::from_str(&current.body).expect("the current is JSON");
    assert_eq!(current["day_position"], 1);

    let usage = get(
        &server,
        &format!("/api/routines/{ROUTINE_A}/usage"),
        &cookie_a,
    );
    assert_eq!(usage.status, 200, "{}", usage.body);
    let usage: Value = serde_json::from_str(&usage.body).expect("the usage is JSON");
    assert_eq!(usage["programs"], json!(["自分の 2 日 (更新)"]));

    let rejected = post_operation(
        &server,
        &cookie_a,
        &json!({
            "operation_id": DELETE_OPERATION_ID,
            "training_id": ROUTINE_A,
            "sequence": 3,
            "kind": "routine_delete",
            "parent_operation_id": null,
            "payload": {"routine_id": ROUTINE_A}
        }),
        ORIGIN,
    );
    assert_eq!(rejected.status, 409, "{}", rejected.body);
    assert_eq!(count(&server, "routines"), 2);

    let deleted = server.request(
        "DELETE",
        &format!("/api/custom-programs/{custom_id}"),
        &[("Origin", ORIGIN), ("Cookie", cookie_a.as_str())],
    );
    assert_eq!(deleted.status, 204);
    assert_eq!(count(&server, "custom_programs"), 0);
}

fn seed(server: &DevServer) {
    seed_user(
        server,
        USER_ID,
        "program-user",
        "program@example.test",
        "利用者",
    );
    seed_auth_session(
        server,
        SESSION_ID,
        USER_ID,
        &base64url::encode(&[SESSION_SEED; 32]),
    );
    seed_terminal(
        server,
        TERMINAL_ID,
        USER_ID,
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
             training_experience, goal, training_days_per_week, activity_level, created_at, \
             updated_at) VALUES ('{USER_ID}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', \
             'hypertrophy', 4, 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}

fn seed_routine(server: &DevServer, routine_id: &str, exercise_row_id: &str, position: i64) {
    let snapshot = json!({
        "exercise_id": BENCH,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    for sql in [
        format!(
            "INSERT INTO routines (id, user_id, name, position, created_at, updated_at) \
             VALUES ('{routine_id}', '{USER_ID}', 'ルーティン', {position}, \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercises (id, user_id, routine_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{exercise_row_id}', '{USER_ID}', \
             '{routine_id}', '{BENCH}', '{snapshot}', 1, '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercise_targets (routine_exercise_id, user_id, set_count, \
             target_reps, rest_seconds, created_at, updated_at) VALUES ('{exercise_row_id}', \
             '{USER_ID}', 3, 8, 90, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the routine must be seeded");
    }
}

fn get(server: &DevServer, path: &str, cookie: &str) -> support::HttpResponse {
    server.request("GET", path, &[("Origin", ORIGIN), ("Cookie", cookie)])
}

fn post(server: &DevServer, path: &str, cookie: &str) -> support::HttpResponse {
    post_json(server, path, cookie, &json!({}))
}

fn post_json(server: &DevServer, path: &str, cookie: &str, body: &Value) -> support::HttpResponse {
    server.request_with_body(
        "POST",
        path,
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        &body.to_string(),
    )
}

fn count(server: &DevServer, table: &str) -> i64 {
    server
        .execute_d1_json(&format!("SELECT COUNT(*) AS count FROM {table}"))
        .expect("the count must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(Value::as_i64)
        .expect("a count")
}
