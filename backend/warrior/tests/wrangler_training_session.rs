//! トレーニングの開始、セットの保存、終了、サマリーの取得の結合テスト。
//!
//! 対応する要求: `REQ-REC-005`。
//! 開始は同期の操作の適用で行い、種目とセットと段は D1 に置く。
//! 終了の値が `set_calc` の計算と一致し、2 回目の終了が同じ値を返し、
//! 別の利用者のトレーニングが 404 になることを確かめる。

mod support;

use serde_json::{json, Value};
use support::DevServer;
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::datetime;
use warrior_core::set_calc::estimated_one_rep_max;
use warrior_core::training_session::TrainingSessionSummary;

const ORIGIN: &str = "http://localhost:8787";
const PORT: u16 = 8804;
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4501";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4502";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4510";
const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4511";
const NO_START_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4512";
const MISSING_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4513";
const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4560";
const BENCH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4530";
const PLANK_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4531";
const BENCH_TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4520";
const PLANK_TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4521";
const WARMUP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4540";
const NORMAL_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4541";
const DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4542";
const TIME_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4543";

#[test]
fn finishing_a_training_saves_the_calculated_summary_for_its_owner() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[11u8; 32]);
    seed_users_and_sessions(&server, &token);
    let cookie = format!("warrior_session={token}");
    let started_seconds = now_seconds() - 3_600;
    let started_at = datetime::format_utc(started_seconds);
    let date = datetime::japan_date_from_utc(&started_at).expect("a Japan date");

    start_training(&server, &cookie, &date, &started_at);
    check_created_session(&server, &date, &started_at);
    seed_sets(&server);
    let summary = finish_training(&server, &cookie, started_seconds);
    check_stored_summary(&server, &summary);
    check_summary_endpoint(&server, &cookie, &summary);
    check_replayed_finish(&server, &cookie, &summary);
    reject_foreign_and_missing_trainings(&server, &cookie);
    reject_a_training_without_a_start_time(&server, &cookie);
}

fn check_created_session(server: &DevServer, date: &str, started_at: &str) {
    let stored = first_row(
        server,
        &format!(
            "SELECT date, started_at, source_kind, routine_id, program_day_id, custom_program_day_id \
             FROM training_sessions WHERE id = '{TRAINING_ID}'"
        ),
    );
    assert_eq!(stored["date"], date, "the date must be the Japan date");
    assert_eq!(stored["started_at"], started_at);
    assert_eq!(stored["source_kind"], "empty");
    assert_eq!(stored["routine_id"], Value::Null);
    assert_eq!(stored["program_day_id"], Value::Null);
    assert_eq!(stored["custom_program_day_id"], Value::Null);
}

fn finish_training(
    server: &DevServer,
    cookie: &str,
    started_seconds: i64,
) -> TrainingSessionSummary {
    let before = now_seconds();
    let finished = post(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/finish"),
    );
    let after = now_seconds();
    assert_eq!(finished.status, 200, "{}", finished.body);
    let summary: TrainingSessionSummary =
        serde_json::from_str(&finished.body).expect("the finish response is a summary");
    assert!(
        summary.duration_seconds >= before - started_seconds,
        "the duration must start at the stored start time"
    );
    assert!(
        summary.duration_seconds <= after - started_seconds,
        "the duration must end at the server's finish time"
    );
    assert!((summary.total_weight_kg - 1_640.0).abs() < 1e-9);
    assert_eq!(summary.exercise_count, 2);
    assert_eq!(summary.set_count, 3);
    assert_eq!(
        summary.personal_bests,
        vec![
            best(BENCH_EXERCISE_ID, "max_weight", 100.0),
            best(
                BENCH_EXERCISE_ID,
                "estimated_one_rep_max",
                estimated_one_rep_max(100.0, 5).expect("a value"),
            ),
            best(BENCH_EXERCISE_ID, "total_weight", 1_140.0),
            best(PLANK_EXERCISE_ID, "longest_duration", 60.0),
        ]
    );
    summary
}

fn check_stored_summary(server: &DevServer, summary: &TrainingSessionSummary) {
    let stored = summary_row(server, TRAINING_ID);
    assert_eq!(stored["duration_seconds"], summary.duration_seconds);
    assert_eq!(stored["total_weight_kg"], 1_640.0);
    assert_eq!(stored["exercise_count"], 2);
    assert_eq!(stored["set_count"], 3);
    assert_eq!(
        serde_json::from_str::<Value>(stored["personal_bests"].as_str().expect("stored JSON"))
            .expect("the personal bests are JSON"),
        serde_json::to_value(&summary.personal_bests).expect("the personal bests are JSON")
    );
}

fn check_summary_endpoint(server: &DevServer, cookie: &str, summary: &TrainingSessionSummary) {
    let response = get(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/summary"),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        serde_json::from_str::<TrainingSessionSummary>(&response.body)
            .expect("the summary response is a summary"),
        *summary
    );
}

fn check_replayed_finish(server: &DevServer, cookie: &str, summary: &TrainingSessionSummary) {
    let before = summary_row(server, TRAINING_ID);
    let replayed = post(
        server,
        cookie,
        &format!("/api/trainings/{TRAINING_ID}/finish"),
    );
    assert_eq!(replayed.status, 200, "{}", replayed.body);
    assert_eq!(
        serde_json::from_str::<TrainingSessionSummary>(&replayed.body)
            .expect("the replay response is a summary"),
        *summary
    );
    assert_eq!(
        before,
        summary_row(server, TRAINING_ID),
        "a replayed finish must not change D1"
    );
}

fn start_training(server: &DevServer, cookie: &str, date: &str, started_at: &str) {
    let operation = json!({
        "operation_id": CREATE_OPERATION_ID,
        "training_id": TRAINING_ID,
        "sequence": 1,
        "kind": "training_session_create",
        "parent_operation_id": null,
        "payload": {
            "id": TRAINING_ID,
            "date": date,
            "started_at": started_at,
            "memo": "",
            "source_kind": "empty",
            "routine_id": null,
            "program_day_id": null,
            "custom_program_day_id": null
        }
    });
    let created = server.request_with_body(
        "POST",
        "/api/sync/operations",
        &[
            ("Origin", ORIGIN),
            ("Content-Type", "application/json"),
            ("Cookie", cookie),
        ],
        &operation.to_string(),
    );
    assert_eq!(created.status, 200, "{}", created.body);
    assert_eq!(
        serde_json::from_str::<Value>(&created.body).expect("the response is JSON"),
        json!({ "status": "applied" })
    );
}

fn reject_foreign_and_missing_trainings(server: &DevServer, cookie: &str) {
    for training_id in [OTHER_TRAINING_ID, MISSING_TRAINING_ID] {
        let finished = post(
            server,
            cookie,
            &format!("/api/trainings/{training_id}/finish"),
        );
        assert_eq!(finished.status, 404, "{}", finished.body);
        let summary = get(
            server,
            cookie,
            &format!("/api/trainings/{training_id}/summary"),
        );
        assert_eq!(summary.status, 404, "{}", summary.body);
    }
    assert_eq!(
        summary_row(server, OTHER_TRAINING_ID)["finished_at"],
        Value::Null,
        "a foreign finish must not change the row"
    );
}

fn reject_a_training_without_a_start_time(server: &DevServer, cookie: &str) {
    let finished = post(
        server,
        cookie,
        &format!("/api/trainings/{NO_START_TRAINING_ID}/finish"),
    );
    assert_eq!(finished.status, 400, "{}", finished.body);
    let summary = get(
        server,
        cookie,
        &format!("/api/trainings/{NO_START_TRAINING_ID}/summary"),
    );
    assert_eq!(summary.status, 404, "{}", summary.body);
}

fn post(server: &DevServer, cookie: &str, path: &str) -> support::HttpResponse {
    server.request_with_body("POST", path, &[("Origin", ORIGIN), ("Cookie", cookie)], "")
}

fn get(server: &DevServer, cookie: &str, path: &str) -> support::HttpResponse {
    server.request("GET", path, &[("Origin", ORIGIN), ("Cookie", cookie)])
}

fn now_seconds() -> i64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_millis()
        / 1_000) as i64
}

fn seed_users_and_sessions(server: &DevServer, token: &str) {
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-training', 'training@example.test', '記録の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-other-training', 'other-training@example.test', '別の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4561', '{USER_ID}', '{}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')",
            hash_session_token(token)
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{OTHER_TRAINING_ID}', '{OTHER_USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '別の記録', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, memo, created_at, updated_at) \
             VALUES ('{NO_START_TRAINING_ID}', '{USER_ID}', '2026-11-12', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the rows must be seeded");
    }
}

fn seed_sets(server: &DevServer) {
    let bench_snapshot = json!({
        "exercise_id": BENCH_EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    let plank_snapshot = json!({
        "exercise_id": PLANK_EXERCISE_ID,
        "name": "プランク",
        "body_part": "abs",
        "muscles": [],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    for sql in [
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{BENCH_TRAINING_EXERCISE_ID}', '{USER_ID}', '{TRAINING_ID}', '{BENCH_EXERCISE_ID}', '{}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
            json_literal(&bench_snapshot)
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{PLANK_TRAINING_EXERCISE_ID}', '{USER_ID}', '{TRAINING_ID}', '{PLANK_EXERCISE_ID}', '{}', 2, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
            json_literal(&plank_snapshot)
        ),
        set_sql(BENCH_TRAINING_EXERCISE_ID, WARMUP_SET_ID, 1, "warmup"),
        set_sql(BENCH_TRAINING_EXERCISE_ID, NORMAL_SET_ID, 2, "normal"),
        set_sql(BENCH_TRAINING_EXERCISE_ID, DROP_SET_ID, 3, "drop"),
        set_sql(PLANK_TRAINING_EXERCISE_ID, TIME_SET_ID, 1, "normal"),
        segment_sql(WARMUP_SET_ID, "3f2504e0-4f89-41d3-9a0c-0305e82c4571", 1, Some(40.0), Some(10), None),
        segment_sql(NORMAL_SET_ID, "3f2504e0-4f89-41d3-9a0c-0305e82c4572", 1, Some(100.0), Some(5), None),
        segment_sql(DROP_SET_ID, "3f2504e0-4f89-41d3-9a0c-0305e82c4573", 1, Some(90.0), Some(8), None),
        segment_sql(DROP_SET_ID, "3f2504e0-4f89-41d3-9a0c-0305e82c4574", 2, Some(70.0), Some(6), None),
        segment_sql(TIME_SET_ID, "3f2504e0-4f89-41d3-9a0c-0305e82c4575", 1, None, None, Some(60)),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the sets must be seeded");
    }
    server
        .execute_d1_json(&format!(
            "UPDATE sets SET rpe = 8.0 WHERE id = '{NORMAL_SET_ID}'"
        ))
        .expect("the RPE must be stored");
}

fn set_sql(training_exercise_id: &str, set_id: &str, position: i64, kind: &str) -> String {
    format!(
        "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
         VALUES ('{set_id}', '{USER_ID}', '{training_exercise_id}', {position}, '{kind}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
    )
}

fn segment_sql(
    set_id: &str,
    segment_id: &str,
    position: i64,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> String {
    let weight = weight_kg.map_or("NULL".to_owned(), |value| value.to_string());
    let reps = reps.map_or("NULL".to_owned(), |value| value.to_string());
    let duration = duration_seconds.map_or("NULL".to_owned(), |value| value.to_string());
    format!(
        "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, duration_seconds, created_at, updated_at) \
         VALUES ('{segment_id}', '{USER_ID}', '{set_id}', {position}, {weight}, {reps}, 0, 0, {duration}, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
    )
}

fn json_literal(value: &Value) -> String {
    value.to_string().replace('\'', "''")
}

fn best(
    exercise_id: &str,
    kind: &str,
    value: f64,
) -> warrior_core::training_session::PersonalBestItem {
    warrior_core::training_session::PersonalBestItem {
        exercise_id: exercise_id.to_owned(),
        kind: kind.to_owned(),
        value,
    }
}

fn summary_row(server: &DevServer, training_id: &str) -> Value {
    first_row(
        server,
        &format!(
            "SELECT finished_at, duration_seconds, total_weight_kg, exercise_count, set_count, personal_bests \
             FROM training_sessions WHERE id = '{training_id}'"
        ),
    )
}

fn first_row(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .expect("a row")
}
