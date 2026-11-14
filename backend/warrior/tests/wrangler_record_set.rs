//! セットの追加、更新、削除、並べ替え、完了の結合テスト。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`、`REQ-LIB-004`、`REQ-PRF-005`。
//! セットの操作を同期の操作の適用で行い、D1 の行と操作 ID を確かめる。段は更新で
//! 全置換し、削除でセットと一緒に消す。自重種目は記録した時点の体重とセットごとの
//! 加重を別の列に保存する。

#[path = "support/record_set.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8822;
const SECOND_PORT: u16 = 8823;

const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5601";
const UPDATE_TWO_SEGMENTS_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5602";
const UPDATE_ONE_SEGMENT_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5603";
const COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5604";
const SECOND_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5605";
const REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5606";
const STALE_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5607";
const FOREIGN_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5608";
const OTHER_TERMINAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5609";
const NO_TERMINAL_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5610";
const FINISHED_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5611";
const FINISHED_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5612";
const DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5613";
const BODY_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5614";
const BODY_NO_WEIGHT_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5615";
const OTHER_DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5616";
const OTHER_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5617";
const FINISHED_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5620";
const FINISHED_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5621";
const EMPTY_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5630";
const EMPTY_COMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5631";
const EMPTY_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5632";
const EMPTY_SECOND_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5633";
const COMPLETED_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5634";
const COMPLETED_DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5635";
const COMPLETED_UNCOMPLETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5636";
const EMPTY_REJECTED_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5637";
const COMPLETED_CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5638";
const SECOND_SET_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5622";
const TAKEOVER_PORT: u16 = 8824;
const TAKEOVER_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5640";
const TAKEOVER_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5641";
const TAKEOVER_OLD_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5642";
const TAKEOVER_NEW_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5643";

#[test]
fn set_operations_are_idempotent_scoped_and_keep_the_segments() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[51u8; 32]);
    let terminal_token = base64url::encode(&[52u8; 32]);
    let other_terminal_token = base64url::encode(&[53u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let user_cookie = cookie(&token, &terminal_token);
    let other_cookie = cookie(&token, &other_terminal_token);
    let session_only_cookie = format!("warrior_session={token}");

    create_the_sets(&server, &user_cookie);
    update_and_complete_the_set(&server, &user_cookie);
    reorder_the_sets(&server, &user_cookie);
    reject_writes_without_the_editing_right(
        &server,
        &user_cookie,
        &other_cookie,
        &session_only_cookie,
    );
    edit_a_finished_training_without_a_terminal(&server, &session_only_cookie);
    delete_the_set_with_its_segments(&server, &user_cookie);
}

fn create_the_sets(server: &DevServer, cookie: &str) {
    let create = create_operation(
        CREATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        BENCH_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(server, cookie, &create);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    let stored_set = first_row(
        server,
        &format!(
            "SELECT position, kind, completed, body_weight_kg FROM sets \
             WHERE id = '{BENCH_SET_ID}'"
        ),
    );
    assert_eq!(stored_set["position"], 1);
    assert_eq!(stored_set["kind"], "normal");
    assert_eq!(stored_set["completed"], 0);
    assert_eq!(stored_set["body_weight_kg"].as_f64(), Some(70.0));
    let stored_segment = first_row(
        server,
        &format!("SELECT weight_kg, reps FROM set_segments WHERE id = '{BENCH_SEGMENT_ID}'"),
    );
    assert_eq!(stored_segment["weight_kg"].as_f64(), Some(100.0));
    assert_eq!(stored_segment["reps"], 5);
    assert_eq!(applied_count(server, CREATE_OPERATION_ID), 1);

    let mut replay = create.clone();
    replay["payload"]["segments"][0]["weight_kg"] = json!(999.0);
    let replay = post(server, cookie, &replay);
    assert_eq!(replay.status, 200);
    assert_eq!(status(&replay), "already_applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT weight_kg FROM set_segments WHERE id = '{BENCH_SEGMENT_ID}'")
        )["weight_kg"]
            .as_f64(),
        Some(100.0),
        "a replay must not change the stored segment"
    );

    let body = create_operation(
        BODY_OPERATION_ID,
        TRAINING_ID,
        BODYWEIGHT_ROW_ID,
        BODY_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(BODY_SEGMENT_ID, 1, 10.0, 5)],
    );
    let response = post(server, cookie, &body);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT body_weight_kg FROM sets WHERE id = '{BODY_SET_ID}'")
        )["body_weight_kg"]
            .as_f64(),
        Some(70.0)
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT weight_kg FROM set_segments WHERE id = '{BODY_SEGMENT_ID}'")
        )["weight_kg"]
            .as_f64(),
        Some(10.0),
        "the bodyweight set stores the added weight in the segment"
    );

    let no_weight = create_operation(
        BODY_NO_WEIGHT_OPERATION_ID,
        TRAINING_ID,
        BODYWEIGHT_ROW_ID,
        BODY_NO_WEIGHT_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![segment(BODY_NO_WEIGHT_SEGMENT_ID, 1, 10.0, 5)],
    );
    let response = post(server, cookie, &no_weight);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT body_weight_kg FROM sets WHERE id = '{BODY_NO_WEIGHT_SET_ID}'")
        ),
        json!({"body_weight_kg": null})
    );
}

fn update_and_complete_the_set(server: &DevServer, cookie: &str) {
    let two_segments = update_operation(
        UPDATE_TWO_SEGMENTS_OPERATION_ID,
        TRAINING_ID,
        BENCH_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "warmup",
            completed: false,
            body_weight_kg: Some(70.0),
        },
        vec![
            segment(BENCH_SEGMENT_ID, 1, 110.0, 6),
            segment(EXTRA_SEGMENT_ID, 2, 90.0, 8),
        ],
    );
    let response = post(server, cookie, &two_segments);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT kind FROM sets WHERE id = '{BENCH_SET_ID}'")
        )["kind"],
        "warmup"
    );
    assert_eq!(
        count_with(
            server,
            &format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{BENCH_SET_ID}'")
        ),
        2
    );

    let one_segment = update_operation(
        UPDATE_ONE_SEGMENT_OPERATION_ID,
        TRAINING_ID,
        BENCH_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: false,
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 120.0, 7)],
    );
    let response = post(server, cookie, &one_segment);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        count_with(
            server,
            &format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{BENCH_SET_ID}'")
        ),
        1,
        "the update replaces all segments"
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT weight_kg, reps FROM set_segments WHERE id = '{BENCH_SEGMENT_ID}'")
        ),
        json!({"weight_kg": 120, "reps": 7})
    );
    assert_eq!(count(server, "set_segments", EXTRA_SEGMENT_ID), 0);

    let complete = complete_operation(COMPLETE_OPERATION_ID, TRAINING_ID, BENCH_SET_ID, true);
    let response = post(server, cookie, &complete);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{BENCH_SET_ID}'")
        )["completed"],
        1
    );
    assert_eq!(applied_count(server, COMPLETE_OPERATION_ID), 1);

    let replay = post(server, cookie, &complete);
    assert_eq!(replay.status, 200);
    assert_eq!(status(&replay), "already_applied");
    assert_eq!(applied_count(server, COMPLETE_OPERATION_ID), 1);

    reject_edits_of_the_completed_set(server, cookie);
}

fn reject_edits_of_the_completed_set(server: &DevServer, cookie: &str) {
    let update = update_operation(
        COMPLETED_UPDATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 130.0, 5)],
    );
    let response = post(server, cookie, &update);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(applied_count(server, COMPLETED_UPDATE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT weight_kg FROM set_segments WHERE id = '{BENCH_SEGMENT_ID}'")
        )["weight_kg"]
            .as_f64(),
        Some(120.0),
        "an update of a completed set must keep the stored value"
    );

    let delete = delete_operation(COMPLETED_DELETE_OPERATION_ID, TRAINING_ID, BENCH_SET_ID);
    let response = post(server, cookie, &delete);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(applied_count(server, COMPLETED_DELETE_OPERATION_ID), 0);
    assert_eq!(count(server, "sets", BENCH_SET_ID), 1);
    assert_eq!(count(server, "set_segments", BENCH_SEGMENT_ID), 1);

    let create = create_operation(
        COMPLETED_CREATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        BENCH_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 130.0, 9)],
    );
    let response = post(server, cookie, &create);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(applied_count(server, COMPLETED_CREATE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{BENCH_SET_ID}'")
        )["completed"],
        1
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT weight_kg, reps FROM set_segments WHERE id = '{BENCH_SEGMENT_ID}'")
        ),
        json!({"weight_kg": 120, "reps": 7}),
        "a create with the completed set ID must keep the stored value"
    );

    let uncomplete = complete_operation(
        COMPLETED_UNCOMPLETE_OPERATION_ID,
        TRAINING_ID,
        BENCH_SET_ID,
        false,
    );
    let response = post(server, cookie, &uncomplete);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(applied_count(server, COMPLETED_UNCOMPLETE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{BENCH_SET_ID}'")
        )["completed"],
        1
    );
}

fn reorder_the_sets(server: &DevServer, cookie: &str) {
    let second = create_operation(
        SECOND_CREATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        SECOND_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(SECOND_SET_SEGMENT_ID, 1, 80.0, 5)],
    );
    let response = post(server, cookie, &second);
    assert_eq!(response.status, 200, "{}", response.body);

    let reorder = reorder_operation(
        REORDER_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        &[SECOND_SET_ID, BENCH_SET_ID],
    );
    let response = post(server, cookie, &reorder);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(
        first_row(
            server,
            &format!("SELECT position FROM sets WHERE id = '{SECOND_SET_ID}'")
        )["position"],
        1
    );
    assert_eq!(
        first_row(
            server,
            &format!("SELECT position FROM sets WHERE id = '{BENCH_SET_ID}'")
        )["position"],
        2
    );

    let replay = post(server, cookie, &reorder);
    assert_eq!(replay.status, 200);
    assert_eq!(status(&replay), "already_applied");

    let stale = reorder_operation(
        STALE_REORDER_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        &[SECOND_SET_ID],
    );
    let response = post(server, cookie, &stale);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(applied_count(server, STALE_REORDER_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT position FROM sets WHERE id = '{SECOND_SET_ID}'")
        )["position"],
        1
    );
}

fn reject_writes_without_the_editing_right(
    server: &DevServer,
    cookie: &str,
    other_cookie: &str,
    session_only_cookie: &str,
) {
    let foreign = update_operation(
        FOREIGN_UPDATE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        BENCH_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 999.0, 5)],
    );
    let response = post(server, cookie, &foreign);
    assert_eq!(response.status, 404, "{}", response.body);
    assert_eq!(applied_count(server, FOREIGN_UPDATE_OPERATION_ID), 0);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT weight_kg FROM set_segments WHERE id = '{BENCH_SEGMENT_ID}'")
        )["weight_kg"]
            .as_f64(),
        Some(120.0)
    );

    let other_terminal = update_operation(
        OTHER_TERMINAL_OPERATION_ID,
        TRAINING_ID,
        BENCH_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 130.0, 5)],
    );
    let response = post(server, other_cookie, &other_terminal);
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(applied_count(server, OTHER_TERMINAL_OPERATION_ID), 0);

    let no_terminal = update_operation(
        NO_TERMINAL_OPERATION_ID,
        TRAINING_ID,
        BENCH_SET_ID,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: Some(70.0),
        },
        vec![segment(BENCH_SEGMENT_ID, 1, 130.0, 5)],
    );
    let response = post(server, session_only_cookie, &no_terminal);
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(applied_count(server, NO_TERMINAL_OPERATION_ID), 0);

    let other_delete = delete_operation(OTHER_DELETE_OPERATION_ID, OTHER_TRAINING_ID, OTHER_SET_ID);
    let response = post(server, cookie, &other_delete);
    assert_eq!(response.status, 404, "{}", response.body);
    assert_eq!(count(server, "sets", OTHER_SET_ID), 1);
    assert_eq!(applied_count(server, OTHER_DELETE_OPERATION_ID), 0);

    let other_complete = complete_operation(
        OTHER_COMPLETE_OPERATION_ID,
        OTHER_TRAINING_ID,
        OTHER_SET_ID,
        true,
    );
    let response = post(server, cookie, &other_complete);
    assert_eq!(response.status, 404, "{}", response.body);
    assert_eq!(applied_count(server, OTHER_COMPLETE_OPERATION_ID), 0);
}

fn edit_a_finished_training_without_a_terminal(server: &DevServer, session_only_cookie: &str) {
    let create = create_operation(
        FINISHED_CREATE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        FINISHED_ROW_ID,
        FINISHED_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![segment(FINISHED_SEGMENT_ID, 1, 60.0, 5)],
    );
    let response = post(server, session_only_cookie, &create);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");

    let complete = complete_operation(
        FINISHED_COMPLETE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        FINISHED_SET_ID,
        true,
    );
    let response = post(server, session_only_cookie, &complete);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{FINISHED_SET_ID}'")
        )["completed"],
        1
    );
}

fn delete_the_set_with_its_segments(server: &DevServer, cookie: &str) {
    let delete = delete_operation(DELETE_OPERATION_ID, TRAINING_ID, SECOND_SET_ID);
    let response = post(server, cookie, &delete);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(&response), "applied");
    assert_eq!(count(server, "sets", SECOND_SET_ID), 0);
    assert_eq!(count(server, "set_segments", SECOND_SET_SEGMENT_ID), 0);
    assert_eq!(applied_count(server, DELETE_OPERATION_ID), 1);

    let replay = post(server, cookie, &delete);
    assert_eq!(replay.status, 200);
    assert_eq!(status(&replay), "already_applied");
}

#[test]
fn a_completed_set_requires_values() {
    let server = DevServer::start(SECOND_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[61u8; 32]);
    let terminal_token = base64url::encode(&[62u8; 32]);
    let other_terminal_token = base64url::encode(&[63u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let user_cookie = cookie(&token, &terminal_token);
    let set_id = "3f2504e0-4f89-41d3-9a0c-0305e82c5640";
    let segment_id = "3f2504e0-4f89-41d3-9a0c-0305e82c5641";

    let create = create_operation(
        EMPTY_CREATE_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        set_id,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![segment(segment_id, 1, 0.0, 0)],
    );
    let response = post(&server, &user_cookie, &create);
    assert_eq!(response.status, 200, "{}", response.body);

    let complete = complete_operation(EMPTY_COMPLETE_OPERATION_ID, TRAINING_ID, set_id, true);
    let response = post(&server, &user_cookie, &complete);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT completed FROM sets WHERE id = '{set_id}'")
        )["completed"],
        0
    );
    assert_eq!(applied_count(&server, EMPTY_COMPLETE_OPERATION_ID), 0);

    let empty_update = update_operation(
        EMPTY_UPDATE_OPERATION_ID,
        TRAINING_ID,
        set_id,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![segment(segment_id, 1, 0.0, 0)],
    );
    let response = post(&server, &user_cookie, &empty_update);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT completed FROM sets WHERE id = '{set_id}'")
        )["completed"],
        0,
        "an update without values must not complete the set"
    );
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT weight_kg, reps FROM set_segments WHERE id = '{segment_id}'")
        ),
        json!({"weight_kg": 0, "reps": 0})
    );

    let update = update_operation(
        EMPTY_SECOND_UPDATE_OPERATION_ID,
        TRAINING_ID,
        set_id,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![segment(segment_id, 1, 100.0, 5)],
    );
    let response = post(&server, &user_cookie, &update);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT completed FROM sets WHERE id = '{set_id}'")
        )["completed"],
        1
    );

    let rejected_update = update_operation(
        EMPTY_REJECTED_UPDATE_OPERATION_ID,
        TRAINING_ID,
        set_id,
        UpdateSpec {
            position: 1,
            kind: "normal",
            completed: true,
            body_weight_kg: None,
        },
        vec![segment(segment_id, 1, 0.0, 0)],
    );
    let response = post(&server, &user_cookie, &rejected_update);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(
        applied_count(&server, EMPTY_REJECTED_UPDATE_OPERATION_ID),
        0
    );
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT weight_kg, reps FROM set_segments WHERE id = '{segment_id}'")
        ),
        json!({"weight_kg": 100, "reps": 5})
    );

    let finish = server.request(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", &user_cookie)],
    );
    assert_eq!(finish.status, 200, "{}", finish.body);
    let summary: Value = serde_json::from_str(&finish.body).expect("a summary");
    assert_eq!(summary["set_count"], 1);
    assert_eq!(summary["total_weight_kg"], 500.0);
}

#[test]
fn a_takeover_moves_the_set_operations_to_the_new_terminal() {
    let server = DevServer::start(TAKEOVER_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[71u8; 32]);
    let terminal_token = base64url::encode(&[72u8; 32]);
    let other_terminal_token = base64url::encode(&[73u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let user_cookie = cookie(&token, &terminal_token);
    let other_cookie = cookie(&token, &other_terminal_token);

    let takeover = server.request(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/takeover"),
        &[("Origin", ORIGIN), ("Cookie", &other_cookie)],
    );
    assert_eq!(takeover.status, 200, "{}", takeover.body);

    let old = create_operation(
        TAKEOVER_OLD_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        TAKEOVER_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(TAKEOVER_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(&server, &user_cookie, &old);
    assert_eq!(response.status, 403, "{}", response.body);
    assert_eq!(applied_count(&server, TAKEOVER_OLD_OPERATION_ID), 0);
    assert_eq!(count(&server, "sets", TAKEOVER_SET_ID), 0);

    let new = create_operation(
        TAKEOVER_NEW_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        TAKEOVER_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: Some(70.0),
        },
        vec![segment(TAKEOVER_SEGMENT_ID, 1, 100.0, 5)],
    );
    let response = post(&server, &other_cookie, &new);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(applied_count(&server, TAKEOVER_NEW_OPERATION_ID), 1);
    assert_eq!(count(&server, "sets", TAKEOVER_SET_ID), 1);
}
