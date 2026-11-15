//! アカウント削除の結合テスト。
//!
//! 対応する要求: `REQ-MYP-007`、`REQ-NFR-010`。
//! 認証を先に検査し、R2 と D1 の利用者の行を削除し、2 回目を 401 にする。

mod support;

use support::DevServer;
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

#[tokio::test]
async fn account_deletion_removes_user_rows_and_r2_objects_once() {
    let server = DevServer::start(8797);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let session_token = base64url::encode(&[7u8; 32]);
    let session_hash = hash_session_token(&session_token);
    seed_user(&server, &session_hash);
    server
        .put_r2_object(&format!("users/{USER_ID}/ai.json"), b"ai")
        .expect("the user's R2 object must be written");
    server
        .put_r2_object(&format!("exports/{USER_ID}/records.csv"), b"csv")
        .expect("the export must be written");
    server
        .put_r2_object("users/other-user/ai.json", b"keep")
        .expect("the other user's object must be written");

    let session_cookie = format!("warrior_session={session_token}");
    let origin = ("Origin", "http://localhost:8787");
    let without_auth = server.request("DELETE", "/api/account", &[origin]);
    assert_eq!(without_auth.status, 401);

    let without_confirm = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Cookie", session_cookie.as_str()), origin],
        "{}",
    );
    assert_eq!(without_confirm.status, 400);

    let deleted = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Cookie", session_cookie.as_str()), origin],
        r#"{"confirm":"delete-account"}"#,
    );
    assert_eq!(deleted.status, 204);
    assert!(deleted.body.is_empty());
    let cookies = deleted
        .headers
        .iter()
        .filter(|(name, _)| name == "set-cookie")
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>();
    for cookie in [
        "warrior_session=;",
        "warrior_elevation=;",
        "warrior_terminal=;",
    ] {
        assert!(
            cookies
                .iter()
                .any(|value| value.contains(cookie) && value.contains("Max-Age=0")),
            "the cookie {cookie} must be cleared: {cookies:?}"
        );
    }

    assert_eq!(count(&server, "users"), 0);
    assert_eq!(count(&server, "auth_sessions"), 0);
    assert_eq!(count(&server, "terminal_tokens"), 0);
    assert_eq!(count(&server, "passkeys"), 0);
    assert_eq!(count(&server, "webauthn_challenges"), 0);
    assert_eq!(count(&server, "elevation_tokens"), 0);
    assert_eq!(count(&server, "user_rate_limits"), 0);
    assert_eq!(count(&server, "ip_rate_limits"), 1);
    assert_eq!(count(&server, "user_exercises"), 0);
    assert_eq!(count(&server, "user_exercise_muscles"), 0);
    assert_eq!(count(&server, "master_exercise_overrides"), 0);
    assert_eq!(count(&server, "training_sessions"), 0);
    assert_eq!(count(&server, "training_exercises"), 0);
    assert_eq!(count(&server, "sets"), 0);
    assert_eq!(count(&server, "set_segments"), 0);
    assert_eq!(count(&server, "supersets"), 0);
    assert_eq!(count(&server, "superset_members"), 0);
    assert_eq!(count(&server, "applied_operations"), 0);
    assert_eq!(count(&server, "personal_bests"), 0);
    assert_eq!(count(&server, "profiles"), 0);
    assert_eq!(count(&server, "user_settings"), 0);
    assert_eq!(count(&server, "muscles"), 24);
    assert_eq!(count(&server, "master_exercises"), 107);
    assert_eq!(
        server
            .get_r2_object(&format!("users/{USER_ID}/ai.json"))
            .expect("the R2 read must work"),
        None
    );
    assert_eq!(
        server
            .get_r2_object(&format!("exports/{USER_ID}/records.csv"))
            .expect("the R2 read must work"),
        None
    );
    assert_eq!(
        server
            .get_r2_object("users/other-user/ai.json")
            .expect("the R2 read must work"),
        Some(b"keep".to_vec())
    );

    let again = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Cookie", session_cookie.as_str()), origin],
        r#"{"confirm":"delete-account"}"#,
    );
    assert_eq!(again.status, 401);

    // 同じ利用者 ID への行の削除をもう一度実行しても、最終状態は変わらない。
    for table in [
        "webauthn_challenges",
        "elevation_tokens",
        "passkeys",
        "auth_sessions",
        "terminal_tokens",
    ] {
        server
            .execute_d1_json(&format!("DELETE FROM {table} WHERE user_id = '{USER_ID}'"))
            .expect("the repeated deletion must succeed");
    }
    assert_eq!(count(&server, "auth_sessions"), 0);
}

fn seed_user(server: &DevServer, session_hash: &str) {
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-alice', 'alice@example.test', 'テスト利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('11111111-1111-4111-8111-111111111111', '{USER_ID}', '{session_hash}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('55555555-5555-4555-8555-555555555555', '{USER_ID}', 'terminal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO passkeys (id, user_id, credential_id, public_key, sign_count, transports, name, created_at) \
             VALUES ('22222222-2222-4222-8222-222222222222', '{USER_ID}', 'credential', 'key', 0, '', 'パスキー', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO webauthn_challenges (id, user_id, challenge, kind, created_at, expires_at) \
             VALUES ('33333333-3333-4333-8333-333333333333', '{USER_ID}', 'challenge', 'registration', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO elevation_tokens (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('44444444-4444-4444-8444-444444444444', '{USER_ID}', 'elevation', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO user_rate_limits (user_id, kind, window_start, count, expires_at) \n             VALUES ('{USER_ID}', 'ai', 0, 1, '2099-01-01T00:00:00Z')"
        ),
        "INSERT INTO ip_rate_limits (ip, kind, window_start, count, expires_at) \
         VALUES ('203.0.113.10', 'auth', 0, 1, '2099-01-01T00:00:00Z')"
            .to_owned(),
        "INSERT INTO master_exercises (id, name, name_kana, body_part, equipment, record_mode, laterality, grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options) \
         VALUES ('master-1', 'ベンチプレス', 'べんちぷれす', 'chest', 'barbell', 'weight_and_reps', 'bilateral', 'none', NULL, '押す', '注意', 'コツ', 1, '[]')"
            .to_owned(),
        format!(
            "INSERT INTO user_exercises (id, user_id, name, name_kana, body_part, equipment, record_mode, laterality, grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options, created_at, updated_at) \
             VALUES ('user-1', '{USER_ID}', '独自', 'どくじ', 'chest', 'barbell', 'weight_and_reps', 'bilateral', 'none', NULL, '押す', '注意', 'コツ', 1, '[]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        "INSERT INTO user_exercise_muscles (exercise_id, muscle_id, role) \
         VALUES ('user-1', 'pectoralis_major_middle', 'primary')"
            .to_owned(),
        format!(
            "INSERT INTO master_exercise_overrides (user_id, master_exercise_id, deleted, updated_at) \
             VALUES ('{USER_ID}', 'master-1', 0, '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('training-1', '{USER_ID}', '2026-01-01', '2026-01-01T00:00:00Z', 'メモ', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('training-exercise-1', '{USER_ID}', 'training-1', 'master-1', '{{}}', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('set-1', '{USER_ID}', 'training-exercise-1', 0, 'normal', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, created_at, updated_at) \
             VALUES ('segment-1', '{USER_ID}', 'set-1', 0, 100.0, 5, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO supersets (id, user_id, training_id, position, created_at, updated_at) \
             VALUES ('superset-1', '{USER_ID}', 'training-1', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO superset_members (user_id, superset_id, training_exercise_id, position) \
             VALUES ('{USER_ID}', 'superset-1', 'training-exercise-1', 0)"
        ),
        format!(
            "INSERT INTO applied_operations (operation_id, user_id, training_id, kind, applied_at) \
             VALUES ('operation-1', '{USER_ID}', 'training-1', 'training_session_create', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO personal_bests (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id, created_at, updated_at) \
             VALUES ('{USER_ID}', 'master-1', '', '', 'max_weight', 100.0, '2026-01-01', 'training-1', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, training_experience, goal, training_days_per_week, activity_level, created_at, updated_at) \
             VALUES ('{USER_ID}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', 'hypertrophy', 4, 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO user_settings (user_id, rest_default_seconds, display_mode, protein_g_per_kg, fat_ratio, created_at, updated_at) \
             VALUES ('{USER_ID}', 120, 'dark', 1.8, 0.25, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the user rows must be seeded");
    }
}

fn count(server: &DevServer, table: &str) -> i64 {
    let result = server
        .execute_d1_json(&format!("SELECT COUNT(*) AS count FROM {table}"))
        .expect("the count must be readable");
    result
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_i64())
        .expect("a count")
}
