//! プッシュ通知の購読の結合テスト。
//!
//! 対応する要求: `REQ-TMR-008`、`REQ-NFR-008`。
//! endpoint の許可と拒否、同じ endpoint の重複、端末トークンの保存、
//! ログアウトとアカウントの削除による購読の削除を、ローカルの D1 に対して確かめる。
//! 実際のプッシュサービスへの購読と配信は、staging と実機の計測で確かめる。

mod support;

use serde_json::{json, Value};
use support::{DevServer, HttpResponse};
use warrior_core::auth::{hash_session_token, hash_terminal_token};
use warrior_core::base64url;

const ORIGIN: &str = "http://localhost:8787";
const ALLOWLIST_PORT: u16 = 8830;
const DELETION_PORT: u16 = 8831;
const RATE_LIMIT_PORT: u16 = 8832;
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3a01";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3a02";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3a10";
const SECOND_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3a11";
const OTHER_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3a12";
const SESSION_SEED: u8 = 41;
const OTHER_SESSION_SEED: u8 = 42;
const TERMINAL_SEED: u8 = 51;
const SECOND_TERMINAL_SEED: u8 = 52;
const OTHER_TERMINAL_SEED: u8 = 53;
const NOW: &str = "2026-11-12T00:00:00Z";

#[test]
fn the_push_endpoint_allowlist_and_the_upsert_are_enforced() {
    let public_key = "A".repeat(87);
    let server = DevServer::start_with_vars(ALLOWLIST_PORT, &[("VAPID_PUBLIC_KEY", &public_key)]);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_users_and_terminals(&server);
    let cookie = cookies(SESSION_SEED, TERMINAL_SEED);
    let second_cookie = cookies(SESSION_SEED, SECOND_TERMINAL_SEED);
    let other_cookie = cookies(OTHER_SESSION_SEED, OTHER_TERMINAL_SEED);

    assert_eq!(
        server.request("GET", "/api/push/public-key", &[]).status,
        401,
        "the public key requires authentication"
    );
    let response = server.request(
        "GET",
        "/api/push/public-key",
        &[("Cookie", cookie.as_str())],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        serde_json::from_str::<Value>(&response.body).expect("the public key response"),
        json!({ "public_key": public_key })
    );

    let without_authentication = post(
        &server,
        &[],
        &subscription("https://fcm.googleapis.com/fcm/send/a", "p256dh", "auth"),
    );
    assert_eq!(without_authentication.status, 401);
    let without_terminal = post(
        &server,
        &[("Cookie", session_cookie(SESSION_SEED).as_str())],
        &subscription("https://fcm.googleapis.com/fcm/send/a", "p256dh", "auth"),
    );
    assert_eq!(without_terminal.status, 400);

    for endpoint in [
        "http://fcm.googleapis.com/fcm/send/a",
        "http://web.push.apple.com/send/a",
        "https://evil.example/send/a",
        "https://web.push.apple.com.evil.example/send/a",
        "https://sub.fcm.googleapis.com/send/a",
    ] {
        let response = post(
            &server,
            &[("Cookie", cookie.as_str())],
            &subscription(endpoint, "p256dh", "auth"),
        );
        assert_eq!(
            response.status, 400,
            "the endpoint {endpoint} must be rejected"
        );
        assert_eq!(
            subscription_count(&server, USER_ID),
            0,
            "a rejected endpoint must not be stored"
        );
    }
    assert_eq!(
        post(
            &server,
            &[("Cookie", cookie.as_str())],
            &subscription("https://fcm.googleapis.com/fcm/send/a", "", "auth"),
        )
        .status,
        400,
        "the keys are required"
    );

    let apple = "https://web.push.apple.com/send/a";
    let created = post(
        &server,
        &[("Cookie", cookie.as_str())],
        &subscription(apple, "p256dh-1", "auth-1"),
    );
    assert_eq!(created.status, 204, "{}", created.body);
    assert!(created.body.is_empty());
    assert_eq!(subscription_count(&server, USER_ID), 1);
    let row = subscription_row(&server, USER_ID, apple);
    assert_eq!(row["endpoint"], apple);
    assert_eq!(row["p256dh"], "p256dh-1");
    assert_eq!(row["auth"], "auth-1");
    assert_eq!(row["terminal_token_id"], TERMINAL_ID);
    assert_eq!(
        row["created_at"], row["updated_at"],
        "the first registration sets both times"
    );
    let created_at = row["created_at"].clone();

    let updated = post(
        &server,
        &[("Cookie", second_cookie.as_str())],
        &subscription(apple, "p256dh-2", "auth-2"),
    );
    assert_eq!(updated.status, 204, "{}", updated.body);
    assert_eq!(
        subscription_count(&server, USER_ID),
        1,
        "the same endpoint must keep one row"
    );
    let row = subscription_row(&server, USER_ID, apple);
    assert_eq!(row["p256dh"], "p256dh-2");
    assert_eq!(row["auth"], "auth-2");
    assert_eq!(
        row["terminal_token_id"], SECOND_TERMINAL_ID,
        "the latest terminal owns the subscription"
    );
    assert_eq!(
        row["created_at"], created_at,
        "the first registration time must stay"
    );

    let chrome = "https://fcm.googleapis.com/fcm/send/b";
    let second = post(
        &server,
        &[("Cookie", cookie.as_str())],
        &subscription(chrome, "p256dh-3", "auth-3"),
    );
    assert_eq!(second.status, 204, "{}", second.body);
    assert_eq!(subscription_count(&server, USER_ID), 2);

    let other = post(
        &server,
        &[("Cookie", other_cookie.as_str())],
        &subscription(apple, "p256dh-4", "auth-4"),
    );
    assert_eq!(other.status, 204, "{}", other.body);
    assert_eq!(
        subscription_count(&server, OTHER_USER_ID),
        1,
        "the same endpoint belongs to each user"
    );
    assert_eq!(subscription_count(&server, USER_ID), 2);

    let deleted = delete(&server, &cookie, apple);
    assert_eq!(deleted.status, 204, "{}", deleted.body);
    assert_eq!(subscription_count(&server, USER_ID), 1);
    assert_eq!(subscription_count(&server, OTHER_USER_ID), 1);
    assert_eq!(delete(&server, &cookie, apple).status, 204);
    assert_eq!(subscription_count(&server, USER_ID), 1);
}

#[test]
fn logout_and_account_deletion_remove_the_push_subscriptions() {
    let public_key = "A".repeat(87);
    let server = DevServer::start_with_vars(DELETION_PORT, &[("VAPID_PUBLIC_KEY", &public_key)]);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_users_and_terminals(&server);
    let cookie = cookies(SESSION_SEED, TERMINAL_SEED);
    let second_cookie = cookies(SESSION_SEED, SECOND_TERMINAL_SEED);
    let other_cookie = cookies(OTHER_SESSION_SEED, OTHER_TERMINAL_SEED);

    for (current, endpoint) in [
        (&cookie, "https://web.push.apple.com/send/logout-a"),
        (&cookie, "https://fcm.googleapis.com/fcm/send/logout-b"),
        (
            &second_cookie,
            "https://fcm.googleapis.com/fcm/send/other-terminal",
        ),
        (
            &other_cookie,
            "https://fcm.googleapis.com/fcm/send/other-user",
        ),
    ] {
        let response = post(
            &server,
            &[("Cookie", current.as_str())],
            &subscription(endpoint, "p256dh", "auth"),
        );
        assert_eq!(response.status, 204, "{}", response.body);
    }
    assert_eq!(subscription_count(&server, USER_ID), 3);
    assert_eq!(subscription_count(&server, OTHER_USER_ID), 1);

    let logged_out = server.request(
        "POST",
        "/api/auth/logout",
        &[("Cookie", cookie.as_str()), ("Origin", ORIGIN)],
    );
    assert_eq!(logged_out.status, 200, "{}", logged_out.body);
    assert_eq!(
        subscription_count(&server, USER_ID),
        1,
        "only the terminal that logged out loses its subscriptions"
    );
    assert_eq!(
        subscription_row(
            &server,
            USER_ID,
            "https://fcm.googleapis.com/fcm/send/other-terminal"
        )["terminal_token_id"],
        SECOND_TERMINAL_ID
    );
    assert_eq!(
        count(&server, "auth_sessions"),
        1,
        "the other user's session must stay"
    );
    assert_eq!(
        count(&server, "terminal_tokens"),
        3,
        "the terminal identity stays for the next login"
    );

    let deleted = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Cookie", other_cookie.as_str()), ("Origin", ORIGIN)],
        r#"{"confirm":"delete-account"}"#,
    );
    assert_eq!(deleted.status, 204, "{}", deleted.body);
    assert_eq!(
        subscription_count(&server, OTHER_USER_ID),
        0,
        "account deletion must remove every subscription"
    );
    assert_eq!(
        subscription_count(&server, USER_ID),
        1,
        "another user's subscriptions must stay"
    );
}

fn seed_users_and_terminals(server: &DevServer) {
    let session_token = base64url::encode(&[SESSION_SEED; 32]);
    let other_session_token = base64url::encode(&[OTHER_SESSION_SEED; 32]);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-push', 'push@example.test', 'プッシュの利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-other-push', 'other-push@example.test', '別の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c3a20', '{USER_ID}', '{}', '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(&session_token)
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c3a21', '{OTHER_USER_ID}', '{}', '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(&other_session_token)
        ),
        terminal_insert(TERMINAL_ID, USER_ID, TERMINAL_SEED),
        terminal_insert(SECOND_TERMINAL_ID, USER_ID, SECOND_TERMINAL_SEED),
        terminal_insert(OTHER_TERMINAL_ID, OTHER_USER_ID, OTHER_TERMINAL_SEED),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the push fixture must be seeded");
    }
}

fn terminal_insert(id: &str, user_id: &str, seed: u8) -> String {
    format!(
        "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
         VALUES ('{id}', '{user_id}', '{}', '{NOW}', '{NOW}')",
        hash_terminal_token(&base64url::encode(&[seed; 32]))
    )
}

fn subscription(endpoint: &str, p256dh: &str, auth: &str) -> Value {
    json!({
        "endpoint": endpoint,
        "p256dh": p256dh,
        "auth": auth,
    })
}

fn session_cookie(seed: u8) -> String {
    format!("warrior_session={}", base64url::encode(&[seed; 32]))
}

fn cookies(session_seed: u8, terminal_seed: u8) -> String {
    format!(
        "{}; warrior_terminal={}",
        session_cookie(session_seed),
        base64url::encode(&[terminal_seed; 32])
    )
}

fn post(server: &DevServer, headers: &[(&str, &str)], body: &Value) -> HttpResponse {
    let mut all = headers.to_vec();
    all.push(("Origin", ORIGIN));
    server.request_with_body("POST", "/api/push/subscriptions", &all, &body.to_string())
}

fn delete(server: &DevServer, cookie: &str, endpoint: &str) -> HttpResponse {
    server.request_with_body(
        "DELETE",
        "/api/push/subscriptions",
        &[("Cookie", cookie), ("Origin", ORIGIN)],
        &json!({ "endpoint": endpoint }).to_string(),
    )
}

fn subscription_count(server: &DevServer, user_id: &str) -> i64 {
    count_with(
        server,
        &format!("SELECT COUNT(*) AS count FROM push_subscriptions WHERE user_id = '{user_id}'"),
    )
}

fn subscription_row(server: &DevServer, user_id: &str, endpoint: &str) -> Value {
    server
        .execute_d1_json(&format!(
            "SELECT endpoint, p256dh, auth, terminal_token_id, created_at, updated_at \
             FROM push_subscriptions WHERE user_id = '{user_id}' AND endpoint = '{endpoint}'"
        ))
        .expect("the subscription must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .expect("a stored subscription")
}

fn count(server: &DevServer, table: &str) -> i64 {
    count_with(server, &format!("SELECT COUNT(*) AS count FROM {table}"))
}

fn count_with(server: &DevServer, sql: &str) -> i64 {
    server
        .execute_d1_json(sql)
        .expect("the count must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(Value::as_i64)
        .expect("a count")
}

#[test]
fn the_push_subscription_requests_are_rate_limited_per_user() {
    let server = DevServer::start_with_vars(
        RATE_LIMIT_PORT,
        &[
            ("RATE_LIMIT_PUSH_USER", "1"),
            // テストの途中で窓が切り替わらないよう、長い窓を使う。
            ("RATE_LIMIT_PUSH_WINDOW", "3600"),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_users_and_terminals(&server);
    let cookie = cookies(SESSION_SEED, TERMINAL_SEED);
    let other_cookie = cookies(OTHER_SESSION_SEED, OTHER_TERMINAL_SEED);

    let first = post(
        &server,
        &[("Cookie", cookie.as_str())],
        &subscription(
            "https://fcm.googleapis.com/fcm/send/rate-a",
            "p256dh",
            "auth",
        ),
    );
    assert_eq!(first.status, 204, "{}", first.body);
    let limited = post(
        &server,
        &[("Cookie", cookie.as_str())],
        &subscription(
            "https://fcm.googleapis.com/fcm/send/rate-b",
            "p256dh",
            "auth",
        ),
    );
    assert_eq!(limited.status, 429, "{}", limited.body);
    assert!(limited.body.contains("\"code\":\"rate_limited\""));
    let retry = limited
        .header("retry-after")
        .and_then(|value| value.parse::<u64>().ok())
        .expect("Retry-After");
    assert!((1..=60).contains(&retry));
    assert_eq!(
        subscription_count(&server, USER_ID),
        1,
        "the rejected request must not be stored"
    );

    let other = post(
        &server,
        &[("Cookie", other_cookie.as_str())],
        &subscription(
            "https://fcm.googleapis.com/fcm/send/rate-other",
            "p256dh",
            "auth",
        ),
    );
    assert_eq!(other.status, 204, "{}", other.body);
    assert_eq!(subscription_count(&server, OTHER_USER_ID), 1);

    let deleted = delete(
        &server,
        &cookie,
        "https://fcm.googleapis.com/fcm/send/rate-a",
    );
    assert_eq!(deleted.status, 429, "{}", deleted.body);
}
