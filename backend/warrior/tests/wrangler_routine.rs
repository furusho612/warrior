//! ルーティンの保存、編集、複製、削除、開始の結合テスト。
//!
//! 対応する要求: `REQ-RTN-001`、`REQ-RTN-003`、`REQ-RTN-004`。
//! 端末の送信待ちの列と同じ同期の操作でルーティンを保存し、一覧を読み、開始すると
//! 種目とセットがルーティンの目標から作られることを確かめる。別の利用者のルーティンを
//! 編集、削除、複製しようとすると 404 になり、削除しても開始済みのトレーニングは残る。

mod support;

use serde_json::{json, Value};
use support::record_common::{
    applied_count, cookie, count, count_with, first_row, post_operation, seed_auth_session,
    seed_terminal, seed_user, status,
};
use support::{DevServer, HttpResponse};
use warrior_core::base64url;

const PORT: u16 = 8862;
const ORIGIN: &str = "http://localhost:8787";

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6101";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6102";
const ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6110";
const COPIED_ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6111";
const BENCH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6120";
const PLANK_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6121";
const COPIED_BENCH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6122";
const COPIED_PLANK_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6123";
const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const PLANK_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005d";
const CREATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6130";
const UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6131";
const COPY_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6132";
const DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6133";
const FOREIGN_UPDATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6134";
const FOREIGN_COPY_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6135";
const FOREIGN_DELETE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6136";

#[test]
fn routines_are_saved_edited_copied_deleted_and_started_for_their_owner() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let (session_a, terminal_a) = seed_account(&server, USER_ID, "routine-a");
    let (session_b, terminal_b) = seed_account(&server, OTHER_USER_ID, "routine-b");
    let cookie_a = cookie(&session_a, &terminal_a);
    let cookie_b = cookie(&session_b, &terminal_b);

    assert_applied(&post_operation(
        &server,
        &cookie_a,
        &routine_create(),
        ORIGIN,
    ));
    assert_eq!(count(&server, "routines", ROUTINE_ID), 1);
    assert_eq!(count(&server, "routine_exercises", BENCH_ROW_ID), 1);
    assert_eq!(count(&server, "routine_exercises", PLANK_ROW_ID), 1);
    assert_eq!(
        count_with(
            &server,
            &format!(
                "SELECT COUNT(*) AS count FROM routine_exercise_targets \
                 WHERE routine_exercise_id = '{BENCH_ROW_ID}'"
            )
        ),
        1
    );
    let listed = get_routines(&server, &cookie_a);
    assert_eq!(routine_names(&listed), vec!["胸の日".to_owned()]);
    let bench = &listed["routines"][0]["exercises"][0];
    assert_eq!(bench["exercise_id"], BENCH_EXERCISE_ID);
    assert_eq!(bench["position"], 1);
    assert_eq!(bench["target"]["set_count"], 3);
    assert_eq!(bench["target"]["weight_kg"], 100.0);
    assert_eq!(bench["target"]["reps"], 5);
    assert_eq!(bench["target"]["rest_seconds"], 120);
    let plank = &listed["routines"][0]["exercises"][1];
    assert_eq!(plank["exercise_id"], PLANK_EXERCISE_ID);
    assert_eq!(plank["position"], 2);
    assert_eq!(plank["target"]["set_count"], 2);
    assert_eq!(plank["target"]["duration_seconds"], 90);
    assert_eq!(
        routine_names(&get_routines(&server, &cookie_b)),
        Vec::<String>::new(),
        "another user must not see the routines"
    );

    server
        .execute_d1_json(&format!(
            "UPDATE master_exercises SET name = 'ベンチプレス改' WHERE id = '{BENCH_EXERCISE_ID}'"
        ))
        .expect("the master exercise must be updated");
    let listed = get_routines(&server, &cookie_a);
    assert_eq!(
        listed["routines"][0]["exercises"][0]["snapshot"]["name"], "ベンチプレス",
        "the saved routine keeps its snapshot"
    );

    let first = start(&server, &cookie_a, ROUTINE_ID);
    assert_eq!(first["session"]["source_kind"], "routine");
    assert_eq!(first["session"]["routine_id"], ROUTINE_ID);
    assert_eq!(first["exercises"].as_array().map(Vec::len), Some(2));
    assert_eq!(first["exercises"][0]["exercise_id"], BENCH_EXERCISE_ID);
    assert_eq!(first["exercises"][0]["position"], 1);
    let started_snapshot: Value = serde_json::from_str(
        first["exercises"][0]["snapshot"]
            .as_str()
            .expect("the stored snapshot"),
    )
    .expect("the snapshot is JSON");
    assert_eq!(
        started_snapshot["name"], "ベンチプレス改",
        "the start uses the current master snapshot"
    );
    assert_eq!(first["exercises"][1]["exercise_id"], PLANK_EXERCISE_ID);
    assert_eq!(first["exercises"][1]["position"], 2);
    assert_eq!(first["sets"].as_array().map(Vec::len), Some(5));
    let bench_sets = first["sets"]
        .as_array()
        .expect("the sets")
        .iter()
        .filter(|set| set["training_exercise_id"] == first["exercises"][0]["id"])
        .collect::<Vec<_>>();
    assert_eq!(bench_sets.len(), 3);
    let bench_set_ids = bench_sets
        .iter()
        .map(|set| set["id"].as_str().expect("a bench set id"))
        .collect::<Vec<_>>();
    let first_segment = first["segments"]
        .as_array()
        .expect("the segments")
        .iter()
        .find(|segment| {
            segment["set_id"]
                .as_str()
                .is_some_and(|set_id| bench_set_ids.contains(&set_id))
        })
        .expect("a bench segment");
    assert_eq!(first_segment["weight_kg"], 100.0);
    assert_eq!(first_segment["reps"], 5);
    let plank_sets = first["sets"]
        .as_array()
        .expect("the sets")
        .iter()
        .filter(|set| set["training_exercise_id"] == first["exercises"][1]["id"])
        .collect::<Vec<_>>();
    assert_eq!(plank_sets.len(), 2);
    let first_training_id = first["session"]["id"]
        .as_str()
        .expect("the training id")
        .to_owned();
    assert_eq!(
        first_row(
            &server,
            &format!(
                "SELECT owner_terminal_id FROM training_sessions WHERE id = '{first_training_id}'"
            )
        )["owner_terminal_id"],
        json!(terminal_a)
    );

    let second = start(&server, &cookie_a, ROUTINE_ID);
    let second_training_id = second["session"]["id"]
        .as_str()
        .expect("the second training id")
        .to_owned();
    assert_ne!(first_training_id, second_training_id);
    assert_ne!(
        first["exercises"][0]["id"], second["exercises"][0]["id"],
        "each start creates its own exercise rows"
    );
    server
        .execute_d1_json(&format!(
            "UPDATE set_segments SET weight_kg = 999 WHERE set_id IN \
             (SELECT id FROM sets WHERE training_exercise_id = '{}')",
            first["exercises"][0]["id"]
                .as_str()
                .expect("the first row id")
        ))
        .expect("the first training must be edited");
    let second_segment = second["segments"]
        .as_array()
        .expect("the second segments")
        .iter()
        .find(|segment| segment["weight_kg"] == 100.0)
        .expect("the second training must keep the routine target");
    assert_eq!(second_segment["reps"], 5);

    assert_applied(&post_operation(
        &server,
        &cookie_a,
        &routine_update(),
        ORIGIN,
    ));
    assert_eq!(
        first_row(
            &server,
            &format!("SELECT name FROM routines WHERE id = '{ROUTINE_ID}'")
        )["name"],
        "胸と体幹"
    );
    assert_eq!(
        first_row(
            &server,
            &format!(
                "SELECT target_reps FROM routine_exercise_targets \
                 WHERE routine_exercise_id = '{BENCH_ROW_ID}'"
            )
        )["target_reps"],
        8
    );

    reject_other_users_operations(&server, &cookie_b);
    assert_applied(&post_operation(&server, &cookie_a, &routine_copy(), ORIGIN));
    assert_eq!(count(&server, "routines", COPIED_ROUTINE_ID), 1);
    assert_eq!(
        first_row(
            &server,
            &format!(
                "SELECT set_count FROM routine_exercise_targets \
                 WHERE routine_exercise_id = '{COPIED_BENCH_ROW_ID}'"
            )
        )["set_count"],
        4
    );
    assert_eq!(routine_names(&get_routines(&server, &cookie_a)).len(), 2);

    assert_applied(&post_operation(
        &server,
        &cookie_a,
        &routine_delete(),
        ORIGIN,
    ));
    assert_eq!(count(&server, "routines", ROUTINE_ID), 0);
    assert_eq!(count(&server, "routine_exercises", BENCH_ROW_ID), 0);
    assert_eq!(count(&server, "routines", COPIED_ROUTINE_ID), 1);
    for training_id in [&first_training_id, &second_training_id] {
        let row = first_row(
            &server,
            &format!(
                "SELECT routine_id, source_kind FROM training_sessions WHERE id = '{training_id}'"
            ),
        );
        assert_eq!(row["routine_id"], Value::Null);
        assert_eq!(row["source_kind"], "routine");
        assert_eq!(count(&server, "training_sessions", training_id), 1);
    }

    let replay = post_operation(&server, &cookie_a, &routine_create(), ORIGIN);
    assert_eq!(status(&replay), "already_applied");
    assert_eq!(count(&server, "routines", ROUTINE_ID), 0);
}

fn reject_other_users_operations(server: &DevServer, cookie: &str) {
    for (operation, operation_id) in [
        (routine_update(), FOREIGN_UPDATE_OPERATION_ID),
        (routine_copy(), FOREIGN_COPY_OPERATION_ID),
        (routine_delete(), FOREIGN_DELETE_OPERATION_ID),
    ] {
        let response = post_operation(server, cookie, &operation, ORIGIN);
        assert_eq!(response.status, 404, "{}", response.body);
        assert_eq!(applied_count(server, operation_id), 0);
    }
    assert_eq!(count(server, "routines", ROUTINE_ID), 1);
    assert_eq!(count(server, "routines", COPIED_ROUTINE_ID), 0);
}

fn seed_account(server: &DevServer, user_id: &str, sub: &str) -> (String, String) {
    let first = user_id == USER_ID;
    let session = base64url::encode(&[if first { 91u8 } else { 93u8 }; 32]);
    let terminal = base64url::encode(&[if first { 92u8 } else { 94u8 }; 32]);
    seed_user(server, user_id, sub, &format!("{sub}@example.test"), sub);
    seed_auth_session(
        server,
        if first {
            "3f2504e0-4f89-41d3-9a0c-0305e82c6140"
        } else {
            "3f2504e0-4f89-41d3-9a0c-0305e82c6141"
        },
        user_id,
        &session,
    );
    seed_terminal(
        server,
        if first {
            "3f2504e0-4f89-41d3-9a0c-0305e82c6150"
        } else {
            "3f2504e0-4f89-41d3-9a0c-0305e82c6151"
        },
        user_id,
        &terminal,
    );
    (session, terminal)
}

fn get_routines(server: &DevServer, cookie: &str) -> Value {
    let response = server.request("GET", "/api/routines", &[("Cookie", cookie)]);
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the routines response is JSON")
}

fn routine_names(body: &Value) -> Vec<String> {
    body["routines"]
        .as_array()
        .expect("the routines")
        .iter()
        .map(|routine| routine["name"].as_str().expect("a routine name").to_owned())
        .collect()
}

fn start(server: &DevServer, cookie: &str, routine_id: &str) -> Value {
    let response = server.request_with_body(
        "POST",
        &format!("/api/routines/{routine_id}/start"),
        &[
            ("Origin", ORIGIN),
            ("Cookie", cookie),
            ("Content-Type", "application/json"),
        ],
        "",
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the training record is JSON")
}

fn assert_applied(response: &HttpResponse) {
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(status(response), "applied");
}

fn routine_create() -> Value {
    json!({
        "operation_id": CREATE_OPERATION_ID,
        "training_id": ROUTINE_ID,
        "sequence": 1,
        "kind": "routine_create",
        "parent_operation_id": null,
        "payload": {
            "routine_id": ROUTINE_ID,
            "name": "胸の日",
            "position": 1,
            "exercises": [
                exercise(
                    BENCH_ROW_ID,
                    BENCH_EXERCISE_ID,
                    "ベンチプレス",
                    "chest",
                    "pectoralis_major_middle",
                    "weight_and_reps",
                    1,
                    3,
                    Some(100.0),
                    Some(5),
                    None,
                    120,
                ),
                exercise(
                    PLANK_ROW_ID,
                    PLANK_EXERCISE_ID,
                    "プランク",
                    "abs",
                    "rectus_abdominis_upper",
                    "time",
                    2,
                    2,
                    None,
                    None,
                    Some(90),
                    60,
                ),
            ],
        },
    })
}

fn routine_update() -> Value {
    json!({
        "operation_id": UPDATE_OPERATION_ID,
        "training_id": ROUTINE_ID,
        "sequence": 2,
        "kind": "routine_update",
        "parent_operation_id": null,
        "payload": {
            "routine_id": ROUTINE_ID,
            "name": "胸と体幹",
            "position": 1,
            "exercises": [
                exercise(
                    BENCH_ROW_ID,
                    BENCH_EXERCISE_ID,
                    "ベンチプレス",
                    "chest",
                    "pectoralis_major_middle",
                    "weight_and_reps",
                    1,
                    4,
                    Some(105.0),
                    Some(8),
                    None,
                    90,
                ),
                exercise(
                    PLANK_ROW_ID,
                    PLANK_EXERCISE_ID,
                    "プランク",
                    "abs",
                    "rectus_abdominis_upper",
                    "time",
                    2,
                    3,
                    None,
                    None,
                    Some(120),
                    60,
                ),
            ],
        },
    })
}

fn routine_copy() -> Value {
    json!({
        "operation_id": COPY_OPERATION_ID,
        "training_id": COPIED_ROUTINE_ID,
        "sequence": 1,
        "kind": "routine_copy",
        "parent_operation_id": null,
        "payload": {
            "routine_id": ROUTINE_ID,
            "new_routine_id": COPIED_ROUTINE_ID,
            "name": "胸と体幹",
            "position": 2,
            "exercises": [
                exercise(
                    COPIED_BENCH_ROW_ID,
                    BENCH_EXERCISE_ID,
                    "ベンチプレス",
                    "chest",
                    "pectoralis_major_middle",
                    "weight_and_reps",
                    1,
                    4,
                    Some(105.0),
                    Some(8),
                    None,
                    90,
                ),
                exercise(
                    COPIED_PLANK_ROW_ID,
                    PLANK_EXERCISE_ID,
                    "プランク",
                    "abs",
                    "rectus_abdominis_upper",
                    "time",
                    2,
                    3,
                    None,
                    None,
                    Some(120),
                    60,
                ),
            ],
        },
    })
}

fn routine_delete() -> Value {
    json!({
        "operation_id": DELETE_OPERATION_ID,
        "training_id": ROUTINE_ID,
        "sequence": 3,
        "kind": "routine_delete",
        "parent_operation_id": null,
        "payload": {
            "routine_id": ROUTINE_ID,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn exercise(
    row_id: &str,
    exercise_id: &str,
    name: &str,
    body_part: &str,
    muscle: &str,
    record_mode: &str,
    position: i64,
    set_count: i64,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
    rest_seconds: i64,
) -> Value {
    json!({
        "routine_exercise_id": row_id,
        "exercise_id": exercise_id,
        "snapshot": {
            "exercise_id": exercise_id,
            "name": name,
            "body_part": body_part,
            "muscles": [{"muscle": muscle, "role": "primary"}],
            "record_mode": record_mode,
            "laterality": "bilateral",
            "grip_mode": "none",
            "grip_options": [],
        },
        "position": position,
        "target": {
            "set_count": set_count,
            "weight_kg": weight_kg,
            "reps": reps,
            "duration_seconds": duration_seconds,
            "grip": null,
            "rest_seconds": rest_seconds,
        },
    })
}
