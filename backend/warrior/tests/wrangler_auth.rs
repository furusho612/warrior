//! Google OAuth の状態とセッションの結合テスト。
//!
//! 対応する要求: `REQ-NFR-006`、`REQ-NFR-009`。
//! state の 1 回限りと期限切れ、セッションの失効、ログに秘密を出さないことを確かめる。

mod support;

use support::{DevServer, TestOidc};
use warrior_core::base64url;

#[tokio::test]
async fn oauth_state_is_single_use_expires_and_the_session_is_revoked() {
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);

    let user = provider.request("GET", "/test/user?sub=alice", &[], None);
    let sub_cookie = cookie_pair(user.header("set-cookie").expect("the test user cookie"));
    let start = server.request("GET", "/api/auth/google/start", &[]);
    assert_eq!(start.status, 302);
    let start_location = start.header("location").expect("the authorize redirect");
    let state = query_value(start_location, "state");
    let state_cookie = cookie_pair(start.header("set-cookie").expect("the state cookie"));

    let authorize = provider.request(
        "GET",
        &format!(
            "/authorize?{}",
            start_location
                .split_once('?')
                .expect("the authorize query")
                .1
        ),
        &[("Cookie", &sub_cookie)],
        None,
    );
    assert_eq!(authorize.status, 302);
    let callback_location = authorize.header("location").expect("the callback redirect");
    let callback_path = callback_location
        .strip_prefix("http://localhost:8787")
        .expect("the callback must point at the app");
    let code = query_value(callback_path, "code");

    let callback = server.request("GET", callback_path, &[("Cookie", &state_cookie)]);
    assert_eq!(callback.status, 302);
    let session = callback
        .headers
        .iter()
        .find_map(|(name, value)| {
            (name == "set-cookie" && value.starts_with("warrior_session="))
                .then(|| cookie_pair(value))
        })
        .expect("the session cookie");
    let me = server.request("GET", "/api/me", &[("Cookie", &session)]);
    assert_eq!(me.status, 200);
    assert!(
        me.body.contains("\"email\":\"alice@example.test\""),
        "{}",
        me.body
    );
    assert!(
        me.body.contains("\"display_name\":\"テスト利用者\""),
        "{}",
        me.body
    );

    let reused = server.request("GET", callback_path, &[("Cookie", &state_cookie)]);
    assert_eq!(reused.status, 302);
    assert!(reused
        .header("location")
        .expect("the login error redirect")
        .contains("/login/error"));

    let expired_state = base64url::encode(&[0u8; 32]);
    server
        .execute_d1_json(&format!(
            "INSERT INTO oauth_states (state, nonce, code_verifier, redirect_uri, created_at, expires_at) \
             VALUES ('{expired_state}', 'nonce', 'verifier', 'http://localhost:8787/api/auth/google/callback', '2000-01-01T00:00:00Z', '2000-01-01T00:05:00Z')"
        ))
        .expect("the expired state must be inserted");
    let expired = server.request(
        "GET",
        &format!("/api/auth/google/callback?state={expired_state}&code=expired"),
        &[("Cookie", &format!("warrior_oauth_state={expired_state}"))],
    );
    assert_eq!(expired.status, 302);
    assert!(expired
        .header("location")
        .expect("the login error redirect")
        .contains("/login/error"));

    let logout = server.request(
        "POST",
        "/api/auth/logout",
        &[("Cookie", &session), ("Origin", "http://localhost:8787")],
    );
    assert_eq!(logout.status, 200);
    let after_logout = server.request("GET", "/api/me", &[("Cookie", &session)]);
    assert_eq!(after_logout.status, 401);
    assert!(
        after_logout.body.contains("\"code\":\"unauthorized\""),
        "{}",
        after_logout.body
    );

    let request_logs = server
        .log_lines()
        .into_iter()
        .filter(|line| line.contains("\"event\":\"request\""))
        .collect::<Vec<_>>();
    for secret in [&code, &state, &session] {
        assert!(
            !request_logs.iter().any(|line| line.contains(secret)),
            "the request log must not contain {secret}"
        );
    }
}

fn cookie_pair(header: &str) -> String {
    header
        .split(';')
        .next()
        .expect("a cookie pair")
        .trim()
        .to_owned()
}

fn query_value(url: &str, name: &str) -> String {
    url.split_once('?')
        .expect("a query string")
        .1
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then(|| value.to_owned())
        })
        .unwrap_or_else(|| panic!("the query must contain {name}: {url}"))
}
