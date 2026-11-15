//! 記録の種目の追加、並べ替え、削除と、前回の取得の結合テスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-REC-003`、`REQ-REC-004`。
//! 種目の操作は同期の操作の適用で行い、D1 の行を確かめる。削除はセットと段も消す。
//! 前回の取得は、種目、グリップとアタッチメント、ルーティン、プログラム、空の状態の
//! 規則に従うことを確かめる。

#[path = "support/record_exercise.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::record_common;
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8816;
const PREVIOUS_PORT: u16 = 8817;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4901";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4902";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4910";
const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4911";
const FINISHED_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4912";
const OTHER_TRAINING_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b01";
const OTHER_TRAINING_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b02";
const OTHER_TRAINING_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b03";
const DELETE_OTHER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b04";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4920";
const OTHER_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4921";
const BENCH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4930";
const PLANK_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4931";
const BENCH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4940";
const PLANK_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4941";
const DELETED_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4942";
const BENCH_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4950";
const BENCH_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4951";
const PLANK_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4952";
const PLANK_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4953";
const SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4960";
const CREATE_BENCH_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4970";
const CREATE_PLANK_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4971";
const REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4973";
const SAME_ORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4974";
const SUBSET_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4979";
const STALE_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a90";
const DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4975";
const FINISHED_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4976";
const GRIP_JSON: &str = r#"{"grip":{"style":"pronated","width":"medium"}}"#;

#[test]
fn exercise_operations_are_idempotent_scoped_and_delete_the_sets() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[21u8; 32]);
    let terminal_token = base64url::encode(&[22u8; 32]);
    let other_terminal_token = base64url::encode(&[23u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let user_cookie = cookie(&token, &terminal_token);
    let other_cookie = cookie(&token, &other_terminal_token);
    let session_only_cookie = format!("warrior_session={token}");

    create_the_exercises(&server, &user_cookie);
    reorder_the_exercises(&server, &user_cookie);
    reject_writes_without_the_editing_right(
        &server,
        &user_cookie,
        &other_cookie,
        &session_only_cookie,
    );
    seed_sets(&server);
    delete_the_exercise_with_its_sets(&server, &user_cookie);
    edit_a_finished_training_without_a_terminal(&server, &session_only_cookie);
}

fn create_the_exercises(server: &DevServer, cookie: &str) {
    let bench = post(
        server,
        cookie,
        &create_operation(
            CREATE_BENCH_OPERATION_ID,
            BENCH_ROW_ID,
            ExerciseSpec {
                exercise_id: BENCH_EXERCISE_ID,
                name: "ベンチプレス",
                body_part: "chest",
                muscle: "pectoralis_major_middle",
                record_mode: "weight_and_reps",
            },
            1,
        ),
    );
    assert_eq!(bench.status, 200, "{}", bench.body);
    assert_eq!(status(&bench), "applied");
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT exercise_id, position FROM training_exercises WHERE id = '{BENCH_ROW_ID}'"
            )
        ),
        json!({"exercise_id": BENCH_EXERCISE_ID, "position": 1})
    );
    assert_eq!(applied_count(server, CREATE_BENCH_OPERATION_ID), 1);

    let replay = post(
        server,
        cookie,
        &create_operation(
            CREATE_BENCH_OPERATION_ID,
            BENCH_ROW_ID,
            ExerciseSpec {
                exercise_id: BENCH_EXERCISE_ID,
                name: "別の名前",
                body_part: "chest",
                muscle: "pectoralis_major_middle",
                record_mode: "weight_and_reps",
            },
            9,
        ),
    );
    assert_eq!(replay.status, 200);
    assert_eq!(status(&replay), "already_applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT position FROM training_exercises WHERE id = '{BENCH_ROW_ID}'")
        )["position"],
        1,
        "a replay must not change the stored position"
    );

    let plank = post(
        server,
        cookie,
        &create_operation(
            CREATE_PLANK_OPERATION_ID,
            PLANK_ROW_ID,
            ExerciseSpec {
                exercise_id: PLANK_EXERCISE_ID,
                name: "プランク",
                body_part: "abs",
                muscle: "rectus_abdominis_upper",
                record_mode: "time",
            },
            2,
        ),
    );
    assert_eq!(plank.status, 200, "{}", plank.body);
    assert_eq!(status(&plank), "applied");
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT exercise_id, position FROM training_exercises WHERE id = '{PLANK_ROW_ID}'"
            )
        ),
        json!({"exercise_id": PLANK_EXERCISE_ID, "position": 2})
    );
}

fn reorder_the_exercises(server: &DevServer, cookie: &str) {
    let reorder = post(
        server,
        cookie,
        &reorder_operation(REORDER_OPERATION_ID, &[PLANK_ROW_ID, BENCH_ROW_ID]),
    );
    assert_eq!(reorder.status, 200, "{}", reorder.body);
    assert_eq!(status(&reorder), "applied");
    assert_eq!(position(server, PLANK_ROW_ID), 1);
    assert_eq!(position(server, BENCH_ROW_ID), 2);
    assert_eq!(applied_count(server, REORDER_OPERATION_ID), 1);

    let same_order = post(
        server,
        cookie,
        &reorder_operation(SAME_ORDER_OPERATION_ID, &[PLANK_ROW_ID, BENCH_ROW_ID]),
    );
    assert_eq!(same_order.status, 200, "{}", same_order.body);
    assert_eq!(status(&same_order), "applied");
    assert_eq!(
        applied_count(server, SAME_ORDER_OPERATION_ID),
        1,
        "a reorder with the same order must record the operation"
    );
    assert_eq!(position(server, PLANK_ROW_ID), 1);
    assert_eq!(position(server, BENCH_ROW_ID), 2);

    let subset = post(
        server,
        cookie,
        &reorder_operation(SUBSET_REORDER_OPERATION_ID, &[PLANK_ROW_ID]),
    );
    assert_eq!(subset.status, 409, "{}", subset.body);
    assert_eq!(applied_count(server, SUBSET_REORDER_OPERATION_ID), 0);
    assert_eq!(position(server, PLANK_ROW_ID), 1);
    assert_eq!(position(server, BENCH_ROW_ID), 2);

    // 現在の種目に無い行 ID を含む古い本文は、ID の集合が一致しないため 409 になる。
    let stale = post(
        server,
        cookie,
        &reorder_operation(STALE_REORDER_OPERATION_ID, &[BENCH_ROW_ID, DELETED_ROW_ID]),
    );
    assert_eq!(stale.status, 409, "{}", stale.body);
    assert_eq!(applied_count(server, STALE_REORDER_OPERATION_ID), 0);
    assert_eq!(position(server, PLANK_ROW_ID), 1);
    assert_eq!(position(server, BENCH_ROW_ID), 2);
}

fn reject_writes_without_the_editing_right(
    server: &DevServer,
    _cookie: &str,
    other_cookie: &str,
    session_only_cookie: &str,
) {
    let operation = create_operation(
        "3f2504e0-4f89-41d3-9a0c-0305e82c4977",
        "3f2504e0-4f89-41d3-9a0c-0305e82c4943",
        ExerciseSpec {
            exercise_id: BENCH_EXERCISE_ID,
            name: "ベンチプレス",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "weight_and_reps",
        },
        3,
    );
    let other_terminal = post(server, other_cookie, &operation);
    assert_eq!(other_terminal.status, 403, "{}", other_terminal.body);
    let no_terminal = post(server, session_only_cookie, &operation);
    assert_eq!(no_terminal.status, 403, "{}", no_terminal.body);

    let mut foreign = operation.clone();
    foreign["training_id"] = json!(OTHER_TRAINING_ID);
    foreign["payload"]["training_id"] = json!(OTHER_TRAINING_ID);
    foreign["operation_id"] = json!("3f2504e0-4f89-41d3-9a0c-0305e82c4978");
    let foreign_response = post_with_training(server, other_cookie, &foreign, OTHER_TRAINING_ID);
    assert_eq!(foreign_response.status, 404, "{}", foreign_response.body);
}

fn seed_sets(server: &DevServer) {
    record_common::seed_set(
        server,
        record_common::SetSeed {
            set_id: BENCH_SET_ID,
            user_id: USER_ID,
            training_exercise_id: BENCH_ROW_ID,
            position: 1,
            kind: "normal",
            completed: 1,
            grip: None,
        },
    );
    record_common::seed_segment(
        server,
        record_common::SegmentSeed {
            segment_id: BENCH_SEGMENT_ID,
            user_id: USER_ID,
            set_id: BENCH_SET_ID,
            position: 1,
            weight_kg: Some(100.0),
            reps: Some(5),
            duration_seconds: None,
        },
    );
    record_common::seed_set(
        server,
        record_common::SetSeed {
            set_id: PLANK_SET_ID,
            user_id: USER_ID,
            training_exercise_id: PLANK_ROW_ID,
            position: 1,
            kind: "normal",
            completed: 1,
            grip: None,
        },
    );
    record_common::seed_segment(
        server,
        record_common::SegmentSeed {
            segment_id: PLANK_SEGMENT_ID,
            user_id: USER_ID,
            set_id: PLANK_SET_ID,
            position: 1,
            weight_kg: None,
            reps: None,
            duration_seconds: Some(90),
        },
    );
    for sql in [
        format!(
            "INSERT INTO supersets (id, user_id, training_id, position, created_at, updated_at) \
             VALUES ('{SUPERSET_ID}', '{USER_ID}', '{TRAINING_ID}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO superset_members (user_id, superset_id, training_exercise_id, position) \
             VALUES ('{USER_ID}', '{SUPERSET_ID}', '{BENCH_ROW_ID}', 1)"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the superset must be seeded");
    }
}

fn delete_the_exercise_with_its_sets(server: &DevServer, cookie: &str) {
    let delete = delete_operation(DELETE_OPERATION_ID, BENCH_ROW_ID);
    let rejected = post(server, cookie, &delete);
    assert_eq!(rejected.status, 409, "{}", rejected.body);
    assert_eq!(applied_count(server, DELETE_OPERATION_ID), 0);
    assert_eq!(count(server, "training_exercises", BENCH_ROW_ID), 1);
    assert_eq!(count(server, "sets", BENCH_SET_ID), 1);

    server
        .execute_d1_json(&format!(
            "UPDATE sets SET completed = 0 WHERE training_exercise_id = '{BENCH_ROW_ID}'"
        ))
        .expect("the completed sets must be reopened for the delete");

    let delete = post(
        server,
        cookie,
        &delete_operation(DELETE_OPERATION_ID, BENCH_ROW_ID),
    );
    assert_eq!(delete.status, 200, "{}", delete.body);
    assert_eq!(status(&delete), "applied");
    assert_eq!(count(server, "training_exercises", BENCH_ROW_ID), 0);
    assert_eq!(count(server, "sets", BENCH_SET_ID), 0);
    assert_eq!(count(server, "set_segments", BENCH_SEGMENT_ID), 0);
    assert_eq!(
        count_with(
            server,
            &format!(
                "SELECT COUNT(*) AS count FROM superset_members WHERE training_exercise_id = '{BENCH_ROW_ID}'"
            )
        ),
        0
    );
    assert_eq!(
        count(server, "sets", PLANK_SET_ID),
        1,
        "the delete must keep the other exercise's sets"
    );
    assert_eq!(applied_count(server, DELETE_OPERATION_ID), 1);

    let replay = post(
        server,
        cookie,
        &delete_operation(DELETE_OPERATION_ID, BENCH_ROW_ID),
    );
    assert_eq!(replay.status, 200);
    assert_eq!(status(&replay), "already_applied");
}

fn edit_a_finished_training_without_a_terminal(server: &DevServer, session_only_cookie: &str) {
    let operation = create_operation(
        FINISHED_CREATE_OPERATION_ID,
        DELETED_ROW_ID,
        ExerciseSpec {
            exercise_id: BENCH_EXERCISE_ID,
            name: "ベンチプレス",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "weight_and_reps",
        },
        1,
    );
    let response = post_with_training(
        server,
        session_only_cookie,
        &operation,
        FINISHED_TRAINING_ID,
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(count(server, "training_exercises", DELETED_ROW_ID), 1);
}

#[test]
fn a_delete_for_another_trainings_exercise_is_rejected_and_keeps_the_set() {
    let server = DevServer::start(8820);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[41u8; 32]);
    let terminal_token = base64url::encode(&[42u8; 32]);
    let other_terminal_token = base64url::encode(&[43u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let user_cookie = cookie(&token, &terminal_token);
    seed_other_training_exercise(&server);

    let response = post(
        &server,
        &user_cookie,
        &delete_operation(DELETE_OTHER_OPERATION_ID, OTHER_TRAINING_ROW_ID),
    );
    assert_eq!(response.status, 404, "{}", response.body);
    assert_eq!(
        count(&server, "training_exercises", OTHER_TRAINING_ROW_ID),
        1
    );
    assert_eq!(count(&server, "sets", OTHER_TRAINING_SET_ID), 1);
    assert_eq!(count(&server, "set_segments", OTHER_TRAINING_SEGMENT_ID), 1);
    assert_eq!(applied_count(&server, DELETE_OTHER_OPERATION_ID), 0);
}

#[test]
fn the_previous_endpoint_follows_the_source_and_grip_rules() {
    let server = DevServer::start(PREVIOUS_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[31u8; 32]);
    let terminal_token = base64url::encode(&[32u8; 32]);
    let other_terminal_token = base64url::encode(&[33u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let user_cookie = cookie(&token, &terminal_token);
    seed_previous_sessions(&server);

    let empty = get_previous(&server, &user_cookie, TRAINING_ID, BENCH_EXERCISE_ID, None);
    assert_eq!(empty.status, 200, "{}", empty.body);
    let body = response_json(&empty);
    assert_previous_exercise(&body, 900.0, 2);
    assert_eq!(body["training"]["total_weight_kg"], 950.0);
    assert!(body["exercise"]["sets"][0]["grip"].is_null());
    assert_eq!(body["exercise"]["sets"][0]["kind"], "normal");
    assert!(body["exercise"]["sets"][0]["rpe"].is_null());
    assert_eq!(
        body["exercise"]["sets"][0]["segments"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        body["exercise"]["sets"][0]["segments"][0]["weight_kg"],
        100.0
    );
    assert_eq!(body["exercise"]["sets"][0]["segments"][0]["reps"], 5);
    assert_eq!(
        body["exercise"]["sets"][1]["segments"][0]["weight_kg"],
        80.0
    );

    let training_only = server.request(
        "GET",
        &format!("/api/trainings/{TRAINING_ID}/previous"),
        &[("Cookie", &user_cookie)],
    );
    assert_eq!(training_only.status, 200, "{}", training_only.body);
    let body = response_json(&training_only);
    assert!(body["exercise"].is_null());
    assert_eq!(body["training"]["total_weight_kg"], 950.0);

    // 未終了のトレーニングにある完了セットは、前回に選ばない。
    let unfinished = get_previous(&server, &user_cookie, TRAINING_ID, BENCH_EXERCISE_ID, None);
    assert_eq!(unfinished.status, 200, "{}", unfinished.body);
    assert_previous_exercise(&response_json(&unfinished), 900.0, 2);

    let grip = get_previous(
        &server,
        &user_cookie,
        TRAINING_ID,
        BENCH_EXERCISE_ID,
        Some("grip"),
    );
    assert_eq!(grip.status, 200, "{}", grip.body);
    let body = response_json(&grip);
    assert_previous_exercise(&body, 500.0, 1);
    assert_eq!(
        body["exercise"]["sets"][0]["grip"],
        json!({"grip": {"style": "pronated", "width": "medium"}})
    );
    assert_eq!(body["training"]["total_weight_kg"], 950.0);

    let missing_grip = get_previous(
        &server,
        &user_cookie,
        TRAINING_ID,
        BENCH_EXERCISE_ID,
        Some("other_grip"),
    );
    assert!(response_json(&missing_grip)["exercise"].is_null());
    assert_eq!(
        response_json(&missing_grip)["training"]["total_weight_kg"],
        950.0
    );

    let program = get_previous(
        &server,
        &user_cookie,
        PROGRAM_TRAINING_ID,
        PLANK_EXERCISE_ID,
        None,
    );
    assert_eq!(program.status, 200, "{}", program.body);
    let body = response_json(&program);
    assert_eq!(body["training"]["total_weight_kg"], 700.0);
    assert_previous_time(&body, 30, 30);

    let fallback = get_previous(
        &server,
        &user_cookie,
        FALLBACK_TRAINING_ID,
        PLANK_EXERCISE_ID,
        None,
    );
    assert_eq!(fallback.status, 200, "{}", fallback.body);
    assert_eq!(
        response_json(&fallback)["training"]["total_weight_kg"],
        800.0,
        "a missing program day must fall back to the routine"
    );

    let custom = get_previous(
        &server,
        &user_cookie,
        CUSTOM_TRAINING_ID,
        PLANK_EXERCISE_ID,
        None,
    );
    assert_eq!(custom.status, 200, "{}", custom.body);
    let body = response_json(&custom);
    assert_eq!(body["training"]["total_weight_kg"], 600.0);
    assert_previous_time(&body, 30, 30);
    assert_eq!(body["exercise"]["sets"][0]["kind"], "normal");
    assert_eq!(
        body["exercise"]["sets"][0]["segments"][0]["duration_seconds"],
        30
    );
    assert_eq!(
        body["exercise"]["sets"][0]["segments"][0]["weight_kg"], 0.0,
        "時間で記録するセットの重量は 0 である"
    );

    let routine = get_previous(
        &server,
        &user_cookie,
        ROUTINE_TRAINING_ID,
        PLANK_EXERCISE_ID,
        None,
    );
    assert_eq!(routine.status, 200, "{}", routine.body);
    let body = response_json(&routine);
    assert_eq!(body["training"]["total_weight_kg"], 800.0);
    assert_previous_time(&body, 30, 30);

    let no_previous = get_previous(
        &server,
        &user_cookie,
        TRAINING_ID,
        "3f2504e0-4f89-41d3-9a0c-0305e82c4932",
        None,
    );
    assert!(response_json(&no_previous)["exercise"].is_null());
    assert_eq!(
        response_json(&no_previous)["training"]["total_weight_kg"],
        950.0
    );

    let missing_exercise = server.request(
        "GET",
        &format!("/api/trainings/{TRAINING_ID}/previous"),
        &[("Cookie", &user_cookie)],
    );
    assert_eq!(missing_exercise.status, 200, "{}", missing_exercise.body);
    let body = response_json(&missing_exercise);
    assert!(body["exercise"].is_null());
    assert_eq!(body["training"]["total_weight_kg"], 950.0);
    let invalid_grip = server.request(
        "GET",
        &format!(
            "/api/trainings/{TRAINING_ID}/previous?exercise_id={BENCH_EXERCISE_ID}&grip=not-json"
        ),
        &[("Cookie", &user_cookie)],
    );
    assert_eq!(invalid_grip.status, 400);
    let foreign = server.request(
        "GET",
        &format!("/api/trainings/{OTHER_TRAINING_ID}/previous?exercise_id={BENCH_EXERCISE_ID}"),
        &[("Cookie", &user_cookie)],
    );
    assert_eq!(foreign.status, 404);
    let unauthorized = server.request(
        "GET",
        &format!("/api/trainings/{TRAINING_ID}/previous?exercise_id={BENCH_EXERCISE_ID}"),
        &[],
    );
    assert_eq!(unauthorized.status, 401);
}

const PROGRAM_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4913";
const CUSTOM_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4914";
const ROUTINE_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4915";
const FALLBACK_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4916";
const MISSING_PROGRAM_DAY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b11";
const PAST_EMPTY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4989";
const PAST_EMPTY_LATEST_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4981";
const PAST_GRIP_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4982";
const PAST_PROGRAM_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4983";
const PAST_ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4984";
const PAST_CUSTOM_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4985";
const PAST_PROGRAM_DAY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4986";
const PAST_CUSTOM_DAY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4987";
const PAST_ROUTINE_ID_VALUE: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4988";
const UNFINISHED_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4c01";
const UNFINISHED_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4c02";
const UNFINISHED_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4c03";
const UNFINISHED_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4c04";
