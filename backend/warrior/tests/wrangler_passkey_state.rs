//! パスキーの再認証と期限切れチャレンジの結合テスト。
//!
//! 対応する要求: `REQ-NFR-013`。
//! 再認証なしの登録を 403 にし、期限切れのチャレンジを 400 にする。

mod support;

use support::DevServer;
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

#[tokio::test]
async fn passkey_registration_requires_elevation_and_rejects_expired_challenges() {
    let server = DevServer::start(8796);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let session_token = base64url::encode(&[7u8; 32]);
    let elevation_token = base64url::encode(&[8u8; 32]);
    let challenge = base64url::encode(&[9u8; 32]);
    let session_hash = hash_session_token(&session_token);
    let elevation_hash = hash_session_token(&elevation_token);
    server
        .execute_d1_json(&format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-alice', 'alice@example.test', 'テスト利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the user must be inserted");
    server
        .execute_d1_json(&format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('11111111-1111-4111-8111-111111111111', '{USER_ID}', '{session_hash}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ))
        .expect("the session must be inserted");
    server
        .execute_d1_json(&format!(
            "INSERT INTO elevation_tokens (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('22222222-2222-4222-8222-222222222222', '{USER_ID}', '{elevation_hash}', '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ))
        .expect("the elevation must be inserted");
    server
        .execute_d1_json(&format!(
            "INSERT INTO webauthn_challenges (id, user_id, challenge, kind, created_at, expires_at) \
             VALUES ('33333333-3333-4333-8333-333333333333', '{USER_ID}', '{challenge}', 'registration', '2000-01-01T00:00:00Z', '2000-01-01T00:05:00Z')"
        ))
        .expect("the challenge must be inserted");

    let session_cookie = format!("warrior_session={session_token}");
    let elevation_cookie = format!("warrior_elevation={elevation_token}");
    let origin = ("Origin", "http://localhost:8787");

    let without_elevation = server.request(
        "POST",
        "/api/auth/passkey/register/start",
        &[("Cookie", session_cookie.as_str()), origin],
    );
    assert_eq!(without_elevation.status, 403);

    let both_cookies = format!("{session_cookie}; {elevation_cookie}");
    let with_elevation = server.request(
        "POST",
        "/api/auth/passkey/register/start",
        &[("Cookie", both_cookies.as_str()), origin],
    );
    assert_eq!(with_elevation.status, 200);

    let client_data = serde_json::json!({
        "type": "webauthn.create",
        "challenge": challenge,
        "origin": "http://localhost:8787"
    })
    .to_string();
    let body = serde_json::json!({
        "name": "期限切れ",
        "credential": {
            "response": {
                "clientDataJSON": base64url::encode(client_data.as_bytes()),
                "attestationObject": base64url::encode(&[0u8])
            }
        }
    })
    .to_string();
    let expired = server.request_with_body(
        "POST",
        "/api/auth/passkey/register/finish",
        &[("Cookie", both_cookies.as_str()), origin],
        &body,
    );
    assert_eq!(expired.status, 400);
}
