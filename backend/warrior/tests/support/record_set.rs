use serde_json::{json, Value};
use warrior_core::auth::{hash_session_token, hash_terminal_token};

use super::*;

pub(crate) const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5501";
pub(crate) const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5502";
pub(crate) const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5510";
pub(crate) const FINISHED_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5511";
pub(crate) const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5512";
pub(crate) const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5520";
pub(crate) const OTHER_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5521";
pub(crate) const BENCH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5530";
pub(crate) const BODYWEIGHT_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5531";
pub(crate) const FINISHED_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5532";
pub(crate) const OTHER_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5533";
pub(crate) const BENCH_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5540";
pub(crate) const SECOND_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5541";
pub(crate) const BODY_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5542";
pub(crate) const BODY_NO_WEIGHT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5543";
pub(crate) const FINISHED_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5544";
pub(crate) const OTHER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5545";
pub(crate) const BENCH_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5550";
pub(crate) const EXTRA_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5551";
pub(crate) const BODY_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5552";
pub(crate) const BODY_NO_WEIGHT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5553";
pub(crate) const FINISHED_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5554";
pub(crate) const OTHER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5555";
pub(crate) const BENCH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5560";
pub(crate) const BODYWEIGHT_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5561";
pub(crate) const ORIGIN: &str = "http://localhost:8787";

pub(crate) fn cookie(token: &str, terminal_token: &str) -> String {
    format!("warrior_session={token}; warrior_terminal={terminal_token}")
}

pub(crate) fn seed_base(
    server: &DevServer,
    token: &str,
    terminal_token: &str,
    other_terminal_token: &str,
) {
    let session_hash = hash_session_token(token);
    let terminal_hash = hash_terminal_token(terminal_token);
    let other_terminal_hash = hash_terminal_token(other_terminal_token);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-record-set', 'record-set@example.test', 'セットの利用者', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-record-set-other', 'other@example.test', '別の利用者', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5570', '{USER_ID}', '{session_hash}', \
              '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{TERMINAL_ID}', '{USER_ID}', '{terminal_hash}', '2026-01-01T00:00:00Z', \
              '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{OTHER_TERMINAL_ID}', '{USER_ID}', '{other_terminal_hash}', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO profiles (user_id, weight_kg, created_at, updated_at) \
             VALUES ('{USER_ID}', 70.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        session_sql(TRAINING_ID, USER_ID, "2026-11-12", TERMINAL_ID, None),
        session_sql(
            FINISHED_TRAINING_ID,
            USER_ID,
            "2026-11-11",
            "NULL",
            Some("2026-11-11T04:00:00Z"),
        ),
        session_sql(OTHER_TRAINING_ID, OTHER_USER_ID, "2026-11-12", "NULL", None),
        exercise_sql(
            BENCH_ROW_ID,
            USER_ID,
            TRAINING_ID,
            BENCH_EXERCISE_ID,
            1,
            "weight_and_reps",
        ),
        exercise_sql(
            BODYWEIGHT_ROW_ID,
            USER_ID,
            TRAINING_ID,
            BODYWEIGHT_EXERCISE_ID,
            2,
            "bodyweight_and_reps",
        ),
        exercise_sql(
            FINISHED_ROW_ID,
            USER_ID,
            FINISHED_TRAINING_ID,
            BENCH_EXERCISE_ID,
            1,
            "weight_and_reps",
        ),
        exercise_sql(
            OTHER_ROW_ID,
            OTHER_USER_ID,
            OTHER_TRAINING_ID,
            BENCH_EXERCISE_ID,
            1,
            "weight_and_reps",
        ),
        set_sql(FINISHED_SET_ID, USER_ID, FINISHED_ROW_ID, 1, 1),
        segment_sql(FINISHED_SET_ID, USER_ID, FINISHED_SEGMENT_ID, 60.0, 5),
        set_sql(OTHER_SET_ID, OTHER_USER_ID, OTHER_ROW_ID, 1, 1),
        segment_sql(OTHER_SET_ID, OTHER_USER_ID, OTHER_SEGMENT_ID, 60.0, 5),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the base rows must be seeded");
    }
}

fn session_sql(
    training_id: &str,
    user_id: &str,
    date: &str,
    owner: &str,
    finished_at: Option<&str>,
) -> String {
    let owner = if owner == "NULL" {
        "NULL".to_owned()
    } else {
        format!("'{owner}'")
    };
    match finished_at {
        Some(finished_at) => format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, owner_terminal_id, finished_at, \
              created_at, updated_at) \
             VALUES ('{training_id}', '{user_id}', '{date}', '{date}T03:00:00Z', '', 'empty', \
              {owner}, '{finished_at}', '{date}T03:00:00Z', '{finished_at}')"
        ),
        None => format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, owner_terminal_id, \
              created_at, updated_at) \
             VALUES ('{training_id}', '{user_id}', '{date}', '{date}T03:00:00Z', '', 'empty', \
              {owner}, '{date}T03:00:00Z', '{date}T03:00:00Z')"
        ),
    }
}

fn exercise_sql(
    row_id: &str,
    user_id: &str,
    training_id: &str,
    exercise_id: &str,
    position: i64,
    record_mode: &str,
) -> String {
    let snapshot = json!({
        "exercise_id": exercise_id,
        "name": "種目",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": record_mode,
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    format!(
        "INSERT INTO training_exercises \
         (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
         VALUES ('{row_id}', '{user_id}', '{training_id}', '{exercise_id}', '{}', {position}, \
          '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        snapshot.to_string().replace('\'', "''")
    )
}

fn set_sql(set_id: &str, user_id: &str, row_id: &str, position: i64, completed: i64) -> String {
    format!(
        "INSERT INTO sets \
         (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
         VALUES ('{set_id}', '{user_id}', '{row_id}', {position}, 'normal', {completed}, \
          '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    )
}

fn segment_sql(set_id: &str, user_id: &str, segment_id: &str, weight_kg: f64, reps: i64) -> String {
    format!(
        "INSERT INTO set_segments \
         (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
          created_at, updated_at) \
         VALUES ('{segment_id}', '{user_id}', '{set_id}', 1, {weight_kg}, {reps}, 0, 0, \
          '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    )
}

pub(crate) fn segment(
    segment_id: &str,
    position: i64,
    weight_kg: f64,
    reps: u32,
) -> serde_json::Value {
    json!({
        "segment_id": segment_id,
        "position": position,
        "weight_kg": weight_kg,
        "reps": reps,
        "momentum_reps": 0,
        "assisted_reps": 0,
        "duration_seconds": null,
        "left": null,
        "right": null
    })
}

/// セットの追加の操作に載せるセットの指定。
pub(crate) struct SetSpec<'a> {
    pub(crate) position: i64,
    pub(crate) kind: &'a str,
    pub(crate) body_weight_kg: Option<f64>,
}

pub(crate) fn create_operation(
    operation_id: &str,
    training_id: &str,
    training_exercise_id: &str,
    set_id: &str,
    spec: SetSpec<'_>,
    segments: Vec<serde_json::Value>,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 1,
        "kind": "set_create",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": training_exercise_id,
            "set_id": set_id,
            "position": spec.position,
            "kind": spec.kind,
            "rpe": null,
            "grip": null,
            "body_weight_kg": spec.body_weight_kg,
            "segments": segments
        }
    })
}

pub(crate) fn update_operation(
    operation_id: &str,
    training_id: &str,
    set_id: &str,
    position: i64,
    kind: &str,
    body_weight_kg: Option<f64>,
    segments: Vec<serde_json::Value>,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "set_update",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "set_id": set_id,
            "position": position,
            "kind": kind,
            "rpe": null,
            "grip": null,
            "body_weight_kg": body_weight_kg,
            "segments": segments
        }
    })
}

pub(crate) fn delete_operation(operation_id: &str, training_id: &str, set_id: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 3,
        "kind": "set_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "set_id": set_id
        }
    })
}

pub(crate) fn reorder_operation(
    operation_id: &str,
    training_id: &str,
    training_exercise_id: &str,
    ordered_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 4,
        "kind": "set_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": training_exercise_id,
            "ordered_ids": ordered_ids
        }
    })
}

pub(crate) fn complete_operation(
    operation_id: &str,
    training_id: &str,
    set_id: &str,
    completed: bool,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 5,
        "kind": "set_complete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "set_id": set_id,
            "completed": completed
        }
    })
}

pub(crate) fn post(server: &DevServer, cookie: &str, operation: &Value) -> support::HttpResponse {
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

pub(crate) fn status(response: &support::HttpResponse) -> String {
    serde_json::from_str::<Value>(&response.body).expect("the response is JSON")["status"]
        .as_str()
        .expect("a status")
        .to_owned()
}

pub(crate) fn applied_count(server: &DevServer, operation_id: &str) -> i64 {
    count_with(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations \
             WHERE operation_id = '{operation_id}'"
        ),
    )
}

pub(crate) fn count(server: &DevServer, table: &str, id: &str) -> i64 {
    count_with(
        server,
        &format!("SELECT COUNT(*) AS count FROM {table} WHERE id = '{id}'"),
    )
}

pub(crate) fn count_with(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}

pub(crate) fn first_row(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .expect("a row")
}
