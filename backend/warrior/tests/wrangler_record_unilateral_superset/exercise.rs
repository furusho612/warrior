//! 左右別のセットの結合テスト。
//!
//! 対応する要求: `REQ-SET-004`、`REQ-SET-005`。
//! 左右別のセットが左右の値を保存し、総重量が左右の合算になり、重量と回数で記録する
//! 種目では左右の片方または両方が無いセットを作成・完了できないことを確かめる。
//! 自重と回数で記録する種目は、左右の加重が空でも完了できる。

use serde_json::{json, Value};
use warrior_core::base64url;

use crate::helpers::*;
use crate::support::record_unilateral_superset::{side_segment, superset_create_operation};
use crate::support::{record_common, DevServer};

const UNILATERAL_PORT: u16 = 8838;
const TIME_PORT: u16 = 8842;
const BODYWEIGHT_PORT: u16 = 8844;

const UNILATERAL_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5910";
const UNILATERAL_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5911";
const TIME_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b0";
const TIME_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b1";
const TIME_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b2";
const TIME_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b3";
const TIME_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b4";
const TIME_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b5";
const BODYWEIGHT_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59c0";
const BODYWEIGHT_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59c1";
const BODYWEIGHT_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59c2";
const BODYWEIGHT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59c3";
const BODYWEIGHT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59c4";
const BODYWEIGHT_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59c5";

const NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5920";
const NORMAL_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5921";
const NORMAL_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5922";
const DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5930";
const DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5931";
const DROP_SEGMENT_ONE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5932";
const DROP_SEGMENT_TWO_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5933";
const PARTIAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5940";
const PARTIAL_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5941";
const PARTIAL_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5942";
const NO_SIDE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5990";
const NO_SIDE_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5991";
const NO_SIDE_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5992";
const NO_WEIGHT_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5993";
const NO_WEIGHT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5994";
const NO_WEIGHT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5995";
const NO_WEIGHT_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c599b";
const NO_WEIGHT_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c599c";
const DRAFT_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5996";
const DRAFT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5997";
const DRAFT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5998";
const DRAFT_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5999";
const DRAFT_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c599a";
const COMPLETE_NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5950";
const COMPLETE_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5951";
const PARTIAL_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5952";
const FORBIDDEN_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5960";
const FORBIDDEN_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5961";

#[test]
fn unilateral_sets_sum_both_sides_and_require_both_to_complete() {
    let server = DevServer::start(UNILATERAL_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[91u8; 32]);
    let terminal_token = base64url::encode(&[92u8; 32]);
    let other_terminal_token = base64url::encode(&[93u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    let other_cookie = cookie(&token, &other_terminal_token);
    seed_unilateral_exercise(&server);

    create_and_complete_the_normal_set(&server, &session_cookie);
    create_and_complete_the_drop_set(&server, &session_cookie);
    reject_the_partial_set(&server, &session_cookie);
    reject_the_set_without_sides(&server, &session_cookie);
    reject_a_superset_from_another_terminal(&server, &other_cookie);
    finish_and_check_the_unilateral_totals(&server, &session_cookie);
}

#[test]
fn unilateral_bodyweight_sets_complete_without_an_added_weight() {
    let server = DevServer::start(BODYWEIGHT_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[103u8; 32]);
    let terminal_token = base64url::encode(&[104u8; 32]);
    let other_terminal_token = base64url::encode(&[105u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    seed_unilateral_bodyweight_exercise(&server);

    let operation = create_operation(
        BODYWEIGHT_OPERATION_ID,
        TRAINING_ID,
        BODYWEIGHT_ROW_ID,
        BODYWEIGHT_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![json!({
            "segment_id": BODYWEIGHT_SEGMENT_ID,
            "position": 1,
            "weight_kg": 0.0,
            "reps": 0,
            "momentum_reps": 0,
            "assisted_reps": 0,
            "duration_seconds": null,
            "left": {"weight_kg": null, "reps": 5, "duration_seconds": null},
            "right": {"weight_kg": null, "reps": 4, "duration_seconds": null}
        })],
    );
    let response = post(&server, &session_cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);

    let response = post(
        &server,
        &session_cookie,
        &complete_operation(
            BODYWEIGHT_COMPLETE_OPERATION_ID,
            TRAINING_ID,
            BODYWEIGHT_SET_ID,
            true,
        ),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT completed FROM sets WHERE id = '{BODYWEIGHT_SET_ID}'")
        )["completed"],
        1,
        "a unilateral bodyweight set must complete with an empty added weight"
    );
    let stored = first_row(
        &server,
        &format!(
            "SELECT left_weight_kg, left_reps, right_weight_kg, right_reps \
             FROM set_segments WHERE id = '{BODYWEIGHT_SEGMENT_ID}'"
        ),
    );
    assert_eq!(stored["left_weight_kg"], Value::Null);
    assert_eq!(stored["left_reps"].as_i64(), Some(5));
    assert_eq!(stored["right_weight_kg"], Value::Null);
    assert_eq!(stored["right_reps"].as_i64(), Some(4));
}

#[test]
fn unilateral_time_sets_keep_the_side_durations() {
    let server = DevServer::start(TIME_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[97u8; 32]);
    let terminal_token = base64url::encode(&[98u8; 32]);
    let other_terminal_token = base64url::encode(&[99u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    seed_unilateral_time_exercise(&server);

    let operation = create_operation(
        TIME_OPERATION_ID,
        TRAINING_ID,
        TIME_ROW_ID,
        TIME_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![json!({
            "segment_id": TIME_SEGMENT_ID,
            "position": 1,
            "weight_kg": 0.0,
            "reps": 0,
            "momentum_reps": 0,
            "assisted_reps": 0,
            "duration_seconds": null,
            "left": {"weight_kg": null, "reps": 0, "duration_seconds": 30},
            "right": {"weight_kg": null, "reps": 0, "duration_seconds": 60}
        })],
    );
    let response = post(&server, &session_cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            &server,
            &format!(
                "SELECT duration_seconds, left_duration_seconds, right_duration_seconds \
                 FROM set_segments WHERE id = '{TIME_SEGMENT_ID}'"
            )
        ),
        json!({
            "duration_seconds": Value::Null,
            "left_duration_seconds": 30,
            "right_duration_seconds": 60
        })
    );

    let response = post(
        &server,
        &session_cookie,
        &complete_operation(TIME_COMPLETE_OPERATION_ID, TRAINING_ID, TIME_SET_ID, true),
    );
    assert_eq!(response.status, 200, "{}", response.body);

    let response = server.request(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", &session_cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let summary: Value = serde_json::from_str(&response.body).expect("a summary");
    let longest = summary["personal_bests"]
        .as_array()
        .expect("personal bests")
        .iter()
        .find(|item| item["kind"] == "longest_duration")
        .expect("a longest duration best");
    assert_eq!(longest["value"].as_f64(), Some(60.0));

    let day = server.request(
        "GET",
        "/api/calendar/2026-11-12",
        &[("Cookie", &session_cookie)],
    );
    assert_eq!(day.status, 200, "{}", day.body);
    assert!(day.body.contains(TIME_EXERCISE_ID), "{}", day.body);

    let previous = server.request(
        "GET",
        &format!("/api/trainings/{TRAINING_ID}/previous?exercise_id={TIME_EXERCISE_ID}"),
        &[("Cookie", &session_cookie)],
    );
    assert_eq!(previous.status, 200, "{}", previous.body);
}

fn seed_unilateral_time_exercise(server: &DevServer) {
    record_common::seed_exercise_with_laterality(
        server,
        record_common::ExerciseSeed {
            row_id: TIME_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: TIME_EXERCISE_ID,
            name: "サイドプランク",
            body_part: "abs",
            muscle: "rectus_abdominis_upper",
            record_mode: "time",
            position: 4,
        },
        "unilateral",
    );
}

fn seed_unilateral_bodyweight_exercise(server: &DevServer) {
    record_common::seed_exercise_with_laterality(
        server,
        record_common::ExerciseSeed {
            row_id: BODYWEIGHT_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: BODYWEIGHT_EXERCISE_ID,
            name: "ワンハンドプッシュアップ",
            body_part: "chest",
            muscle: "pectoralis_major_middle",
            record_mode: "bodyweight_and_reps",
            position: 3,
        },
        "unilateral",
    );
}

fn seed_unilateral_exercise(server: &DevServer) {
    record_common::seed_exercise_with_laterality(
        server,
        record_common::ExerciseSeed {
            row_id: UNILATERAL_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: UNILATERAL_EXERCISE_ID,
            name: "ワンハンドダンベルロウ",
            body_part: "back",
            muscle: "latissimus_dorsi",
            record_mode: "weight_and_reps",
            position: 3,
        },
        "unilateral",
    );
}

fn create_and_complete_the_normal_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        NORMAL_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        NORMAL_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![side_segment(NORMAL_SEGMENT_ID, 1, 10.0, 5, 12.0, 4)],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    let stored = first_row(
        server,
        &format!(
            "SELECT left_weight_kg, left_reps, right_weight_kg, right_reps \
             FROM set_segments WHERE id = '{NORMAL_SEGMENT_ID}'"
        ),
    );
    assert_eq!(stored["left_weight_kg"].as_f64(), Some(10.0));
    assert_eq!(stored["left_reps"].as_i64(), Some(5));
    assert_eq!(stored["right_weight_kg"].as_f64(), Some(12.0));
    assert_eq!(stored["right_reps"].as_i64(), Some(4));

    let response = post(
        server,
        cookie,
        &complete_operation(
            COMPLETE_NORMAL_OPERATION_ID,
            TRAINING_ID,
            NORMAL_SET_ID,
            true,
        ),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{NORMAL_SET_ID}'")
        )["completed"],
        1
    );
}

fn create_and_complete_the_drop_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        DROP_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        DROP_SET_ID,
        SetSpec {
            position: 2,
            kind: "drop",
            body_weight_kg: None,
        },
        vec![
            side_segment(DROP_SEGMENT_ONE_ID, 1, 8.0, 6, 9.0, 5),
            side_segment(DROP_SEGMENT_TWO_ID, 2, 6.0, 10, 7.0, 8),
        ],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    let response = post(
        server,
        cookie,
        &complete_operation(COMPLETE_DROP_OPERATION_ID, TRAINING_ID, DROP_SET_ID, true),
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{DROP_SET_ID}'")
        )["completed"],
        1
    );
}

fn reject_the_partial_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        PARTIAL_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        PARTIAL_SET_ID,
        SetSpec {
            position: 3,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![json!({
            "segment_id": PARTIAL_SEGMENT_ID,
            "position": 1,
            "weight_kg": 0.0,
            "reps": 0,
            "momentum_reps": 0,
            "assisted_reps": 0,
            "duration_seconds": null,
            "left": {"weight_kg": 10.0, "reps": 5, "duration_seconds": null},
            "right": null
        })],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "sets", PARTIAL_SET_ID), 0);
    assert_eq!(applied_count(server, PARTIAL_OPERATION_ID), 0);

    // 左右の片方だけを持つセットは保存できないため、直接用意して完了の検証だけを行う。
    record_common::seed_set(
        server,
        record_common::SetSeed {
            set_id: PARTIAL_SET_ID,
            user_id: USER_ID,
            training_exercise_id: UNILATERAL_ROW_ID,
            position: 3,
            kind: "normal",
            completed: 0,
            grip: None,
        },
    );
    server
        .execute_d1_json(&format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              left_weight_kg, left_reps, created_at, updated_at) \
             VALUES ('{PARTIAL_SEGMENT_ID}', '{USER_ID}', '{PARTIAL_SET_ID}', 1, 0.0, 0, 0, 0, \
              10.0, 5, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the partial segment must be seeded");

    let response = post(
        server,
        cookie,
        &complete_operation(
            PARTIAL_COMPLETE_OPERATION_ID,
            TRAINING_ID,
            PARTIAL_SET_ID,
            true,
        ),
    );
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{PARTIAL_SET_ID}'")
        )["completed"],
        0
    );
    assert_eq!(applied_count(server, PARTIAL_COMPLETE_OPERATION_ID), 0);
}

fn reject_the_set_without_sides(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        NO_SIDE_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        NO_SIDE_SET_ID,
        SetSpec {
            position: 4,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![json!({
            "segment_id": NO_SIDE_SEGMENT_ID,
            "position": 1,
            "weight_kg": 100.0,
            "reps": 5,
            "momentum_reps": 0,
            "assisted_reps": 0,
            "duration_seconds": null,
            "left": null,
            "right": null
        })],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "sets", NO_SIDE_SET_ID), 0);
    assert_eq!(applied_count(server, NO_SIDE_OPERATION_ID), 0);

    reject_a_missing_side_weight(server, cookie);
    reject_a_completed_update_and_complete_without_reps(server, cookie);
}

fn reject_a_missing_side_weight(server: &DevServer, cookie: &str) {
    let segments = vec![json!({
        "segment_id": NO_WEIGHT_SEGMENT_ID,
        "position": 1,
        "weight_kg": 0.0,
        "reps": 0,
        "momentum_reps": 0,
        "assisted_reps": 0,
        "duration_seconds": null,
        "left": {"weight_kg": null, "reps": 5, "duration_seconds": null},
        "right": {"weight_kg": 12.0, "reps": 4, "duration_seconds": null}
    })];
    let operation = create_operation(
        NO_WEIGHT_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        NO_WEIGHT_SET_ID,
        SetSpec {
            position: 5,
            kind: "normal",
            body_weight_kg: None,
        },
        segments.clone(),
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);

    // 重量欄が空の左右別のセットは完了できない。
    let operation = update_operation(
        NO_WEIGHT_UPDATE_OPERATION_ID,
        TRAINING_ID,
        NO_WEIGHT_SET_ID,
        UpdateSpec {
            position: 5,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        segments,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{NO_WEIGHT_SET_ID}'")
        )["completed"],
        0,
        "an update must not complete a set whose weight is missing"
    );
    assert_eq!(applied_count(server, NO_WEIGHT_UPDATE_OPERATION_ID), 0);

    let response = post(
        server,
        cookie,
        &complete_operation(
            NO_WEIGHT_COMPLETE_OPERATION_ID,
            TRAINING_ID,
            NO_WEIGHT_SET_ID,
            true,
        ),
    );
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{NO_WEIGHT_SET_ID}'")
        )["completed"],
        0
    );
    assert_eq!(applied_count(server, NO_WEIGHT_COMPLETE_OPERATION_ID), 0);
}

fn reject_a_completed_update_and_complete_without_reps(server: &DevServer, cookie: &str) {
    let segments = vec![json!({
        "segment_id": DRAFT_SEGMENT_ID,
        "position": 1,
        "weight_kg": 0.0,
        "reps": 0,
        "momentum_reps": 0,
        "assisted_reps": 0,
        "duration_seconds": null,
        "left": {"weight_kg": 10.0, "reps": 0, "duration_seconds": null},
        "right": {"weight_kg": 12.0, "reps": 0, "duration_seconds": null}
    })];
    let operation = create_operation(
        DRAFT_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        DRAFT_SET_ID,
        SetSpec {
            position: 6,
            kind: "normal",
            body_weight_kg: None,
        },
        segments.clone(),
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);

    let operation = update_operation(
        DRAFT_UPDATE_OPERATION_ID,
        TRAINING_ID,
        DRAFT_SET_ID,
        UpdateSpec {
            position: 6,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        segments,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{DRAFT_SET_ID}'")
        )["completed"],
        0,
        "an update must not complete a set without reps"
    );
    assert_eq!(applied_count(server, DRAFT_UPDATE_OPERATION_ID), 0);

    let response = post(
        server,
        cookie,
        &complete_operation(DRAFT_COMPLETE_OPERATION_ID, TRAINING_ID, DRAFT_SET_ID, true),
    );
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{DRAFT_SET_ID}'")
        )["completed"],
        0
    );
    assert_eq!(applied_count(server, DRAFT_COMPLETE_OPERATION_ID), 0);
}

fn reject_a_superset_from_another_terminal(server: &DevServer, other_cookie: &str) {
    let operation = superset_create_operation(
        FORBIDDEN_SUPERSET_OPERATION_ID,
        TRAINING_ID,
        FORBIDDEN_SUPERSET_ID,
        &[BENCH_ROW_ID, UNILATERAL_ROW_ID],
    );
    let response = post(server, other_cookie, &operation);
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(count(server, "supersets", FORBIDDEN_SUPERSET_ID), 0);
    assert_eq!(applied_count(server, FORBIDDEN_SUPERSET_OPERATION_ID), 0);
}

fn finish_and_check_the_unilateral_totals(server: &DevServer, cookie: &str) {
    let response = server.request(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let summary: Value = serde_json::from_str(&response.body).expect("a summary");
    assert_eq!(summary["set_count"], 2);
    assert_eq!(summary["total_weight_kg"].as_f64(), Some(307.0));
}
