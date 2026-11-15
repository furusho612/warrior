//! ドロップセットの段と位置の制約の結合テスト。
//!
//! 対応する要求: `REQ-SET-003`、`REQ-SET-007`、`REQ-SET-009`。
//! 先頭とウォームアップの直後のドロップセットをサーバーが 400 で拒み、通常の
//! セットの直後のドロップセットを段と RPE とともに保存することを確かめる。
//! 段を全段持つ総重量と、セット数が 1 であることを終了のサマリーで確かめる。

#[allow(dead_code)]
#[path = "support/record_set.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8840;

const FIRST_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5810";
const FIRST_DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5811";
const FIRST_DROP_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5812";
const WARMUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5820";
const WARMUP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5821";
const WARMUP_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5822";
const AFTER_WARMUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5830";
const AFTER_WARMUP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5831";
const AFTER_WARMUP_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5832";
const NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5840";
const NORMAL_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5841";
const NORMAL_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5842";
const NORMAL_TO_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5850";
const DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5860";
const DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5861";
const DROP_SEGMENT_ONE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5862";
const DROP_SEGMENT_TWO_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5863";
const INVERSE_WARMUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5870";
const INVALID_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5880";
const VALID_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5881";
const COMPLETE_NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5890";
const COMPLETE_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5891";
const TEMP_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58a0";
const TEMP_DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58a1";
const TEMP_DROP_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58a2";
const INVALID_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58b0";
const TEMP_DROP_DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58b1";
const INVALID_COMPLETED_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58c0";
const DELETE_NORMAL_BEFORE_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58d0";
const DELETE_DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58d1";
const DELETE_NORMAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c58d2";

#[test]
fn drop_sets_reject_the_first_and_warmup_positions_and_keep_their_segments() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[81u8; 32]);
    let terminal_token = base64url::encode(&[82u8; 32]);
    let other_terminal_token = base64url::encode(&[83u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let cookie = cookie(&token, &terminal_token);

    reject_the_first_drop_set(&server, &cookie);
    reject_the_drop_set_after_a_warmup(&server, &cookie);
    create_the_normal_set(&server, &cookie);
    reject_changing_to_drop_after_the_warmup(&server, &cookie);
    create_the_drop_set(&server, &cookie);
    reject_changing_the_previous_set_to_a_warmup(&server, &cookie);
    reject_reordering_the_drop_set_to_the_first_position(&server, &cookie);
    accept_the_current_order(&server, &cookie);
    reject_deleting_the_preceding_set(&server, &cookie);
    reject_incomplete_drop_set_completion(&server, &cookie);
    complete_the_sets(&server, &cookie);
    reject_an_invalid_completed_update(&server, &cookie);
    finish_and_check_the_summary(&server, &cookie);
    reject_deleting_the_completed_sets(&server, &cookie);
}

fn reject_the_first_drop_set(server: &DevServer, cookie: &str) {
    let operation = create_operation(
        FIRST_DROP_OPERATION_ID,
        TRAINING_ID,
        BODYWEIGHT_ROW_ID,
        FIRST_DROP_SET_ID,
        SetSpec {
            position: 1,
            kind: "drop",
            body_weight_kg: Some(70.0),
        },
        vec![segment(FIRST_DROP_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "sets", FIRST_DROP_SET_ID), 0);
    assert_eq!(count(server, "set_segments", FIRST_DROP_SEGMENT_ID), 0);
    assert_eq!(applied_count(server, FIRST_DROP_OPERATION_ID), 0);
}

fn reject_the_drop_set_after_a_warmup(server: &DevServer, cookie: &str) {
    let warmup = create_operation(
        WARMUP_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        WARMUP_SET_ID,
        SetSpec {
            position: 1,
            kind: "warmup",
            body_weight_kg: Some(70.0),
        },
        vec![segment(WARMUP_SEGMENT_ID, 1, 50.0, 10)],
    );
    let response = post(server, cookie, &warmup);
    assert_eq!(response.status, 200, "{}", response.body);

    let drop = create_operation(
        AFTER_WARMUP_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        AFTER_WARMUP_SET_ID,
        SetSpec {
            position: 2,
            kind: "drop",
            body_weight_kg: Some(70.0),
        },
        vec![segment(AFTER_WARMUP_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(server, cookie, &drop);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "sets", AFTER_WARMUP_SET_ID), 0);
    assert_eq!(applied_count(server, AFTER_WARMUP_OPERATION_ID), 0);
}

fn create_the_normal_set(server: &DevServer, cookie: &str) {
    let normal = create_operation(
        NORMAL_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        NORMAL_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(NORMAL_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(server, cookie, &normal);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
}

fn reject_changing_to_drop_after_the_warmup(server: &DevServer, cookie: &str) {
    let update = update_operation(
        NORMAL_TO_DROP_OPERATION_ID,
        TRAINING_ID,
        NORMAL_SET_ID,
        UpdateSpec {
            position: 2,
            kind: "drop",
            completed: false,
            body_weight_kg: Some(70.0),
        },
        vec![segment(NORMAL_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT kind FROM sets WHERE id = '{NORMAL_SET_ID}'")
        )["kind"],
        "normal"
    );
    assert_eq!(applied_count(server, NORMAL_TO_DROP_OPERATION_ID), 0);
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
            segment(DROP_SEGMENT_ONE_ID, 1, 100.0, 8),
            segment(DROP_SEGMENT_TWO_ID, 2, 80.0, 6),
        ],
    );
    drop["payload"]["rpe"] = json!(8.0);
    let response = post(server, cookie, &drop);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT kind, rpe FROM sets WHERE id = '{DROP_SET_ID}'")
        )["kind"],
        "drop"
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT rpe FROM sets WHERE id = '{DROP_SET_ID}'")
        )["rpe"]
            .as_f64(),
        Some(8.0)
    );
    assert_eq!(
        count_with(
            server,
            &format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{DROP_SET_ID}'")
        ),
        2
    );
    assert_eq!(
        first_row(
            server,
            &format!(
                "SELECT weight_kg, reps FROM set_segments \
                 WHERE id = '{DROP_SEGMENT_TWO_ID}'"
            )
        ),
        json!({"weight_kg": 80, "reps": 6})
    );
}

fn reject_changing_the_previous_set_to_a_warmup(server: &DevServer, cookie: &str) {
    let update = update_operation(
        INVERSE_WARMUP_OPERATION_ID,
        TRAINING_ID,
        NORMAL_SET_ID,
        UpdateSpec {
            position: 2,
            kind: "warmup",
            completed: false,
            body_weight_kg: Some(70.0),
        },
        vec![segment(NORMAL_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT kind FROM sets WHERE id = '{NORMAL_SET_ID}'")
        )["kind"],
        "normal"
    );
    assert_eq!(applied_count(server, INVERSE_WARMUP_OPERATION_ID), 0);
}

fn reject_reordering_the_drop_set_to_the_first_position(server: &DevServer, cookie: &str) {
    let reorder = reorder_operation(
        INVALID_REORDER_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        &[DROP_SET_ID, WARMUP_SET_ID, NORMAL_SET_ID],
    );
    let response = post(server, cookie, &reorder);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, INVALID_REORDER_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT position FROM sets WHERE id = '{DROP_SET_ID}'")
        )["position"],
        3
    );
}

fn accept_the_current_order(server: &DevServer, cookie: &str) {
    let reorder = reorder_operation(
        VALID_REORDER_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        &[WARMUP_SET_ID, NORMAL_SET_ID, DROP_SET_ID],
    );
    let response = post(server, cookie, &reorder);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
}

fn reject_incomplete_drop_set_completion(server: &DevServer, cookie: &str) {
    let temp = create_operation(
        TEMP_DROP_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        TEMP_DROP_SET_ID,
        SetSpec {
            position: 4,
            kind: "drop",
            body_weight_kg: Some(70.0),
        },
        vec![
            segment(TEMP_DROP_SEGMENT_ID, 1, 100.0, 5),
            segment(EXTRA_SEGMENT_ID, 2, 80.0, 0),
        ],
    );
    let response = post(server, cookie, &temp);
    assert_eq!(response.status, 200, "{}", response.body);

    let complete = complete_operation(
        INVALID_COMPLETE_OPERATION_ID,
        TRAINING_ID,
        TEMP_DROP_SET_ID,
        true,
    );
    let response = post(server, cookie, &complete);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, INVALID_COMPLETE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{TEMP_DROP_SET_ID}'")
        )["completed"],
        0
    );

    let delete = delete_operation(TEMP_DROP_DELETE_OPERATION_ID, TRAINING_ID, TEMP_DROP_SET_ID);
    let response = post(server, cookie, &delete);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "sets", TEMP_DROP_SET_ID), 0);
}

fn reject_an_invalid_completed_update(server: &DevServer, cookie: &str) {
    let update = update_operation(
        INVALID_COMPLETED_UPDATE_OPERATION_ID,
        TRAINING_ID,
        DROP_SET_ID,
        UpdateSpec {
            position: 3,
            kind: "drop",
            completed: true,
            body_weight_kg: Some(70.0),
        },
        vec![
            segment(DROP_SEGMENT_ONE_ID, 1, 100.0, 8),
            segment(DROP_SEGMENT_TWO_ID, 2, 80.0, 0),
        ],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(
        applied_count(server, INVALID_COMPLETED_UPDATE_OPERATION_ID),
        0
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{DROP_SET_ID}'")
        )["completed"],
        1
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT reps FROM set_segments WHERE id = '{DROP_SEGMENT_TWO_ID}'")
        )["reps"],
        6
    );
}

fn complete_the_sets(server: &DevServer, cookie: &str) {
    for (operation_id, set_id) in [
        (COMPLETE_NORMAL_OPERATION_ID, NORMAL_SET_ID),
        (COMPLETE_DROP_OPERATION_ID, DROP_SET_ID),
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
    assert_eq!(summary["set_count"], 2);
    assert_eq!(summary["total_weight_kg"].as_f64(), Some(1_780.0));
}

fn reject_deleting_the_preceding_set(server: &DevServer, cookie: &str) {
    let blocked = delete_operation(
        DELETE_NORMAL_BEFORE_DROP_OPERATION_ID,
        TRAINING_ID,
        NORMAL_SET_ID,
    );
    let response = post(server, cookie, &blocked);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        applied_count(server, DELETE_NORMAL_BEFORE_DROP_OPERATION_ID),
        0
    );
    assert_eq!(count(server, "sets", NORMAL_SET_ID), 1);
}

fn reject_deleting_the_completed_sets(server: &DevServer, cookie: &str) {
    for (operation_id, set_id) in [
        (DELETE_DROP_OPERATION_ID, DROP_SET_ID),
        (DELETE_NORMAL_OPERATION_ID, NORMAL_SET_ID),
    ] {
        let delete = delete_operation(operation_id, TRAINING_ID, set_id);
        let response = post(server, cookie, &delete);
        assert_eq!(response.status, 409, "{}", response.body);
        assert_eq!(applied_count(server, operation_id), 0);
        assert_eq!(count(server, "sets", set_id), 1);
    }
}
