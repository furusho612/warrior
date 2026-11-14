//! 左右別のセットとスーパーセットの結合テスト。
//!
//! 対応する要求: `REQ-SET-004`、`REQ-SET-005`。
//! 左右別のセットが左右の値を保存し、総重量が左右の合算になり、左右の片方または両方が
//! 無いセットを作成・完了できないことを確かめる。スーパーセットの組が同じトレーニングの
//! 種目だけを受け付け、既に組に属する種目を含む組を拒否し、並べ替えで組の種目を変えられず、
//! 種目の削除で 2 つ未満になった組を解散することを確かめる。

#[allow(dead_code)]
#[path = "support/record_set.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::{record_common, DevServer};
use warrior_core::base64url;

const UNILATERAL_PORT: u16 = 8838;
const SUPERSET_PORT: u16 = 8839;
const TIME_PORT: u16 = 8842;
const CONCURRENT_PORT: u16 = 8843;

const UNILATERAL_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5910";
const UNILATERAL_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5911";
const SECOND_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5912";
const THIRD_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5913";
const FOURTH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5916";
const SECOND_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5914";
const THIRD_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5915";
const FOURTH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5917";
const TIME_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b0";
const TIME_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b1";
const TIME_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b2";
const TIME_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b3";
const TIME_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b4";
const TIME_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b5";

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

const TWO_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5970";
const TWO_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5971";
const REORDER_EXERCISE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5972";
const REORDER_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5973";
const DELETE_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5974";
const DELETE_SUPERSET_AGAIN_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5975";
const EMPTY_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5976";
const EMPTY_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5977";
const ONE_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a1";
const ONE_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a2";
const DUPLICATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a3";
const DUPLICATE_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a4";
const SECOND_GROUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a5";
const SECOND_GROUP_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a6";
const CHANGING_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a7";
const DELETE_SECOND_GROUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a8";
const DUPLICATE_SECOND_GROUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a9";
const DUPLICATE_SECOND_GROUP_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59aa";
const CONCURRENT_A_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ab";
const CONCURRENT_A_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ac";
const CONCURRENT_B_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ad";
const CONCURRENT_B_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ae";
const FOREIGN_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5978";
const FOREIGN_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5979";
const THREE_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5980";
const THREE_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5981";
const DELETE_SECOND_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5982";
const DELETE_THIRD_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5983";

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
fn supersets_keep_members_in_order_and_dissolve_when_one_remains() {
    let server = DevServer::start(SUPERSET_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[94u8; 32]);
    let terminal_token = base64url::encode(&[95u8; 32]);
    let other_terminal_token = base64url::encode(&[96u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    seed_superset_exercises(&server);

    create_and_reorder_the_two_member_group(&server, &session_cookie);
    reject_a_reorder_that_changes_the_group(&server, &session_cookie);
    reject_a_duplicate_group(&server, &session_cookie);
    delete_the_two_member_group(&server, &session_cookie);
    reject_the_empty_and_foreign_groups(&server, &session_cookie);
    dissolve_the_three_member_group(&server, &session_cookie);
}

#[test]
fn concurrent_group_creates_conflict_on_the_member_index() {
    let server = DevServer::start(CONCURRENT_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[100u8; 32]);
    let terminal_token = base64url::encode(&[101u8; 32]);
    let other_terminal_token = base64url::encode(&[102u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    seed_superset_exercises(&server);

    let first = superset_create_operation(
        CONCURRENT_A_OPERATION_ID,
        FINISHED_TRAINING_ID,
        CONCURRENT_A_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let second = superset_create_operation(
        CONCURRENT_B_OPERATION_ID,
        FINISHED_TRAINING_ID,
        CONCURRENT_B_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| post(&server, &session_cookie, &first));
        let second = scope.spawn(|| post(&server, &session_cookie, &second));
        (
            first.join().expect("the first create"),
            second.join().expect("the second create"),
        )
    });
    let mut statuses = vec![first.status, second.status];
    statuses.sort_unstable();
    assert_eq!(statuses, vec![200, 409], "{}", first.body);
    assert_eq!(
        count(&server, "supersets", CONCURRENT_A_SUPERSET_ID)
            + count(&server, "supersets", CONCURRENT_B_SUPERSET_ID),
        1
    );
}

#[test]
fn unilateral_time_sets_keep_the_top_level_duration() {
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
            "duration_seconds": 60,
            "left": null,
            "right": null
        })],
    );
    let response = post(&server, &session_cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);

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

fn seed_superset_exercises(server: &DevServer) {
    for (row_id, exercise_id, name, position) in [
        (SECOND_ROW_ID, SECOND_EXERCISE_ID, "ダンベルプレス", 2),
        (THIRD_ROW_ID, THIRD_EXERCISE_ID, "ダンベルフライ", 3),
        (FOURTH_ROW_ID, FOURTH_EXERCISE_ID, "ダンベルロウ", 4),
    ] {
        record_common::seed_exercise(
            server,
            record_common::ExerciseSeed {
                row_id,
                user_id: USER_ID,
                training_id: FINISHED_TRAINING_ID,
                exercise_id,
                name,
                body_part: "chest",
                muscle: "pectoralis_major_middle",
                record_mode: "weight_and_reps",
                position,
            },
        );
    }
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

fn create_and_reorder_the_two_member_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        TWO_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID, SECOND_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, TWO_MEMBER_SUPERSET_ID),
        vec![FINISHED_ROW_ID.to_owned(), SECOND_ROW_ID.to_owned()]
    );

    let operation = exercise_reorder_operation(
        REORDER_EXERCISE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        &[SECOND_ROW_ID, FINISHED_ROW_ID, THIRD_ROW_ID, FOURTH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);

    let operation = superset_reorder_operation(
        REORDER_SUPERSET_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
        &[SECOND_ROW_ID, FINISHED_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, TWO_MEMBER_SUPERSET_ID),
        vec![SECOND_ROW_ID.to_owned(), FINISHED_ROW_ID.to_owned()]
    );
}

fn reject_a_reorder_that_changes_the_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        SECOND_GROUP_OPERATION_ID,
        FINISHED_TRAINING_ID,
        SECOND_GROUP_SUPERSET_ID,
        &[THIRD_ROW_ID, FOURTH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, SECOND_GROUP_SUPERSET_ID),
        vec![THIRD_ROW_ID.to_owned(), FOURTH_ROW_ID.to_owned()]
    );

    let operation = superset_reorder_operation(
        CHANGING_REORDER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, CHANGING_REORDER_OPERATION_ID), 0);
    assert_eq!(
        member_ids(server, TWO_MEMBER_SUPERSET_ID),
        vec![SECOND_ROW_ID.to_owned(), FINISHED_ROW_ID.to_owned()]
    );
    assert_eq!(
        member_ids(server, SECOND_GROUP_SUPERSET_ID),
        vec![THIRD_ROW_ID.to_owned(), FOURTH_ROW_ID.to_owned()]
    );

    let operation = superset_create_operation(
        DUPLICATE_SECOND_GROUP_OPERATION_ID,
        FINISHED_TRAINING_ID,
        DUPLICATE_SECOND_GROUP_SUPERSET_ID,
        &[THIRD_ROW_ID, FOURTH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(
        count(server, "supersets", DUPLICATE_SECOND_GROUP_SUPERSET_ID),
        0
    );

    let operation = superset_delete_operation(
        DELETE_SECOND_GROUP_OPERATION_ID,
        FINISHED_TRAINING_ID,
        SECOND_GROUP_SUPERSET_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "supersets", SECOND_GROUP_SUPERSET_ID), 0);
}

fn delete_the_two_member_group(server: &DevServer, cookie: &str) {
    let operation = superset_delete_operation(
        DELETE_SUPERSET_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "supersets", TWO_MEMBER_SUPERSET_ID), 0);
    assert!(member_ids(server, TWO_MEMBER_SUPERSET_ID).is_empty());

    let operation = superset_delete_operation(
        DELETE_SUPERSET_AGAIN_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        applied_count(server, DELETE_SUPERSET_AGAIN_OPERATION_ID),
        1,
        "a delete after the group is gone is recorded to stop the resend"
    );
}

fn reject_the_empty_and_foreign_groups(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        EMPTY_SUPERSET_OPERATION_ID,
        FINISHED_TRAINING_ID,
        EMPTY_SUPERSET_ID,
        &[],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "supersets", EMPTY_SUPERSET_ID), 0);

    let operation = superset_create_operation(
        ONE_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        ONE_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "supersets", ONE_MEMBER_SUPERSET_ID), 0);

    let operation = superset_create_operation(
        FOREIGN_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        FOREIGN_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID, BENCH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "supersets", FOREIGN_MEMBER_SUPERSET_ID), 0);
}

fn reject_a_duplicate_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        DUPLICATE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        DUPLICATE_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(count(server, "supersets", DUPLICATE_SUPERSET_ID), 0);
    assert_eq!(applied_count(server, DUPLICATE_OPERATION_ID), 0);
}

fn dissolve_the_three_member_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        THREE_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        THREE_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID, SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, THREE_MEMBER_SUPERSET_ID),
        vec![
            FINISHED_ROW_ID.to_owned(),
            SECOND_ROW_ID.to_owned(),
            THIRD_ROW_ID.to_owned()
        ]
    );

    let operation = exercise_delete_operation(
        DELETE_SECOND_OPERATION_ID,
        FINISHED_TRAINING_ID,
        SECOND_ROW_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, THREE_MEMBER_SUPERSET_ID),
        vec![FINISHED_ROW_ID.to_owned(), THIRD_ROW_ID.to_owned()]
    );

    let operation = exercise_delete_operation(
        DELETE_THIRD_OPERATION_ID,
        FINISHED_TRAINING_ID,
        THIRD_ROW_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "supersets", THREE_MEMBER_SUPERSET_ID), 0);
    assert!(member_ids(server, THREE_MEMBER_SUPERSET_ID).is_empty());
    assert_eq!(
        count_with(
            server,
            &format!(
                "SELECT COUNT(*) AS count FROM superset_members \
                 WHERE superset_id = '{THREE_MEMBER_SUPERSET_ID}'"
            )
        ),
        0
    );
}

fn side_segment(
    segment_id: &str,
    position: i64,
    left_weight_kg: f64,
    left_reps: u32,
    right_weight_kg: f64,
    right_reps: u32,
) -> Value {
    json!({
        "segment_id": segment_id,
        "position": position,
        "weight_kg": 0.0,
        "reps": 0,
        "momentum_reps": 0,
        "assisted_reps": 0,
        "duration_seconds": null,
        "left": {
            "weight_kg": left_weight_kg,
            "reps": left_reps,
            "duration_seconds": null
        },
        "right": {
            "weight_kg": right_weight_kg,
            "reps": right_reps,
            "duration_seconds": null
        }
    })
}

fn superset_create_operation(
    operation_id: &str,
    training_id: &str,
    superset_id: &str,
    member_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 1,
        "kind": "superset_create",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "superset_id": superset_id,
            "member_ids": member_ids
        }
    })
}

fn superset_delete_operation(operation_id: &str, training_id: &str, superset_id: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 2,
        "kind": "superset_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "superset_id": superset_id
        }
    })
}

fn superset_reorder_operation(
    operation_id: &str,
    training_id: &str,
    superset_id: &str,
    member_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 3,
        "kind": "superset_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "superset_id": superset_id,
            "member_ids": member_ids
        }
    })
}

fn exercise_delete_operation(
    operation_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 4,
        "kind": "training_exercise_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "training_exercise_id": training_exercise_id
        }
    })
}

fn exercise_reorder_operation(
    operation_id: &str,
    training_id: &str,
    ordered_ids: &[&str],
) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": training_id,
        "sequence": 5,
        "kind": "training_exercise_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": training_id,
            "ordered_ids": ordered_ids
        }
    })
}

fn member_ids(server: &DevServer, superset_id: &str) -> Vec<String> {
    let value = server
        .execute_d1_json(&format!(
            "SELECT training_exercise_id FROM superset_members \
             WHERE superset_id = '{superset_id}' ORDER BY position"
        ))
        .expect("the members must be readable");
    value
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    row.get("training_exercise_id")
                        .and_then(|id| id.as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}
