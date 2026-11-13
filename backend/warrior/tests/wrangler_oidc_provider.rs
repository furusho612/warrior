//! テスト専用の OIDC の提供者の経路のテスト。
//!
//! 対応する要求はない。ADR-024 が定める E2E の認証の基盤を確かめる。
//! 本物の Google のアカウントには接続しない。

mod support;

use support::TestOidc;
use warrior_core::base64url;

#[test]
fn wrangler_oidc_provider_returns_google_shaped_responses() {
    let provider = TestOidc::start(8789);

    let user = provider.request("GET", "/test/user?sub=alice", &[], None);
    assert_eq!(user.status, 200);
    assert_eq!(
        user.header("set-cookie"),
        Some("sub=alice; Path=/; HttpOnly")
    );

    let authorize = provider.request(
        "GET",
        "/authorize?redirect_uri=http://localhost:8787/callback&state=s1&nonce=n1&client_id=warrior-local",
        &[("Cookie", "sub=alice")],
        None,
    );
    assert_eq!(authorize.status, 302);
    let location = authorize.header("location").expect("a redirect location");
    assert!(location.starts_with("http://localhost:8787/callback?code=code-"));
    assert!(location.ends_with("&state=s1"));
    let code = location
        .split("code=")
        .nth(1)
        .and_then(|value| value.split('&').next())
        .expect("a code");

    let token = provider.request(
        "POST",
        "/token",
        &[("Content-Type", "application/x-www-form-urlencoded")],
        Some(&format!("code={code}&client_id=warrior-local")),
    );
    assert_eq!(token.status, 200);
    let body: serde_json::Value = serde_json::from_str(&token.body).unwrap();
    let access_token = body["access_token"].as_str().unwrap();
    assert!(access_token.starts_with("token-"));
    let id_token = body["id_token"].as_str().unwrap();
    let payload = id_token.split('.').nth(1).expect("a JWT payload");
    let payload: serde_json::Value =
        serde_json::from_slice(&base64url::decode(payload).expect("base64url payload")).unwrap();
    assert_eq!(payload["sub"], "alice");
    assert_eq!(payload["aud"], "warrior-local");
    assert_eq!(payload["nonce"], "n1");

    let userinfo = provider.request(
        "GET",
        "/userinfo",
        &[("Authorization", &format!("Bearer {access_token}"))],
        None,
    );
    assert_eq!(userinfo.status, 200);
    let userinfo: serde_json::Value = serde_json::from_str(&userinfo.body).unwrap();
    assert_eq!(userinfo["sub"], "alice");
    assert_eq!(userinfo["email"], "alice@example.test");

    let unauthorized = provider.request("GET", "/userinfo", &[], None);
    assert_eq!(unauthorized.status, 401);
    assert_eq!(provider.origin(), "http://localhost:8789");
}
