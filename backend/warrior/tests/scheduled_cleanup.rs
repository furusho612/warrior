//! Cron Triggers の期限切れ削除の結合テスト。
//!
//! 対応する要求: `REQ-NFR-017`。

mod support;

use support::DevServer;

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

#[test]
fn scheduled_removes_expired_short_lived_rows_and_keeps_future_rows() {
    let server = DevServer::start(8799);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub', 'a@example.test', 'a', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('11111111-1111-4111-8111-111111111111', '{USER_ID}', 'old', '2000-01-01T00:00:00Z', '2000-01-01T00:00:00Z'), \
             ('22222222-2222-4222-8222-222222222222', '{USER_ID}', 'new', '2099-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
        "INSERT INTO webauthn_challenges (id, user_id, challenge, kind, created_at, expires_at) \
         VALUES ('33333333-3333-4333-8333-333333333333', NULL, 'old', 'authentication', '2000-01-01T00:00:00Z', '2000-01-01T00:00:00Z'), \
         ('44444444-4444-4444-8444-444444444444', NULL, 'new', 'authentication', '2099-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
            .to_owned(),
        "INSERT INTO oauth_states (state, nonce, code_verifier, redirect_uri, created_at, expires_at) \
         VALUES ('old', 'n', 'v', 'http://localhost:8787/cb', '2000-01-01T00:00:00Z', '2000-01-01T00:00:00Z'), \
         ('new', 'n', 'v', 'http://localhost:8787/cb', '2099-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
            .to_owned(),
        format!(
            "INSERT INTO user_rate_limits (user_id, kind, window_start, count, expires_at) \
             VALUES ('{USER_ID}', 'old', 0, 1, '2000-01-01T00:00:00Z'), \
             ('{USER_ID}', 'new', 0, 1, '2099-01-01T00:00:00Z')"
        ),
        "INSERT INTO ip_rate_limits (ip, kind, window_start, count, expires_at) \
         VALUES ('old', 'auth', 0, 1, '2000-01-01T00:00:00Z'), \
         ('new', 'auth', 0, 1, '2099-01-01T00:00:00Z')"
            .to_owned(),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the rows must be seeded");
    }

    let scheduled = server.request("GET", "/cdn-cgi/local/scheduled", &[]);
    assert_eq!(scheduled.status, 200);

    assert_eq!(count(&server, "auth_sessions"), 1);
    assert_eq!(count(&server, "webauthn_challenges"), 1);
    assert_eq!(count(&server, "oauth_states"), 1);
    assert_eq!(count(&server, "user_rate_limits"), 1);
    assert_eq!(count(&server, "ip_rate_limits"), 1);
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
