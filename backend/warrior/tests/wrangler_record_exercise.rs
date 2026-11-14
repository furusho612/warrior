//! 記録の種目の追加、並べ替え、削除と、前回の取得の結合テスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-REC-003`、`REQ-REC-004`。
//! 種目の操作は同期の操作の適用で行い、D1 の行を確かめる。削除はセットと段も消す。
//! 前回の取得は、種目、グリップとアタッチメント、ルーティン、プログラム、空の状態の
//! 規則に従うことを確かめる。

mod support;

use serde_json::{json, Value};
use support::DevServer;
use warrior_core::auth::{hash_session_token, hash_terminal_token};
use warrior_core::base64url;

const PORT: u16 = 8816;
const PREVIOUS_PORT: u16 = 8817;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4901";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4902";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4910";
const OTHER_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4911";
const FINISHED_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4912";
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
    for sql in [
        seed_set_sql(BENCH_ROW_ID, BENCH_SET_ID, 1, "normal", None),
        seed_segment_sql(
            BENCH_SET_ID,
            BENCH_SEGMENT_ID,
            Some(100.0),
            Some(5),
            None,
        ),
        seed_set_sql(PLANK_ROW_ID, PLANK_SET_ID, 1, "normal", None),
        seed_segment_sql(PLANK_SET_ID, PLANK_SEGMENT_ID, None, None, Some(90)),
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
            .expect("the sets must be seeded");
    }
}

fn delete_the_exercise_with_its_sets(server: &DevServer, cookie: &str) {
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
    assert_eq!(body["exercise"]["sets"][0]["grip"], GRIP_JSON);
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
    assert_eq!(missing_exercise.status, 400);
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
const PAST_EMPTY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4980";
const PAST_EMPTY_LATEST_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4981";
const PAST_GRIP_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4982";
const PAST_PROGRAM_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4983";
const PAST_ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4984";
const PAST_CUSTOM_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4985";
const PAST_PROGRAM_DAY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4986";
const PAST_CUSTOM_DAY_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4987";
const PAST_ROUTINE_ID_VALUE: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4988";

fn seed_previous_sessions(server: &DevServer) {
    seed_current_session(
        server,
        PROGRAM_TRAINING_ID,
        "program",
        None,
        Some(PAST_PROGRAM_DAY_ID),
        None,
    );
    seed_current_session(
        server,
        CUSTOM_TRAINING_ID,
        "custom_program",
        None,
        None,
        Some(PAST_CUSTOM_DAY_ID),
    );
    seed_current_session(
        server,
        ROUTINE_TRAINING_ID,
        "routine",
        Some(PAST_ROUTINE_ID_VALUE),
        None,
        None,
    );

    seed_past_training(
        server,
        PastTraining {
            training_id: PAST_EMPTY_ID,
            source_kind: "empty",
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
            finished_at: "2026-01-01T10:00:00Z",
            total_weight_kg: 800.0,
        },
        PastSets::Bench(700.0, None, 1),
    );
    seed_past_training(
        server,
        PastTraining {
            training_id: PAST_EMPTY_LATEST_ID,
            source_kind: "empty",
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
            finished_at: "2026-02-15T10:00:00Z",
            total_weight_kg: 950.0,
        },
        PastSets::Bench(900.0, None, 2),
    );
    seed_past_training(
        server,
        PastTraining {
            training_id: PAST_GRIP_ID,
            source_kind: "empty",
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
            finished_at: "2026-02-01T10:00:00Z",
            total_weight_kg: 500.0,
        },
        PastSets::Bench(500.0, Some(GRIP_JSON), 1),
    );
    seed_past_training(
        server,
        PastTraining {
            training_id: PAST_PROGRAM_ID,
            source_kind: "program",
            routine_id: None,
            program_day_id: Some(PAST_PROGRAM_DAY_ID),
            custom_program_day_id: None,
            finished_at: "2026-03-01T10:00:00Z",
            total_weight_kg: 700.0,
        },
        PastSets::Plank(60, 60),
    );
    seed_past_training(
        server,
        PastTraining {
            training_id: PAST_ROUTINE_ID,
            source_kind: "routine",
            routine_id: Some(PAST_ROUTINE_ID_VALUE),
            program_day_id: None,
            custom_program_day_id: None,
            finished_at: "2026-04-01T10:00:00Z",
            total_weight_kg: 800.0,
        },
        PastSets::Plank(90, 90),
    );
    seed_past_training(
        server,
        PastTraining {
            training_id: PAST_CUSTOM_ID,
            source_kind: "custom_program",
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: Some(PAST_CUSTOM_DAY_ID),
            finished_at: "2026-05-01T10:00:00Z",
            total_weight_kg: 600.0,
        },
        PastSets::Plank(30, 30),
    );
}

enum PastSets {
    Bench(f64, Option<&'static str>, i64),
    Plank(i64, i64),
}

struct PastTraining<'a> {
    training_id: &'a str,
    source_kind: &'a str,
    routine_id: Option<&'a str>,
    program_day_id: Option<&'a str>,
    custom_program_day_id: Option<&'a str>,
    finished_at: &'a str,
    total_weight_kg: f64,
}

fn seed_past_training(server: &DevServer, training: PastTraining<'_>, sets: PastSets) {
    let training_id = training.training_id;
    let source_kind = training.source_kind;
    let finished_at = training.finished_at;
    let total_weight_kg = training.total_weight_kg;
    let (exercise_id, snapshot, set_id, segment_id) = match &sets {
        PastSets::Bench(_, _, _) => (
            BENCH_EXERCISE_ID,
            bench_snapshot(),
            format!("{training_id}-set-bench"),
            format!("{training_id}-segment-bench"),
        ),
        PastSets::Plank(_, _) => (
            PLANK_EXERCISE_ID,
            plank_snapshot(),
            format!("{training_id}-set-plank"),
            format!("{training_id}-segment-plank"),
        ),
    };
    let row_id = format!("{training_id}-exercise");
    let routine = sql_text(training.routine_id);
    let program = sql_text(training.program_day_id);
    let custom = sql_text(training.custom_program_day_id);
    for sql in [
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, routine_id, program_day_id, \
              custom_program_day_id, finished_at, total_weight_kg, created_at, updated_at) \
             VALUES ('{training_id}', '{USER_ID}', '2026-01-01', '2026-01-01T03:00:00Z', '', \
              '{source_kind}', {routine}, {program}, {custom}, '{finished_at}', {total_weight_kg}, \
              '2026-01-01T03:00:00Z', '{finished_at}')"
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{row_id}', '{USER_ID}', '{training_id}', '{exercise_id}', '{}', 1, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')",
            json_literal(&snapshot)
        ),
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{set_id}', '{USER_ID}', '{row_id}', 1, 'normal', 1, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the past training must be seeded");
    }
    match sets {
        PastSets::Bench(total_weight, grip, count) => {
            for index in 0..count {
                let current_set = if index == 0 {
                    set_id.clone()
                } else {
                    let extra_set = format!("{set_id}-{index}");
                    seed_extra_set(server, &row_id, &extra_set, index + 1);
                    extra_set
                };
                let weight = total_weight / f64::from(count as u32 * 5);
                seed_set_segment(
                    server,
                    &current_set,
                    &format!("{segment_id}-{index}"),
                    1,
                    Some(weight),
                    Some(5),
                    None,
                );
            }
            if let Some(grip) = grip {
                server
                    .execute_d1_json(&format!(
                        "UPDATE sets SET grip = '{}' WHERE id = '{set_id}'",
                        grip.replace('\'', "''")
                    ))
                    .expect("the grip must be stored");
            }
        }
        PastSets::Plank(total, longest) => {
            seed_set_segment(server, &set_id, &segment_id, 1, None, None, Some(longest));
            let set_count = total / longest;
            for index in 1..set_count {
                let extra_set = format!("{set_id}-{index}");
                seed_extra_set(server, &row_id, &extra_set, index + 1);
                seed_set_segment(
                    server,
                    &extra_set,
                    &format!("{segment_id}-{index}"),
                    1,
                    None,
                    None,
                    Some(longest),
                );
            }
        }
    }
}

fn seed_current_session(
    server: &DevServer,
    training_id: &str,
    source_kind: &str,
    routine_id: Option<&str>,
    program_day_id: Option<&str>,
    custom_program_day_id: Option<&str>,
) {
    let routine = sql_text(routine_id);
    let program = sql_text(program_day_id);
    let custom = sql_text(custom_program_day_id);
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, routine_id, program_day_id, \
              custom_program_day_id, owner_terminal_id, created_at, updated_at) \
             VALUES ('{training_id}', '{USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '', \
              '{source_kind}', {routine}, {program}, {custom}, '{TERMINAL_ID}', \
              '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ))
        .expect("the current session must be seeded");
}

fn seed_extra_set(server: &DevServer, row_id: &str, set_id: &str, position: i64) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{set_id}', '{USER_ID}', '{row_id}', {position}, 'normal', 1, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
        ))
        .expect("the extra set must be seeded");
}

fn seed_set_segment(
    server: &DevServer,
    set_id: &str,
    segment_id: &str,
    position: i64,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              duration_seconds, created_at, updated_at) \
             VALUES ('{segment_id}', '{USER_ID}', '{set_id}', {position}, {}, {}, 0, 0, {}, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')",
            sql_number(weight_kg),
            sql_number(reps),
            sql_number(duration_seconds)
        ))
        .expect("the segment must be seeded");
}

fn assert_previous_exercise(body: &Value, total_weight_kg: f64, set_count: usize) {
    assert_eq!(body["exercise"]["total_weight_kg"], total_weight_kg);
    assert_eq!(
        body["exercise"]["sets"].as_array().map(Vec::len),
        Some(set_count)
    );
}

fn assert_previous_time(body: &Value, total_seconds: i64, longest_seconds: i64) {
    assert_eq!(body["exercise"]["total_duration_seconds"], total_seconds);
    assert_eq!(body["exercise"]["max_duration_seconds"], longest_seconds);
}

fn get_previous(
    server: &DevServer,
    cookie: &str,
    training_id: &str,
    exercise_id: &str,
    grip: Option<&str>,
) -> support::HttpResponse {
    let grip = match grip {
        None => String::new(),
        Some("grip") => format!("&grip={}", encode(GRIP_JSON)),
        Some("other_grip") => format!(
            "&grip={}",
            encode(r#"{"grip":{"style":"supinated","width":"wide"}}"#)
        ),
        Some(other) => panic!("unknown grip {other}"),
    };
    server.request(
        "GET",
        &format!("/api/trainings/{training_id}/previous?exercise_id={exercise_id}{grip}"),
        &[("Cookie", cookie)],
    )
}

fn encode(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

struct ExerciseSpec<'a> {
    exercise_id: &'a str,
    name: &'a str,
    body_part: &'a str,
    muscle: &'a str,
    record_mode: &'a str,
}

fn create_operation(
    operation_id: &str,
    training_exercise_id: &str,
    spec: ExerciseSpec<'_>,
    position: i64,
) -> Value {
    let exercise_id = spec.exercise_id;
    json!({
        "operation_id": operation_id,
        "training_id": TRAINING_ID,
        "sequence": 1,
        "kind": "training_exercise_create",
        "parent_operation_id": null,
        "payload": {
            "training_id": TRAINING_ID,
            "training_exercise_id": training_exercise_id,
            "exercise_id": exercise_id,
            "snapshot": {
                "exercise_id": exercise_id,
                "name": spec.name,
                "body_part": spec.body_part,
                "muscles": [{"muscle": spec.muscle, "role": "primary"}],
                "record_mode": spec.record_mode,
                "laterality": "bilateral",
                "grip_mode": "none",
                "grip_options": []
            },
            "position": position
        }
    })
}

fn delete_operation(operation_id: &str, training_exercise_id: &str) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": TRAINING_ID,
        "sequence": 3,
        "kind": "training_exercise_delete",
        "parent_operation_id": null,
        "payload": {
            "training_id": TRAINING_ID,
            "training_exercise_id": training_exercise_id
        }
    })
}

fn reorder_operation(operation_id: &str, ordered_ids: &[&str]) -> Value {
    json!({
        "operation_id": operation_id,
        "training_id": TRAINING_ID,
        "sequence": 2,
        "kind": "training_exercise_reorder",
        "parent_operation_id": null,
        "payload": {
            "training_id": TRAINING_ID,
            "ordered_ids": ordered_ids
        }
    })
}

fn post(server: &DevServer, cookie: &str, operation: &Value) -> support::HttpResponse {
    post_with_training(server, cookie, operation, TRAINING_ID)
}

fn post_with_training(
    server: &DevServer,
    cookie: &str,
    operation: &Value,
    training_id: &str,
) -> support::HttpResponse {
    let mut operation = operation.clone();
    operation["training_id"] = json!(training_id);
    if operation["payload"]["training_id"].is_string() {
        operation["payload"]["training_id"] = json!(training_id);
    }
    server.request_with_body(
        "POST",
        "/api/sync/operations",
        &[
            ("Origin", ORIGIN),
            ("Content-Type", "application/json"),
            ("Cookie", cookie),
        ],
        &serde_json::to_string(&operation).expect("the operation is JSON"),
    )
}

fn response_json(response: &support::HttpResponse) -> Value {
    serde_json::from_str(&response.body).expect("the response is JSON")
}

fn status(response: &support::HttpResponse) -> String {
    response_json(response)["status"]
        .as_str()
        .expect("a status")
        .to_owned()
}

fn seed_base(server: &DevServer, token: &str, terminal_token: &str, other_terminal_token: &str) {
    let session_hash = hash_session_token(token);
    let terminal_hash = hash_terminal_token(terminal_token);
    let other_terminal_hash = hash_terminal_token(other_terminal_token);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-record-exercise', 'record-exercise@example.test', '記録の利用者', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-record-exercise-other', 'other@example.test', '別の利用者', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4990', '{USER_ID}', '{session_hash}', \
              '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{TERMINAL_ID}', '{USER_ID}', '{terminal_hash}', '2026-01-01T00:00:00Z', \
              '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{OTHER_TERMINAL_ID}', '{USER_ID}', '{other_terminal_hash}', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, owner_terminal_id, created_at, updated_at) \
             VALUES ('{TRAINING_ID}', '{USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '', 'empty', \
              '{TERMINAL_ID}', '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, owner_terminal_id, created_at, updated_at) \
             VALUES ('{FINISHED_TRAINING_ID}', '{USER_ID}', '2026-11-11', '2026-11-11T03:00:00Z', '', \
              'empty', NULL, '2026-11-11T03:00:00Z', '2026-11-11T03:00:00Z')"
        ),
        format!(
            "UPDATE training_sessions SET finished_at = '2020-01-01T04:00:00Z', \
             total_weight_kg = 0.0 WHERE id = '{FINISHED_TRAINING_ID}'"
        ),
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, created_at, updated_at) \
             VALUES ('{OTHER_TRAINING_ID}', '{OTHER_USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '', \
              'empty', '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the base rows must be seeded");
    }
}

fn cookie(token: &str, terminal_token: &str) -> String {
    format!("warrior_session={token}; warrior_terminal={terminal_token}")
}

fn position(server: &DevServer, training_exercise_id: &str) -> i64 {
    first_row(
        server,
        &format!("SELECT position FROM training_exercises WHERE id = '{training_exercise_id}'"),
    )["position"]
        .as_i64()
        .expect("a position")
}

fn applied_count(server: &DevServer, operation_id: &str) -> i64 {
    count_with(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations WHERE operation_id = '{operation_id}'"
        ),
    )
}

fn count(server: &DevServer, table: &str, id: &str) -> i64 {
    count_with(
        server,
        &format!("SELECT COUNT(*) AS count FROM {table} WHERE id = '{id}'"),
    )
}

fn count_with(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
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

fn seed_set_sql(
    training_exercise_id: &str,
    set_id: &str,
    position: i64,
    kind: &str,
    grip: Option<&str>,
) -> String {
    format!(
        "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, grip, created_at, updated_at) \
         VALUES ('{set_id}', '{USER_ID}', '{training_exercise_id}', {position}, '{kind}', 1, {}, \
          '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
        sql_text(grip)
    )
}

fn seed_segment_sql(
    set_id: &str,
    segment_id: &str,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> String {
    format!(
        "INSERT INTO set_segments \
         (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
          duration_seconds, created_at, updated_at) \
         VALUES ('{segment_id}', '{USER_ID}', '{set_id}', 1, {}, {}, 0, 0, {}, \
          '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
        sql_number(weight_kg),
        sql_number(reps),
        sql_number(duration_seconds)
    )
}

fn bench_snapshot() -> Value {
    json!({
        "exercise_id": BENCH_EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
}

fn plank_snapshot() -> Value {
    json!({
        "exercise_id": PLANK_EXERCISE_ID,
        "name": "プランク",
        "body_part": "abs",
        "muscles": [{"muscle": "rectus_abdominis_upper", "role": "primary"}],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
}

fn sql_text(value: Option<&str>) -> String {
    value.map_or_else(|| "NULL".to_owned(), |text| format!("'{}'", text))
}

fn sql_number<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "NULL".to_owned(), |value| value.to_string())
}

fn json_literal(value: &Value) -> String {
    value.to_string().replace('\'', "''")
}
