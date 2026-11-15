//! 反動と補助の回数、RPE の結合テスト。
//!
//! 対応する要求: `REQ-SET-006`、`REQ-SET-007`、`REQ-SET-009`。
//! 反動と補助の回数を段に保存し、合計が回数を超えるセットを拒否することを確かめる。
//! RPE の 6.0、6.5、10.0、空を保存し、範囲外と 0.5 刻みでない値を拒否することを確かめる。
//! 総重量が補助の回数だけを除き、自己ベストの種目別の総重量がストリクト回数で
//! 計算されることを終了のサマリーで確かめる。左右別のセットでは、同じ補助の回数を
//! 左右それぞれに適用し、総重量が 230kg になることを確かめる。

#[allow(dead_code)]
#[path = "support/record_set.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::{record_common, DevServer};
use warrior_core::base64url;

const PORT: u16 = 8845;

const WARMUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a10";
const WARMUP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a11";
const WARMUP_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a12";
const WARMUP_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a13";
const NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a20";
const NORMAL_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a21";
const NORMAL_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a22";
const DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a30";
const DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a31";
const DROP_SEGMENT_ONE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a32";
const DROP_SEGMENT_TWO_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a33";
const INVALID_RPE_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a40";
const INVALID_RPE_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a41";
const INVALID_RPE_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a42";
const OVER_REPS_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a50";
const OVER_REPS_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a51";
const OVER_REPS_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a52";
const COMPLETE_NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a60";
const COMPLETE_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a61";
const UNILATERAL_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a80";
const UNILATERAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a81";
const UNILATERAL_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a82";
const UNILATERAL_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a83";
const COMPLETE_UNILATERAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a84";
const UNILATERAL_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000018";

#[test]
fn saves_the_reps_breakdown_and_rpe_and_rejects_invalid_values() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[101u8; 32]);
    let terminal_token = base64url::encode(&[102u8; 32]);
    let other_terminal_token = base64url::encode(&[103u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    seed_unilateral_exercise(&server);
    let cookie = cookie(&token, &terminal_token);

    create_the_warmup_with_an_equal_breakdown(&server, &cookie);
    update_the_rpe_boundaries(&server, &cookie);
    reject_invalid_rpe_updates(&server, &cookie);
    clear_the_rpe(&server, &cookie);
    reject_invalid_creates(&server, &cookie);
    create_the_normal_set(&server, &cookie);
    create_the_drop_set(&server, &cookie);
    create_the_unilateral_set(&server, &cookie);
    check_the_record(&server, &cookie);
    complete_the_sets(&server, &cookie);
    finish_and_check_the_summary(&server, &cookie);
}

fn create_the_warmup_with_an_equal_breakdown(server: &DevServer, cookie: &str) {
    let mut warmup = create_operation(
        WARMUP_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        WARMUP_SET_ID,
        SetSpec {
            position: 1,
            kind: "warmup",
            body_weight_kg: Some(70.0),
        },
        vec![breakdown_segment(WARMUP_SEGMENT_ID, 1, 50.0, 10, 5, 5)],
    );
    warmup["payload"]["rpe"] = json!(6.0);
    let response = post(server, cookie, &warmup);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT rpe FROM sets WHERE id = '{WARMUP_SET_ID}'")
        )["rpe"]
            .as_f64(),
        Some(6.0)
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT momentum_reps, assisted_reps FROM set_segments \
                 WHERE id = '{WARMUP_SEGMENT_ID}'"
            )
        ),
        json!({"momentum_reps": 5, "assisted_reps": 5})
    );
}

fn update_the_rpe_boundaries(server: &DevServer, cookie: &str) {
    for (operation_id, rpe) in [
        (WARMUP_UPDATE_OPERATION_ID, 6.5),
        ("3f2504e0-4f89-41d3-9a0c-0305e82c5a14", 10.0),
    ] {
        let mut update = update_operation(
            operation_id,
            TRAINING_ID,
            WARMUP_SET_ID,
            UpdateSpec {
                position: 1,
                kind: "warmup",
                completed: false,
                body_weight_kg: Some(70.0),
            },
            vec![breakdown_segment(WARMUP_SEGMENT_ID, 1, 50.0, 10, 5, 5)],
        );
        update["payload"]["rpe"] = json!(rpe);
        let response = post(server, cookie, &update);
        assert_eq!(response.status, 200, "{}", response.body);
        assert_eq!(status(&response), "applied");
        assert_eq!(
            first_row(
                server,
                &format!("SELECT rpe FROM sets WHERE id = '{WARMUP_SET_ID}'")
            )["rpe"]
                .as_f64(),
            Some(rpe)
        );
    }
}

fn reject_invalid_rpe_updates(server: &DevServer, cookie: &str) {
    for (index, rpe) in [5.5, 10.5, 7.25].into_iter().enumerate() {
        let operation_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c5a{:02}", 70 + index);
        let mut update = update_operation(
            &operation_id,
            TRAINING_ID,
            WARMUP_SET_ID,
            UpdateSpec {
                position: 1,
                kind: "warmup",
                completed: false,
                body_weight_kg: Some(70.0),
            },
            vec![breakdown_segment(WARMUP_SEGMENT_ID, 1, 50.0, 10, 5, 5)],
        );
        update["payload"]["rpe"] = json!(rpe);
        let response = post(server, cookie, &update);
        assert_eq!(response.status, 400, "{}", response.body);
        assert_eq!(applied_count(server, &operation_id), 0);
        assert_eq!(
            first_row(
                server,
                &format!("SELECT rpe FROM sets WHERE id = '{WARMUP_SET_ID}'")
            )["rpe"]
                .as_f64(),
            Some(10.0)
        );
    }
}

fn clear_the_rpe(server: &DevServer, cookie: &str) {
    let mut update = update_operation(
        "3f2504e0-4f89-41d3-9a0c-0305e82c5a15",
        TRAINING_ID,
        WARMUP_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "warmup",
            completed: false,
            body_weight_kg: Some(70.0),
        },
        vec![breakdown_segment(WARMUP_SEGMENT_ID, 1, 50.0, 10, 5, 5)],
    );
    update["payload"]["rpe"] = Value::Null;
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT rpe FROM sets WHERE id = '{WARMUP_SET_ID}'")
        )["rpe"],
        Value::Null
    );
}

fn reject_invalid_creates(server: &DevServer, cookie: &str) {
    let mut invalid_rpe = create_operation(
        INVALID_RPE_CREATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        INVALID_RPE_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(INVALID_RPE_SEGMENT_ID, 1, 100.0, 5)],
    );
    invalid_rpe["payload"]["rpe"] = json!(5.5);
    let response = post(server, cookie, &invalid_rpe);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        count_with(
            server,
            &format!("SELECT COUNT(*) AS count FROM sets WHERE id = '{INVALID_RPE_SET_ID}'")
        ),
        0
    );
    assert_eq!(count(server, "set_segments", INVALID_RPE_SEGMENT_ID), 0);
    assert_eq!(applied_count(server, INVALID_RPE_CREATE_OPERATION_ID), 0);

    let mut over_reps = create_operation(
        OVER_REPS_CREATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        OVER_REPS_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![breakdown_segment(OVER_REPS_SEGMENT_ID, 1, 100.0, 5, 3, 3)],
    );
    over_reps["payload"]["rpe"] = json!(6.0);
    let response = post(server, cookie, &over_reps);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "sets", OVER_REPS_SET_ID), 0);
    assert_eq!(count(server, "set_segments", OVER_REPS_SEGMENT_ID), 0);
    assert_eq!(applied_count(server, OVER_REPS_CREATE_OPERATION_ID), 0);
}

fn create_the_normal_set(server: &DevServer, cookie: &str) {
    let mut normal = create_operation(
        NORMAL_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        NORMAL_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![breakdown_segment(NORMAL_SEGMENT_ID, 1, 100.0, 10, 2, 1)],
    );
    normal["payload"]["rpe"] = json!(8.0);
    let response = post(server, cookie, &normal);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT rpe FROM sets WHERE id = '{NORMAL_SET_ID}'")
        )["rpe"]
            .as_f64(),
        Some(8.0)
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT momentum_reps, assisted_reps FROM set_segments \
                 WHERE id = '{NORMAL_SEGMENT_ID}'"
            )
        ),
        json!({"momentum_reps": 2, "assisted_reps": 1})
    );
}

fn create_the_drop_set(server: &DevServer, cookie: &str) {
    let mut drop = create_operation(
        DROP_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        DROP_SET_ID,
        SetSpec {
            position: 3,
            kind: "drop",
            body_weight_kg: Some(70.0),
        },
        vec![
            breakdown_segment(DROP_SEGMENT_ONE_ID, 1, 100.0, 8, 3, 0),
            breakdown_segment(DROP_SEGMENT_TWO_ID, 2, 80.0, 6, 0, 2),
        ],
    );
    drop["payload"]["rpe"] = json!(9.0);
    let response = post(server, cookie, &drop);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT rpe FROM sets WHERE id = '{DROP_SET_ID}'")
        )["rpe"]
            .as_f64(),
        Some(9.0)
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT momentum_reps, assisted_reps FROM set_segments \
                 WHERE id = '{DROP_SEGMENT_ONE_ID}'"
            )
        ),
        json!({"momentum_reps": 3, "assisted_reps": 0})
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT momentum_reps, assisted_reps FROM set_segments \
                 WHERE id = '{DROP_SEGMENT_TWO_ID}'"
            )
        ),
        json!({"momentum_reps": 0, "assisted_reps": 2})
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
            position: 4,
        },
        "unilateral",
    );
}

fn create_the_unilateral_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        UNILATERAL_OPERATION_ID,
        TRAINING_ID,
        UNILATERAL_ROW_ID,
        UNILATERAL_SET_ID,
        SetSpec {
            position: 4,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![json!({
            "segment_id": UNILATERAL_SEGMENT_ID,
            "position": 1,
            "weight_kg": 0.0,
            "reps": 0,
            "momentum_reps": 0,
            "assisted_reps": 1,
            "duration_seconds": null,
            "left": {"weight_kg": 10.0, "reps": 10, "duration_seconds": null},
            "right": {"weight_kg": 20.0, "reps": 8, "duration_seconds": null}
        })],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    let stored = first_row(
        server,
        &format!(
            "SELECT left_weight_kg, left_reps, right_weight_kg, right_reps, assisted_reps \
             FROM set_segments WHERE id = '{UNILATERAL_SEGMENT_ID}'"
        ),
    );
    assert_eq!(stored["left_weight_kg"].as_f64(), Some(10.0));
    assert_eq!(stored["left_reps"].as_i64(), Some(10));
    assert_eq!(stored["right_weight_kg"].as_f64(), Some(20.0));
    assert_eq!(stored["right_reps"].as_i64(), Some(8));
    assert_eq!(stored["assisted_reps"].as_i64(), Some(1));
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
    let normal = sets
        .iter()
        .find(|set| set["id"] == NORMAL_SET_ID)
        .expect("the normal set");
    assert_eq!(normal["rpe"].as_f64(), Some(8.0));
    let drop = sets
        .iter()
        .find(|set| set["id"] == DROP_SET_ID)
        .expect("the drop set");
    assert_eq!(drop["rpe"].as_f64(), Some(9.0));
    let unilateral = sets
        .iter()
        .find(|set| set["id"] == UNILATERAL_SET_ID)
        .expect("the unilateral set");
    assert_eq!(unilateral["rpe"], Value::Null);
    let segments = record["segments"].as_array().expect("the segments");
    let normal_segment = segments
        .iter()
        .find(|segment| segment["id"] == NORMAL_SEGMENT_ID)
        .expect("the normal segment");
    assert_eq!(normal_segment["momentum_reps"], 2);
    assert_eq!(normal_segment["assisted_reps"], 1);
    let unilateral_segment = segments
        .iter()
        .find(|segment| segment["id"] == UNILATERAL_SEGMENT_ID)
        .expect("the unilateral segment");
    assert_eq!(unilateral_segment["left_reps"], 10);
    assert_eq!(unilateral_segment["right_reps"], 8);
    assert_eq!(unilateral_segment["assisted_reps"], 1);
}

fn complete_the_sets(server: &DevServer, cookie: &str) {
    for (operation_id, set_id) in [
        (COMPLETE_NORMAL_OPERATION_ID, NORMAL_SET_ID),
        (COMPLETE_DROP_OPERATION_ID, DROP_SET_ID),
        (COMPLETE_UNILATERAL_OPERATION_ID, UNILATERAL_SET_ID),
    ] {
        let complete = complete_operation(operation_id, TRAINING_ID, set_id, true);
        let response = post(server, cookie, &complete);
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
    assert_eq!(finish.status, 200, "{}", finish.body);
    let summary: Value = serde_json::from_str(&finish.body).expect("a summary");
    assert_eq!(summary["set_count"], 3);
    assert_eq!(summary["total_weight_kg"].as_f64(), Some(2_250.0));
    let bests = summary["personal_bests"].as_array().expect("the bests");
    let bench_total = bests
        .iter()
        .find(|item| item["kind"] == "total_weight" && item["exercise_id"] == BENCH_EXERCISE_ID)
        .expect("the bench total weight best");
    assert_eq!(bench_total["value"].as_f64(), Some(1_520.0));
    let unilateral_total = bests
        .iter()
        .find(|item| {
            item["kind"] == "total_weight" && item["exercise_id"] == UNILATERAL_EXERCISE_ID
        })
        .expect("the unilateral total weight best");
    assert_eq!(unilateral_total["value"].as_f64(), Some(230.0));
}

fn breakdown_segment(
    segment_id: &str,
    position: i64,
    weight_kg: f64,
    reps: u32,
    momentum_reps: u32,
    assisted_reps: u32,
) -> Value {
    let mut segment = segment(segment_id, position, weight_kg, reps);
    segment["momentum_reps"] = json!(momentum_reps);
    segment["assisted_reps"] = json!(assisted_reps);
    segment
}
