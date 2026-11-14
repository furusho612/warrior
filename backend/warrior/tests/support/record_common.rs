//! 記録の結合テストが共有する、操作の送信、応答の読み取り、D1 の行の準備。

use serde_json::{json, Value};
use warrior_core::auth::{hash_session_token, hash_terminal_token};

use super::{DevServer, HttpResponse};

/// 同期の操作を送る。
pub(crate) fn post_operation(
    server: &DevServer,
    cookie: &str,
    operation: &Value,
    origin: &str,
) -> HttpResponse {
    server.request_with_body(
        "POST",
        "/api/sync/operations",
        &[
            ("Origin", origin),
            ("Content-Type", "application/json"),
            ("Cookie", cookie),
        ],
        &serde_json::to_string(operation).expect("the operation is JSON"),
    )
}

/// 応答の `status` を返す。
pub(crate) fn status(response: &HttpResponse) -> String {
    serde_json::from_str::<Value>(&response.body).expect("the response is JSON")["status"]
        .as_str()
        .expect("a status")
        .to_owned()
}

/// 適用済みの操作 ID の件数。
pub(crate) fn applied_count(server: &DevServer, operation_id: &str) -> i64 {
    count_with(
        server,
        &format!(
            "SELECT COUNT(*) AS count FROM applied_operations \
             WHERE operation_id = '{operation_id}'"
        ),
    )
}

/// 表の ID の行数。
pub(crate) fn count(server: &DevServer, table: &str, id: &str) -> i64 {
    count_with(
        server,
        &format!("SELECT COUNT(*) AS count FROM {table} WHERE id = '{id}'"),
    )
}

/// 1 つの件数を返す SQL の結果。
pub(crate) fn count_with(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}

/// 1 行目の結果。
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

/// 1 行目があれば返す。
pub(crate) fn optional_row(server: &DevServer, sql: &str) -> Option<Value> {
    server
        .execute_d1_json(sql)
        .ok()?
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
}

/// セッションと端末の Cookie。
pub(crate) fn cookie(token: &str, terminal_token: &str) -> String {
    format!("warrior_session={token}; warrior_terminal={terminal_token}")
}

/// 利用者を作る。
pub(crate) fn seed_user(
    server: &DevServer,
    user_id: &str,
    google_sub: &str,
    email: &str,
    display_name: &str,
) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{user_id}', '{google_sub}', '{email}', '{display_name}', \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the user must be seeded");
}

/// セッションを作る。
pub(crate) fn seed_auth_session(server: &DevServer, session_id: &str, user_id: &str, token: &str) {
    let token_hash = hash_session_token(token);
    server
        .execute_d1_json(&format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('{session_id}', '{user_id}', '{token_hash}', '2026-01-01T00:00:00Z', \
              '2099-01-01T00:00:00Z')"
        ))
        .expect("the session must be seeded");
}

/// 端末トークンを作る。
pub(crate) fn seed_terminal(server: &DevServer, terminal_id: &str, user_id: &str, token: &str) {
    let token_hash = hash_terminal_token(token);
    server
        .execute_d1_json(&format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{terminal_id}', '{user_id}', '{token_hash}', '2026-01-01T00:00:00Z', \
              '2026-01-01T00:00:00Z')"
        ))
        .expect("the terminal must be seeded");
}

/// トレーニングを作る。終了したトレーニングは `finished_at` を入れる。
pub(crate) fn seed_session(
    server: &DevServer,
    training_id: &str,
    user_id: &str,
    date: &str,
    owner_terminal_id: Option<&str>,
    finished_at: Option<&str>,
) {
    let owner = sql_text(owner_terminal_id);
    let finished = sql_text(finished_at);
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, owner_terminal_id, finished_at, \
              created_at, updated_at) \
             VALUES ('{training_id}', '{user_id}', '{date}', '{date}T03:00:00Z', '', 'empty', \
              {owner}, {finished}, '{date}T03:00:00Z', '{date}T03:00:00Z')"
        ))
        .expect("the training must be seeded");
}

/// 種目を作るための指定。
pub(crate) struct ExerciseSeed<'a> {
    pub(crate) row_id: &'a str,
    pub(crate) user_id: &'a str,
    pub(crate) training_id: &'a str,
    pub(crate) exercise_id: &'a str,
    pub(crate) name: &'a str,
    pub(crate) body_part: &'a str,
    pub(crate) muscle: &'a str,
    pub(crate) record_mode: &'a str,
    pub(crate) position: i64,
}

/// 種目を作る。
pub(crate) fn seed_exercise(server: &DevServer, seed: ExerciseSeed<'_>) {
    let ExerciseSeed {
        row_id,
        user_id,
        training_id,
        exercise_id,
        name,
        body_part,
        muscle,
        record_mode,
        position,
    } = seed;
    let snapshot = json!({
        "exercise_id": exercise_id,
        "name": name,
        "body_part": body_part,
        "muscles": [{"muscle": muscle, "role": "primary"}],
        "record_mode": record_mode,
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{row_id}', '{user_id}', '{training_id}', '{exercise_id}', '{}', {position}, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            json_literal(&snapshot)
        ))
        .expect("the exercise must be seeded");
}

/// セットを作る。
pub(crate) fn seed_set(
    server: &DevServer,
    set_id: &str,
    user_id: &str,
    row_id: &str,
    position: i64,
    kind: &str,
    completed: i64,
) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{set_id}', '{user_id}', '{row_id}', {position}, '{kind}', {completed}, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the set must be seeded");
}

/// 段を作るための指定。
pub(crate) struct SegmentSeed<'a> {
    pub(crate) segment_id: &'a str,
    pub(crate) user_id: &'a str,
    pub(crate) set_id: &'a str,
    pub(crate) position: i64,
    pub(crate) weight_kg: Option<f64>,
    pub(crate) reps: Option<i64>,
    pub(crate) duration_seconds: Option<i64>,
}

/// 段を作る。
pub(crate) fn seed_segment(server: &DevServer, seed: SegmentSeed<'_>) {
    let SegmentSeed {
        segment_id,
        user_id,
        set_id,
        position,
        weight_kg,
        reps,
        duration_seconds,
    } = seed;
    server
        .execute_d1_json(&format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              duration_seconds, created_at, updated_at) \
             VALUES ('{segment_id}', '{user_id}', '{set_id}', {position}, {}, {}, 0, 0, {}, \
              '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            sql_number(weight_kg),
            sql_number(reps),
            sql_number(duration_seconds)
        ))
        .expect("the segment must be seeded");
}

/// SQL の文字列または NULL。
pub(crate) fn sql_text(value: Option<&str>) -> String {
    value.map_or_else(
        || "NULL".to_owned(),
        |text| format!("'{}'", text.replace('\'', "''")),
    )
}

/// SQL の数値または NULL。
pub(crate) fn sql_number<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "NULL".to_owned(), |value| value.to_string())
}

/// SQL の文字列リテラルにする。
pub(crate) fn json_literal(value: &Value) -> String {
    value.to_string().replace('\'', "''")
}
