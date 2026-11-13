//! Google OAuth の状態とセッションの結合テスト。
//!
//! 対応する要求: `REQ-NFR-006`、`REQ-NFR-009`。
//! state の 1 回限りと期限切れ、セッションの期限切れと失効、複数の端末、
//! ログに秘密を出さないことを確かめる。

mod support;

use support::{DevServer, TestOidc};
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::ids::uuid_v4_from_bytes;

/// ログインの開始で得た、コールバックに必要な値。
struct Login {
    state: String,
    state_cookie: String,
    callback_path: String,
    code: String,
}

#[tokio::test]
async fn oauth_state_is_single_use_expires_and_the_session_is_revoked() {
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);

    let user = provider.request("GET", "/test/user?sub=alice", &[], None);
    let sub_cookie = cookie_pair(user.header("set-cookie").expect("the test user cookie"));

    let first = begin_login(&server, &provider, &sub_cookie).await;
    let session = finish_login(&server, &first).await;
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
    let user_id = serde_json::from_str::<serde_json::Value>(&me.body)
        .expect("the me response is JSON")
        .get("id")
        .and_then(|value| value.as_str())
        .expect("the user id")
        .to_owned();

    let second = begin_login(&server, &provider, &sub_cookie).await;
    let second_session = finish_login(&server, &second).await;
    let second_me = server.request("GET", "/api/me", &[("Cookie", &second_session)]);
    assert_eq!(second_me.status, 200);
    let second_me_json = serde_json::from_str::<serde_json::Value>(&second_me.body)
        .expect("the second me response is JSON");
    let second_user_id = second_me_json
        .get("id")
        .and_then(|value| value.as_str())
        .expect("the second user id");
    assert_eq!(
        user_id, second_user_id,
        "the same Google sub must keep the same user on another device"
    );

    let expired_token = base64url::encode(&[1u8; 32]);
    let expired_id = uuid_v4_from_bytes([9u8; 16]);
    server
        .execute_d1_json(&format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('{expired_id}', '{user_id}', '{}', '2000-01-01T00:00:00Z', '2000-01-31T00:00:00Z')",
            hash_session_token(&expired_token)
        ))
        .expect("the expired session must be inserted");
    let expired_session = server.request(
        "GET",
        "/api/me",
        &[("Cookie", &format!("warrior_session={expired_token}"))],
    );
    assert_eq!(expired_session.status, 401);
    assert!(
        expired_session.body.contains("\"code\":\"unauthorized\""),
        "{}",
        expired_session.body
    );

    let reused = server.request(
        "GET",
        &first.callback_path,
        &[("Cookie", &first.state_cookie)],
    );
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
    let other_device = server.request("GET", "/api/me", &[("Cookie", &second_session)]);
    assert_eq!(
        other_device.status, 200,
        "logging out one device must keep the other session"
    );
    let second_logout = server.request(
        "POST",
        "/api/auth/logout",
        &[
            ("Cookie", &second_session),
            ("Origin", "http://localhost:8787"),
        ],
    );
    assert_eq!(second_logout.status, 200);
    let after_second_logout = server.request("GET", "/api/me", &[("Cookie", &second_session)]);
    assert_eq!(after_second_logout.status, 401);

    let request_logs = server
        .log_lines()
        .into_iter()
        .filter(|line| line.contains("\"event\":\"request\""))
        .collect::<Vec<_>>();
    for secret in [
        &first.code,
        &first.state,
        &second.code,
        &second.state,
        &session,
        &second_session,
    ] {
        assert!(
            !request_logs.iter().any(|line| line.contains(secret)),
            "the request log must not contain {secret}"
        );
    }
}

async fn begin_login(server: &DevServer, provider: &TestOidc, sub_cookie: &str) -> Login {
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
        &[("Cookie", sub_cookie)],
        None,
    );
    assert_eq!(authorize.status, 302);
    let callback_location = authorize.header("location").expect("the callback redirect");
    let callback_path = callback_location
        .strip_prefix("http://localhost:8787")
        .expect("the callback must point at the app")
        .to_owned();
    let code = query_value(&callback_path, "code");
    Login {
        state,
        state_cookie,
        callback_path,
        code,
    }
}

async fn finish_login(server: &DevServer, login: &Login) -> String {
    let callback = server.request(
        "GET",
        &login.callback_path,
        &[("Cookie", &login.state_cookie)],
    );
    assert_eq!(callback.status, 302);
    callback
        .headers
        .iter()
        .find_map(|(name, value)| {
            (name == "set-cookie" && value.starts_with("warrior_session="))
                .then(|| cookie_pair(value))
        })
        .expect("the session cookie")
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
