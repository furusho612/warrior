//! セットの完了の応答が返す自己ベストと、`personal_bests` の表の結合テスト。
//!
//! 対応する要求: `REQ-SET-009`、`REQ-SET-011`。
//! 最大重量と推定 1RM はドロップセットの最初の段、種目別の総重量は全段を使うこと、
//! ウォームアップを対象にしないこと、グリップとアタッチメントで比較の単位を分けること、
//! 左右別の時間を左右の最大値にすること、同じ種類を複数回更新したら最後の値が残ることを
//! 確かめる。

#[allow(dead_code)]
#[path = "support/record_set.rs"]
mod helpers;
mod support;

use helpers::*;
use serde_json::{json, Value};
use support::record_common;
use support::DevServer;
use warrior_core::base64url;
use warrior_core::set_calc::estimated_one_rep_max;

const MAIN_PORT: u16 = 8843;
const GRIP_PORT: u16 = 8844;
const TIME_PORT: u16 = 8845;

const WARMUP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5800";
const WARMUP_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5801";
const FIRST_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5802";
const FIRST_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5803";
const SECOND_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5804";
const SECOND_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5805";
const DROP_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5806";
const DROP_FIRST_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5807";
const DROP_SECOND_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5808";

const WARMUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5810";
const FIRST_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5811";
const SECOND_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5812";
const DROP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5813";
const WARMUP_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5814";
const FIRST_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5815";
const SECOND_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5816";
const DROP_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5817";

const GRIP_FIRST_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5820";
const GRIP_FIRST_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5821";
const GRIP_SECOND_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5822";
const GRIP_SECOND_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5823";
const GRIP_FIRST_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5830";
const GRIP_SECOND_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5831";
const GRIP_FIRST_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5832";
const GRIP_SECOND_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5833";

const TIME_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5840";
const TIME_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5841";
const TIME_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5842";
const TIME_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5843";
const TIME_SECOND_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5844";
const TIME_SECOND_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5845";
const TIME_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5850";
const TIME_SECOND_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5851";
const TIME_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5852";
const TIME_SECOND_COMPLETE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5853";

#[test]
fn the_set_completion_response_reports_the_updated_personal_bests() {
    let server = start_server(MAIN_PORT);
    let cookie = seed(&server);

    create_set(
        &server,
        &cookie,
        WARMUP_OPERATION_ID,
        WARMUP_SET_ID,
        1,
        "warmup",
        vec![segment(WARMUP_SEGMENT_ID, 1, 90.0, 10)],
    );
    create_set(
        &server,
        &cookie,
        FIRST_OPERATION_ID,
        FIRST_SET_ID,
        2,
        "normal",
        vec![segment(FIRST_SEGMENT_ID, 1, 100.0, 5)],
    );
    create_set(
        &server,
        &cookie,
        SECOND_OPERATION_ID,
        SECOND_SET_ID,
        3,
        "normal",
        vec![segment(SECOND_SEGMENT_ID, 1, 80.0, 10)],
    );
    create_set(
        &server,
        &cookie,
        DROP_OPERATION_ID,
        DROP_SET_ID,
        4,
        "drop",
        vec![
            segment(DROP_FIRST_SEGMENT_ID, 1, 120.0, 5),
            segment(DROP_SECOND_SEGMENT_ID, 2, 100.0, 8),
        ],
    );

    let warmup = complete(&server, &cookie, WARMUP_COMPLETE_ID, WARMUP_SET_ID);
    assert!(bests(&warmup).is_empty());
    assert_eq!(personal_best_count(&server, TRAINING_ID), 0);

    let first = complete(&server, &cookie, FIRST_COMPLETE_ID, FIRST_SET_ID);
    let first_bests = bests(&first);
    assert_eq!(first_bests.len(), 3);
    assert_best(&first_bests[0], "max_weight", 100.0);
    assert_best(
        &first_bests[1],
        "estimated_one_rep_max",
        estimated_one_rep_max(100.0, 5).expect("the formula"),
    );
    assert_best(&first_bests[2], "total_weight", 500.0);
    assert_eq!(personal_best_count(&server, TRAINING_ID), 3);

    let second = complete(&server, &cookie, SECOND_COMPLETE_ID, SECOND_SET_ID);
    let second_bests = bests(&second);
    assert_eq!(second_bests.len(), 1);
    assert_best(&second_bests[0], "total_weight", 1_300.0);
    assert_eq!(personal_best_count(&server, TRAINING_ID), 3);

    let drop = complete(&server, &cookie, DROP_COMPLETE_ID, DROP_SET_ID);
    let drop_bests = bests(&drop);
    assert_eq!(drop_bests.len(), 3);
    assert_best(&drop_bests[0], "max_weight", 120.0);
    assert_best(
        &drop_bests[1],
        "estimated_one_rep_max",
        estimated_one_rep_max(120.0, 5).expect("the formula"),
    );
    assert_best(&drop_bests[2], "total_weight", 2_700.0);

    let rows = personal_bests(&server, TRAINING_ID);
    assert_eq!(rows.len(), 3);
    let max_weight = rows
        .iter()
        .find(|row| row["kind"] == "max_weight")
        .expect("the max weight row");
    assert_eq!(max_weight["value"].as_f64(), Some(120.0));
    assert_eq!(max_weight["training_id"], TRAINING_ID);
    assert_eq!(max_weight["achieved_on"], "2026-11-12");
    let total = rows
        .iter()
        .find(|row| row["kind"] == "total_weight")
        .expect("the total weight row");
    assert_eq!(total["value"].as_f64(), Some(2_700.0));

    let summary = finish(&server, &cookie);
    let summary_bests = summary["personal_bests"]
        .as_array()
        .expect("the summary bests")
        .clone();
    assert_eq!(summary_bests.len(), 3);
    assert_eq!(summary_bests, bests(&drop));
}

#[test]
fn grip_and_attachment_split_the_personal_bests() {
    let server = start_server(GRIP_PORT);
    let cookie = seed(&server);

    let mut first = create_operation(
        GRIP_FIRST_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        GRIP_FIRST_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![segment(GRIP_FIRST_SEGMENT_ID, 1, 100.0, 5)],
    );
    first["payload"]["grip"] = json!({"grip": {"style": "pronated", "width": "medium"}});
    assert_applied(&post(&server, &cookie, &first));

    let mut second = create_operation(
        GRIP_SECOND_OPERATION_ID,
        TRAINING_ID,
        BENCH_ROW_ID,
        GRIP_SECOND_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![segment(GRIP_SECOND_SEGMENT_ID, 1, 90.0, 5)],
    );
    second["payload"]["grip"] = json!({"attachment": {"attachment": "wide_bar", "style": null}});
    assert_applied(&post(&server, &cookie, &second));

    let first_bests = bests(&complete(
        &server,
        &cookie,
        GRIP_FIRST_COMPLETE_ID,
        GRIP_FIRST_SET_ID,
    ));
    assert_eq!(first_bests.len(), 3);
    assert_eq!(first_bests[0]["grip"], "pronated_medium");
    assert!(first_bests[0]["attachment"].is_null());

    let second_bests = bests(&complete(
        &server,
        &cookie,
        GRIP_SECOND_COMPLETE_ID,
        GRIP_SECOND_SET_ID,
    ));
    assert_eq!(second_bests.len(), 3);
    assert!(second_bests[0]["grip"].is_null());
    assert_eq!(second_bests[0]["attachment"], "wide_bar");

    let rows = personal_bests(&server, TRAINING_ID);
    assert_eq!(rows.len(), 6);
    let max_weights = rows
        .iter()
        .filter(|row| row["kind"] == "max_weight")
        .collect::<Vec<_>>();
    assert_eq!(max_weights.len(), 2);
    let pronated = max_weights
        .iter()
        .find(|row| row["grip"] == "pronated_medium")
        .expect("the pronated grip row");
    assert_eq!(pronated["attachment"], "");
    let attached = max_weights
        .iter()
        .find(|row| row["attachment"] == "wide_bar")
        .expect("the attachment row");
    assert_eq!(attached["grip"], "");
}

#[test]
fn a_unilateral_time_set_uses_the_longest_side() {
    let server = start_server(TIME_PORT);
    let cookie = seed(&server);
    record_common::seed_exercise_with_laterality(
        &server,
        record_common::ExerciseSeed {
            row_id: TIME_ROW_ID,
            user_id: USER_ID,
            training_id: TRAINING_ID,
            exercise_id: TIME_EXERCISE_ID,
            name: "サイドプランク",
            body_part: "abs",
            muscle: "obliques",
            record_mode: "time",
            position: 3,
        },
        "unilateral",
    );

    let first = create_operation(
        TIME_OPERATION_ID,
        TRAINING_ID,
        TIME_ROW_ID,
        TIME_SET_ID,
        SetSpec {
            position: 1,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![side_duration_segment(TIME_SEGMENT_ID, 1, 30, 40)],
    );
    assert_applied(&post(&server, &cookie, &first));
    let first_bests = bests(&complete(&server, &cookie, TIME_COMPLETE_ID, TIME_SET_ID));
    assert_eq!(first_bests.len(), 1);
    assert_best(&first_bests[0], "longest_duration", 40.0);
    assert_eq!(personal_best_count(&server, TRAINING_ID), 1);

    let second = create_operation(
        TIME_SECOND_OPERATION_ID,
        TRAINING_ID,
        TIME_ROW_ID,
        TIME_SECOND_SET_ID,
        SetSpec {
            position: 2,
            kind: "normal",
            body_weight_kg: None,
        },
        vec![side_duration_segment(TIME_SECOND_SEGMENT_ID, 1, 50, 20)],
    );
    assert_applied(&post(&server, &cookie, &second));
    let second_bests = bests(&complete(
        &server,
        &cookie,
        TIME_SECOND_COMPLETE_ID,
        TIME_SECOND_SET_ID,
    ));
    assert_eq!(second_bests.len(), 1);
    assert_best(&second_bests[0], "longest_duration", 50.0);
    assert_eq!(personal_best_count(&server, TRAINING_ID), 1);
    assert_eq!(
        personal_bests(&server, TRAINING_ID)[0]["value"].as_f64(),
        Some(50.0)
    );
    assert_eq!(
        personal_bests(&server, TRAINING_ID)[0]["kind"],
        "longest_duration"
    );
}

fn start_server(port: u16) -> DevServer {
    let server = DevServer::start(port);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    server
}

fn seed(server: &DevServer) -> String {
    let token = base64url::encode(&[121u8; 32]);
    let terminal_token = base64url::encode(&[122u8; 32]);
    let other_terminal_token = base64url::encode(&[123u8; 32]);
    seed_base(server, &token, &terminal_token, &other_terminal_token);
    cookie(&token, &terminal_token)
}

fn create_set(
    server: &DevServer,
    cookie: &str,
    operation_id: &str,
    set_id: &str,
    position: i64,
    kind: &str,
    segments: Vec<Value>,
) {
    let operation = create_operation(
        operation_id,
        TRAINING_ID,
        BENCH_ROW_ID,
        set_id,
        SetSpec {
            position,
            kind,
            body_weight_kg: None,
        },
        segments,
    );
    let response = post(server, cookie, &operation);
    assert_applied(&response);
    assert_eq!(applied_count(server, operation_id), 1);
    assert_eq!(count(server, "sets", set_id), 1);
}

fn assert_applied(response: &support::HttpResponse) {
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(response), "applied");
}

fn complete(server: &DevServer, cookie: &str, operation_id: &str, set_id: &str) -> Value {
    let operation = complete_operation(operation_id, TRAINING_ID, set_id, true);
    let response = post(server, cookie, &operation);
    assert_applied(&response);
    assert_eq!(applied_count(server, operation_id), 1);
    let completed = first_row(
        server,
        &format!("SELECT completed FROM sets WHERE id = '{set_id}'"),
    );
    assert_eq!(completed["completed"], 1);
    serde_json::from_str(&response.body).expect("the completion response is JSON")
}

fn bests(response: &Value) -> Vec<Value> {
    response
        .get("personal_bests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn assert_best(item: &Value, kind: &str, expected: f64) {
    assert_eq!(item["kind"], kind);
    let value = item["value"].as_f64().expect("a value");
    assert!(
        (value - expected).abs() < 1e-9,
        "{kind} must be {expected} but was {value}: {item:?}"
    );
}

fn personal_bests(server: &DevServer, training_id: &str) -> Vec<Value> {
    server
        .execute_d1_json(&format!(
            "SELECT exercise_id, grip, attachment, kind, value, achieved_on, training_id \
             FROM personal_bests WHERE user_id = '{USER_ID}' AND training_id = '{training_id}' \
             ORDER BY kind, exercise_id"
        ))
        .expect("the personal bests must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .expect("the personal best rows")
}

fn personal_best_count(server: &DevServer, training_id: &str) -> i64 {
    count_with(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM personal_bests \
             WHERE user_id = '{USER_ID}' AND training_id = '{training_id}'"
        ),
    )
}

fn finish(server: &DevServer, cookie: &str) -> Value {
    let response = server.request(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the summary is JSON")
}

fn side_duration_segment(
    segment_id: &str,
    position: i64,
    left_duration_seconds: i64,
    right_duration_seconds: i64,
) -> Value {
    let mut segment = segment(segment_id, position, 0.0, 0);
    segment["duration_seconds"] = Value::Null;
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
