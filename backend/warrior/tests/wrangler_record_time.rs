//! 時間で記録するセットの結合テスト。
//!
//! 対応する要求: `REQ-SET-010`、`REQ-NFR-019`。
//! 時間のセットを保存し、左右別の時間と RPE、総重量と推定 1RM の対象外、
//! 部位別のセット数、最長時間の自己ベストをサマリーで確かめる。
//! 1 秒未満と 86400 秒を超える時間を 400 で拒否し、上限ちょうどを受け付ける。

#[allow(dead_code)]
#[path = "support/record_set.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::record_common;
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8849;

const PLANK_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5600";
const SIDE_PLANK_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5601";
const PLANK_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5602";
const SIDE_PLANK_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5603";
const PLANK_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5610";
const PLANK_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5611";
const MAX_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5612";
const MAX_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5613";
const SIDE_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5614";
const SIDE_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5615";
const PLANK_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5620";
const MAX_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5621";
const SIDE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5622";
const ZERO_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5630";
const ZERO_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5631";
const ZERO_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5632";
const OVER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5633";
const OVER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5634";
const OVER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5635";
const SIDE_ZERO_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5636";
const SIDE_ZERO_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5637";
const SIDE_ZERO_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5638";
const SIDE_OVER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5639";
const SIDE_OVER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c563a";
const SIDE_OVER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c563b";
const COMPLETE_PLANK_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5640";
const COMPLETE_MAX_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5641";
const COMPLETE_SIDE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5642";
const EMPTY_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5650";
const EMPTY_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5651";
const EMPTY_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5652";
const EMPTY_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5653";
const EMPTY_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5654";
const REPS_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5655";
const REPS_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5656";
const REPS_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5657";
const WEIGHT_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5658";
const WEIGHT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5659";
const WEIGHT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c565a";
const MOMENTUM_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5663";
const MOMENTUM_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5664";
const MOMENTUM_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5665";
const ASSISTED_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5666";
const ASSISTED_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5667";
const ASSISTED_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5668";
const SEEDED_REPS_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c565b";
const SEEDED_REPS_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c565c";
const SEEDED_REPS_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c565d";
const PLANK_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b0";
const PLANK_UPDATE_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b1";
const PLANK_UPDATE_AGAIN_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59b2";
const SIDE_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c565e";
const SIDE_ONLY_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5660";
const SIDE_ONLY_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5661";
const SIDE_ONLY_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5662";
const SIDE_BOTH_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5669";
const SIDE_BOTH_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c566a";
const SIDE_BOTH_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c566b";
const SIDE_BOTH_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c566c";

#[test]
fn saves_time_sets_and_checks_the_duration_bests() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[111u8; 32]);
    let terminal_token = base64url::encode(&[112u8; 32]);
    let other_terminal_token = base64url::encode(&[113u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    seed_time_exercises(&server);
    let cookie = cookie(&token, &terminal_token);

    create_the_plank_set(&server, &cookie);
    create_the_maximum_plank_set(&server, &cookie);
    create_the_side_plank_set(&server, &cookie);
    reject_a_duration_only_update_for_a_unilateral_time_set(&server, &cookie);
    reject_a_top_level_duration_for_a_unilateral_time_set(&server, &cookie);
    reject_side_and_top_level_duration_for_a_unilateral_time_set(&server, &cookie);
    create_the_empty_plank_set(&server, &cookie);
    reject_non_time_values(&server, &cookie);
    seed_a_reps_only_time_set(&server, &cookie);
    reject_invalid_durations(&server, &cookie);
    check_the_record(&server, &cookie);
    update_the_plank_set_with_duration_only(&server, &cookie);
    complete_the_sets(&server, &cookie);
    finish_and_check_the_summary(&server, &cookie);
}

fn seed_time_exercises(server: &DevServer) {
    record_common::seed_exercise(
        server,
        record_common::ExerciseSeed {
            row_id: PLANK_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: PLANK_EXERCISE_ID,
            name: "プランク",
            body_part: "abs",
            muscle: "rectus_abdominis_upper",
            record_mode: "time",
            position: 3,
            grip_mode: "none",
            grip_options: &[],
        },
    );
    record_common::seed_exercise_with_laterality(
        server,
        record_common::ExerciseSeed {
            row_id: SIDE_PLANK_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: SIDE_PLANK_EXERCISE_ID,
            name: "サイドプランク",
            body_part: "abs",
            muscle: "obliques",
            record_mode: "time",
            position: 4,
            grip_mode: "none",
            grip_options: &[],
        },
        "unilateral",
    );
}

fn create_the_plank_set(server: &DevServer, cookie: &str) {
    let mut operation = create_operation(
        PLANK_OPERATION_ID,
        TRAINING_ID,
        PLANK_ROW_ID,
        PLANK_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![duration_segment(PLANK_SEGMENT_ID, 1, 90)],
    );
    operation["payload"]["rpe"] = json!(8.0);
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    let row = first_row(
        server,
        &format!(
            "SELECT duration_seconds, weight_kg, reps, momentum_reps, assisted_reps \
             FROM set_segments WHERE id = '{PLANK_SEGMENT_ID}'"
        ),
    );
    assert_eq!(row["duration_seconds"].as_i64(), Some(90));
    assert!(row["weight_kg"].is_null());
    assert!(row["reps"].is_null());
    assert_eq!(row["momentum_reps"].as_i64(), Some(0));
    assert_eq!(row["assisted_reps"].as_i64(), Some(0));
    assert_eq!(
        first_row(
            server,
            &format!("SELECT rpe FROM sets WHERE id = '{PLANK_SET_ID}'")
        )["rpe"]
            .as_f64(),
        Some(8.0)
    );
}

fn create_the_maximum_plank_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        MAX_OPERATION_ID,
        TRAINING_ID,
        PLANK_ROW_ID,
        MAX_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![duration_segment(MAX_SEGMENT_ID, 1, 86_400)],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT duration_seconds FROM set_segments WHERE id = '{MAX_SEGMENT_ID}'")
        )["duration_seconds"]
            .as_i64(),
        Some(86_400)
    );
}

fn create_the_side_plank_set(server: &DevServer, cookie: &str) {
    let mut operation = create_operation(
        SIDE_OPERATION_ID,
        TRAINING_ID,
        SIDE_PLANK_ROW_ID,
        SIDE_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![side_duration_segment(SIDE_SEGMENT_ID, 1, 30, 40)],
    );
    operation["payload"]["rpe"] = json!(7.5);
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT left_duration_seconds, right_duration_seconds, \
                 left_weight_kg, left_reps, right_weight_kg, right_reps, \
                 momentum_reps, assisted_reps \
                 FROM set_segments WHERE id = '{SIDE_SEGMENT_ID}'"
            )
        ),
        json!({
            "left_duration_seconds": 30,
            "right_duration_seconds": 40,
            "left_weight_kg": null,
            "left_reps": null,
            "right_weight_kg": null,
            "right_reps": null,
            "momentum_reps": 0,
            "assisted_reps": 0
        })
    );
}

fn reject_a_duration_only_update_for_a_unilateral_time_set(server: &DevServer, cookie: &str) {
    let update = update_operation(
        SIDE_UPDATE_OPERATION_ID,
        TRAINING_ID,
        SIDE_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![duration_segment(SIDE_SEGMENT_ID, 1, 30)],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, SIDE_UPDATE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{SIDE_SET_ID}'")
        )["completed"],
        0
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT left_duration_seconds, right_duration_seconds \
                 FROM set_segments WHERE id = '{SIDE_SEGMENT_ID}'"
            )
        ),
        json!({"left_duration_seconds": 30, "right_duration_seconds": 40})
    );
}

fn reject_a_top_level_duration_for_a_unilateral_time_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        SIDE_ONLY_OPERATION_ID,
        TRAINING_ID,
        SIDE_PLANK_ROW_ID,
        SIDE_ONLY_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![duration_segment(SIDE_ONLY_SEGMENT_ID, 1, 60)],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, SIDE_ONLY_OPERATION_ID), 0);
    assert_eq!(count(server, "sets", SIDE_ONLY_SET_ID), 0);
    assert_eq!(count(server, "set_segments", SIDE_ONLY_SEGMENT_ID), 0);
}

fn reject_side_and_top_level_duration_for_a_unilateral_time_set(server: &DevServer, cookie: &str) {
    let mut segment = side_duration_segment(SIDE_SEGMENT_ID, 1, 30, 40);
    segment["duration_seconds"] = json!(60);
    let update = update_operation(
        SIDE_BOTH_UPDATE_OPERATION_ID,
        TRAINING_ID,
        SIDE_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![segment],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, SIDE_BOTH_UPDATE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT duration_seconds, left_duration_seconds, right_duration_seconds \
                 FROM set_segments WHERE id = '{SIDE_SEGMENT_ID}'"
            )
        ),
        json!({
            "duration_seconds": Value::Null,
            "left_duration_seconds": 30,
            "right_duration_seconds": 40
        })
    );

    for sql in [
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, \
             created_at, updated_at) \
             VALUES ('{SIDE_BOTH_SET_ID}', '{USER_ID}', '{SIDE_PLANK_ROW_ID}', 2, 'normal', 0, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, \
             momentum_reps, assisted_reps, duration_seconds, left_weight_kg, left_reps, \
             left_duration_seconds, right_weight_kg, right_reps, right_duration_seconds, \
             created_at, updated_at) \
             VALUES ('{SIDE_BOTH_SEGMENT_ID}', '{USER_ID}', '{SIDE_BOTH_SET_ID}', 1, \
              NULL, NULL, 0, 0, 60, NULL, NULL, 30, NULL, NULL, 40, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the side and top level duration set must be seeded");
    }
    let complete = complete_operation(
        SIDE_BOTH_COMPLETE_OPERATION_ID,
        TRAINING_ID,
        SIDE_BOTH_SET_ID,
        true,
    );
    let response = post(server, cookie, &complete);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, SIDE_BOTH_COMPLETE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{SIDE_BOTH_SET_ID}'")
        )["completed"],
        0
    );
}

fn create_the_empty_plank_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        EMPTY_OPERATION_ID,
        TRAINING_ID,
        PLANK_ROW_ID,
        EMPTY_SET_ID,
        SetSpec {
            position: 3,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![segment(EMPTY_SEGMENT_ID, 1, 0.0, 0)],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    let row = first_row(
        server,
        &format!(
            "SELECT weight_kg, reps, duration_seconds, momentum_reps, assisted_reps \
             FROM set_segments WHERE id = '{EMPTY_SEGMENT_ID}'"
        ),
    );
    assert!(row["weight_kg"].is_null());
    assert!(row["reps"].is_null());
    assert!(row["duration_seconds"].is_null());
    assert_eq!(row["momentum_reps"].as_i64(), Some(0));
    assert_eq!(row["assisted_reps"].as_i64(), Some(0));

    let complete = complete_operation(EMPTY_COMPLETE_OPERATION_ID, TRAINING_ID, EMPTY_SET_ID, true);
    let response = post(server, cookie, &complete);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{EMPTY_SET_ID}'")
        )["completed"],
        0
    );
    assert_eq!(applied_count(server, EMPTY_COMPLETE_OPERATION_ID), 0);
}

fn reject_non_time_values(server: &DevServer, cookie: &str) {
    for (operation_id, set_id, segment_id, weight_kg, reps, momentum, assisted) in [
        (
            REPS_OPERATION_ID,
            REPS_SET_ID,
            REPS_SEGMENT_ID,
            0.0,
            5,
            0,
            0,
        ),
        (
            WEIGHT_OPERATION_ID,
            WEIGHT_SET_ID,
            WEIGHT_SEGMENT_ID,
            12.5,
            0,
            0,
            0,
        ),
        (
            MOMENTUM_OPERATION_ID,
            MOMENTUM_SET_ID,
            MOMENTUM_SEGMENT_ID,
            0.0,
            5,
            1,
            0,
        ),
        (
            ASSISTED_OPERATION_ID,
            ASSISTED_SET_ID,
            ASSISTED_SEGMENT_ID,
            0.0,
            5,
            0,
            1,
        ),
    ] {
        let mut segment = segment(segment_id, 1, weight_kg, reps);
        segment["momentum_reps"] = json!(momentum);
        segment["assisted_reps"] = json!(assisted);
        let operation = create_operation(
            operation_id,
            TRAINING_ID,
            PLANK_ROW_ID,
            set_id,
            SetSpec {
                position: 5,
                kind: "normal",
                body_weight_kg: None,
            },
            vec![segment],
        );
        let response = post(server, cookie, &operation);
        assert_eq!(response.status, 400, "{}", response.body);
        assert_eq!(applied_count(server, operation_id), 0);
        assert_eq!(count(server, "sets", set_id), 0);
        assert_eq!(count(server, "set_segments", segment_id), 0);
    }

    let update = update_operation(
        EMPTY_UPDATE_OPERATION_ID,
        TRAINING_ID,
        EMPTY_SET_ID,
        UpdateSpec {
            position: 3,
            kind: "normal",
            completed: false,
            body_weight_kg: None,
        },
        vec![segment(EMPTY_SEGMENT_ID, 1, 12.5, 5)],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, EMPTY_UPDATE_OPERATION_ID), 0);
    let stored = first_row(
        server,
        &format!(
            "SELECT weight_kg, reps, duration_seconds FROM set_segments \
             WHERE id = '{EMPTY_SEGMENT_ID}'"
        ),
    );
    assert!(stored["weight_kg"].is_null());
    assert!(stored["reps"].is_null());
    assert!(stored["duration_seconds"].is_null());
}

fn seed_a_reps_only_time_set(server: &DevServer, cookie: &str) {
    for sql in [
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, \
             created_at, updated_at) \
             VALUES ('{SEEDED_REPS_SET_ID}', '{USER_ID}', '{PLANK_ROW_ID}', 4, 'normal', 0, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, \
             momentum_reps, assisted_reps, duration_seconds, created_at, updated_at) \
             VALUES ('{SEEDED_REPS_SEGMENT_ID}', '{USER_ID}', '{SEEDED_REPS_SET_ID}', 1, \
              NULL, 5, 0, 0, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the reps-only time set must be seeded");
    }
    let complete = complete_operation(
        SEEDED_REPS_COMPLETE_OPERATION_ID,
        TRAINING_ID,
        SEEDED_REPS_SET_ID,
        true,
    );
    let response = post(server, cookie, &complete);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{SEEDED_REPS_SET_ID}'")
        )["completed"],
        0
    );
    assert_eq!(applied_count(server, SEEDED_REPS_COMPLETE_OPERATION_ID), 0);
}

fn reject_invalid_durations(server: &DevServer, cookie: &str) {
    for (operation_id, set_id, segment_id, duration) in [
        (ZERO_OPERATION_ID, ZERO_SET_ID, ZERO_SEGMENT_ID, 0),
        (OVER_OPERATION_ID, OVER_SET_ID, OVER_SEGMENT_ID, 86_401),
    ] {
        let operation = create_operation(
            operation_id,
            TRAINING_ID,
            PLANK_ROW_ID,
            set_id,
            SetSpec {
                position: 3,
                kind: "normal",
                body_weight_kg: None,
            },
            vec![duration_segment(segment_id, 1, duration)],
        );
        let response = post(server, cookie, &operation);
        assert_eq!(response.status, 400, "{}", response.body);
        assert_eq!(
            count_with(
                server,
                &format!("SELECT COUNT(*) AS count FROM sets WHERE id = '{set_id}'")
            ),
            0
        );
        assert_eq!(count(server, "set_segments", segment_id), 0);
        assert_eq!(applied_count(server, operation_id), 0);
    }

    for (operation_id, set_id, segment_id, left, right) in [
        (
            SIDE_ZERO_OPERATION_ID,
            SIDE_ZERO_SET_ID,
            SIDE_ZERO_SEGMENT_ID,
            0,
            40,
        ),
        (
            SIDE_OVER_OPERATION_ID,
            SIDE_OVER_SET_ID,
            SIDE_OVER_SEGMENT_ID,
            86_401,
            40,
        ),
    ] {
        let operation = create_operation(
            operation_id,
            TRAINING_ID,
            SIDE_PLANK_ROW_ID,
            set_id,
            SetSpec {
                position: 2,
                kind: "normal",
                body_weight_kg: None,
            },
            vec![side_duration_segment(segment_id, 1, left, right)],
        );
        let response = post(server, cookie, &operation);
        assert_eq!(response.status, 400, "{}", response.body);
        assert_eq!(count(server, "sets", set_id), 0);
        assert_eq!(count(server, "set_segments", segment_id), 0);
        assert_eq!(applied_count(server, operation_id), 0);
    }
}

fn check_the_record(server: &DevServer, cookie: &str) {
    let response = server.request(
        "GET",
        &format!("/api/trainings/{TRAINING_ID}"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    let record: Value = serde_json::from_str(&response.body).expect("a training record");
    let sets = record["sets"].as_array().expect("the sets");
    let plank = sets
        .iter()
        .find(|set| set["id"] == PLANK_SET_ID)
        .expect("the plank set");
    assert_eq!(plank["rpe"].as_f64(), Some(8.0));
    let side = sets
        .iter()
        .find(|set| set["id"] == SIDE_SET_ID)
        .expect("the side plank set");
    assert_eq!(side["rpe"].as_f64(), Some(7.5));
    let segments = record["segments"].as_array().expect("the segments");
    let plank_segment = segments
        .iter()
        .find(|segment| segment["id"] == PLANK_SEGMENT_ID)
        .expect("the plank segment");
    assert_eq!(plank_segment["duration_seconds"], 90);
    assert!(plank_segment["weight_kg"].is_null());
    assert!(plank_segment["reps"].is_null());
    let side_segment = segments
        .iter()
        .find(|segment| segment["id"] == SIDE_SEGMENT_ID)
        .expect("the side plank segment");
    assert_eq!(side_segment["left_duration_seconds"], 30);
    assert_eq!(side_segment["right_duration_seconds"], 40);
    assert!(side_segment["left_weight_kg"].is_null());
    assert!(side_segment["left_reps"].is_null());
    assert!(side_segment["right_weight_kg"].is_null());
    assert!(side_segment["right_reps"].is_null());
}

fn update_the_plank_set_with_duration_only(server: &DevServer, cookie: &str) {
    let update = update_operation(
        PLANK_UPDATE_OPERATION_ID,
        TRAINING_ID,
        PLANK_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: false,
            body_weight_kg: None,
        },
        vec![duration_segment(PLANK_SEGMENT_ID, 1, 60)],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{PLANK_SET_ID}'")
        )["completed"],
        0,
        "a duration-only update without completion stays incomplete"
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT duration_seconds FROM set_segments WHERE id = '{PLANK_SEGMENT_ID}'")
        )["duration_seconds"],
        60
    );
    assert_eq!(applied_count(server, PLANK_UPDATE_OPERATION_ID), 1);

    let complete = update_operation(
        PLANK_UPDATE_COMPLETE_OPERATION_ID,
        TRAINING_ID,
        PLANK_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![duration_segment(PLANK_SEGMENT_ID, 1, 60)],
    );
    let response = post(server, cookie, &complete);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{PLANK_SET_ID}'")
        )["completed"],
        1
    );
    assert_eq!(applied_count(server, PLANK_UPDATE_COMPLETE_OPERATION_ID), 1);

    let again = update_operation(
        PLANK_UPDATE_AGAIN_OPERATION_ID,
        TRAINING_ID,
        PLANK_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![duration_segment(PLANK_SEGMENT_ID, 1, 120)],
    );
    let response = post(server, cookie, &again);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{PLANK_SET_ID}'")
        )["completed"],
        1,
        "a completed time set stays complete after a rejected update"
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT duration_seconds FROM set_segments WHERE id = '{PLANK_SEGMENT_ID}'")
        )["duration_seconds"],
        60,
        "a completed time set keeps the stored duration"
    );
    assert_eq!(applied_count(server, PLANK_UPDATE_AGAIN_OPERATION_ID), 0);
}

fn complete_the_sets(server: &DevServer, cookie: &str) {
    for (operation_id, set_id) in [
        (COMPLETE_PLANK_OPERATION_ID, PLANK_SET_ID),
        (COMPLETE_MAX_OPERATION_ID, MAX_SET_ID),
        (COMPLETE_SIDE_OPERATION_ID, SIDE_SET_ID),
    ] {
        let operation = complete_operation(operation_id, TRAINING_ID, set_id, true);
        let response = post(server, cookie, &operation);
        assert_eq!(response.status, 200, "{}", response.body);
        assert_eq!(status(&response), "applied");
        assert_eq!(
            first_row(
                server,
                &format!("SELECT completed FROM sets WHERE id = '{set_id}'")
            )["completed"],
            1
        );
    }
}

fn finish_and_check_the_summary(server: &DevServer, cookie: &str) {
    let finish = server.request(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
    );
    assert_eq!(
        finish.status,
        200,
        "{}\n{:?}",
        finish.body,
        server.log_lines()
    );
    let summary: Value = serde_json::from_str(&finish.body).expect("a summary");
    assert_eq!(summary["total_weight_kg"].as_f64(), Some(0.0));
    assert_eq!(summary["set_count"].as_u64(), Some(3));
    assert_eq!(summary["exercise_count"].as_u64(), Some(2));
    let bests = summary["personal_bests"].as_array().expect("the bests");
    assert!(bests.iter().all(|item| item["kind"] == "longest_duration"));
    let plank_best = bests
        .iter()
        .find(|item| item["exercise_id"] == PLANK_EXERCISE_ID)
        .expect("the plank best");
    assert_eq!(plank_best["value"].as_f64(), Some(86_400.0));
    let side_best = bests
        .iter()
        .find(|item| item["exercise_id"] == SIDE_PLANK_EXERCISE_ID)
        .expect("the side plank best");
    assert_eq!(side_best["value"].as_f64(), Some(40.0));
}

fn duration_segment(segment_id: &str, position: i64, duration_seconds: i64) -> Value {
    let mut segment = segment(segment_id, position, 0.0, 0);
    segment["duration_seconds"] = json!(duration_seconds);
    segment
}

fn side_duration_segment(
    segment_id: &str,
    position: i64,
    left_duration_seconds: i64,
    right_duration_seconds: i64,
) -> Value {
    let mut segment = segment(segment_id, position, 0.0, 0);
    segment["left"] = json!({
        "weight_kg": 0.0,
        "reps": 0,
        "duration_seconds": left_duration_seconds
    });
    segment["right"] = json!({
        "weight_kg": 0.0,
        "reps": 0,
        "duration_seconds": right_duration_seconds
    });
    segment
}
