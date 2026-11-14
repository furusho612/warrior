use super::*;

pub(crate) fn seed_previous_sessions(server: &DevServer) {
    seed_current_session(
        server,
        PROGRAM_TRAINING_ID,
        "program",
        Some(PAST_ROUTINE_ID_VALUE),
        Some(PAST_PROGRAM_DAY_ID),
        None,
    );
    seed_current_session(
        server,
        FALLBACK_TRAINING_ID,
        "program",
        Some(PAST_ROUTINE_ID_VALUE),
        Some(MISSING_PROGRAM_DAY_ID),
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
    seed_unfinished_session(server);
}

fn seed_unfinished_session(server: &DevServer) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, created_at, updated_at) \
             VALUES ('{UNFINISHED_TRAINING_ID}', '{USER_ID}', '2026-11-13', \
              '2026-11-13T03:00:00Z', '', 'empty', '2026-11-13T03:00:00Z', \
              '2026-11-13T03:00:00Z')"
        ))
        .expect("the unfinished session must be seeded");
    let snapshot = bench_snapshot();
    for sql in [
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{UNFINISHED_ROW_ID}', '{USER_ID}', '{UNFINISHED_TRAINING_ID}', \
              '{BENCH_EXERCISE_ID}', '{}', 1, '2026-11-13T03:00:00Z', \
              '2026-11-13T03:00:00Z')",
            json_literal(&snapshot)
        ),
        seed_set_sql(UNFINISHED_ROW_ID, UNFINISHED_SET_ID, 1, "normal", None),
        seed_segment_sql(
            UNFINISHED_SET_ID,
            UNFINISHED_SEGMENT_ID,
            Some(999.0),
            Some(5),
            None,
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the unfinished set must be seeded");
    }
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
                let base = total_weight / f64::from(count as u32 * 5);
                let weight = if count > 1 {
                    if index == 0 {
                        base + 10.0
                    } else {
                        base - 10.0
                    }
                } else {
                    base
                };
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

pub(crate) fn assert_previous_exercise(body: &Value, total_weight_kg: f64, set_count: usize) {
    assert_eq!(body["exercise"]["total_weight_kg"], total_weight_kg);
    assert_eq!(
        body["exercise"]["sets"].as_array().map(Vec::len),
        Some(set_count)
    );
}

pub(crate) fn assert_previous_time(body: &Value, total_seconds: i64, longest_seconds: i64) {
    assert_eq!(body["exercise"]["total_duration_seconds"], total_seconds);
    assert_eq!(body["exercise"]["max_duration_seconds"], longest_seconds);
}

pub(crate) fn get_previous(
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

/// 種目の作成の操作に載せる種目の定義。
pub(crate) struct ExerciseSpec<'a> {
    pub(crate) exercise_id: &'a str,
    pub(crate) name: &'a str,
    pub(crate) body_part: &'a str,
    pub(crate) muscle: &'a str,
    pub(crate) record_mode: &'a str,
}

pub(crate) fn create_operation(
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

pub(crate) fn delete_operation(operation_id: &str, training_exercise_id: &str) -> Value {
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

pub(crate) fn reorder_operation(operation_id: &str, ordered_ids: &[&str]) -> Value {
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

pub(crate) fn post(server: &DevServer, cookie: &str, operation: &Value) -> support::HttpResponse {
    post_with_training(server, cookie, operation, TRAINING_ID)
}

pub(crate) fn post_with_training(
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

pub(crate) fn response_json(response: &support::HttpResponse) -> Value {
    serde_json::from_str(&response.body).expect("the response is JSON")
}

pub(crate) fn status(response: &support::HttpResponse) -> String {
    response_json(response)["status"]
        .as_str()
        .expect("a status")
        .to_owned()
}

pub(crate) fn seed_base(
    server: &DevServer,
    token: &str,
    terminal_token: &str,
    other_terminal_token: &str,
) {
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

pub(crate) fn cookie(token: &str, terminal_token: &str) -> String {
    format!("warrior_session={token}; warrior_terminal={terminal_token}")
}

pub(crate) fn position(server: &DevServer, training_exercise_id: &str) -> i64 {
    first_row(
        server,
        &format!("SELECT position FROM training_exercises WHERE id = '{training_exercise_id}'"),
    )["position"]
        .as_i64()
        .expect("a position")
}

pub(crate) fn applied_count(server: &DevServer, operation_id: &str) -> i64 {
    count_with(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations WHERE operation_id = '{operation_id}'"
        ),
    )
}

pub(crate) fn count(server: &DevServer, table: &str, id: &str) -> i64 {
    count_with(
        server,
        &format!("SELECT COUNT(*) AS count FROM {table} WHERE id = '{id}'"),
    )
}

pub(crate) fn count_with(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}

pub(crate) fn first_row(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .expect("a row")
}

pub(crate) fn seed_other_training_exercise(server: &DevServer) {
    let snapshot = bench_snapshot();
    for sql in [
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{OTHER_TRAINING_ROW_ID}', '{USER_ID}', '{FINISHED_TRAINING_ID}', \
              '{BENCH_EXERCISE_ID}', '{}', 1, '2026-01-01T00:00:00Z', \
              '2026-01-01T00:00:00Z')",
            json_literal(&snapshot)
        ),
        seed_set_sql(
            OTHER_TRAINING_ROW_ID,
            OTHER_TRAINING_SET_ID,
            1,
            "normal",
            None,
        ),
        seed_segment_sql(
            OTHER_TRAINING_SET_ID,
            OTHER_TRAINING_SEGMENT_ID,
            Some(60.0),
            Some(10),
            None,
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the other training exercise must be seeded");
    }
}

pub(crate) fn seed_set_sql(
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

pub(crate) fn seed_segment_sql(
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
