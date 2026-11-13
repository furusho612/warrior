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

    assert_eq!(count(&server, "users"), 0);
    assert_eq!(count(&server, "auth_sessions"), 0);
    assert_eq!(count(&server, "passkeys"), 0);
    assert_eq!(count(&server, "webauthn_challenges"), 0);
    assert_eq!(count(&server, "elevation_tokens"), 0);
    assert_eq!(count(&server, "user_rate_limits"), 0);
    assert_eq!(count(&server, "ip_rate_limits"), 1);
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
