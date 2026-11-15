//! 利用者共通の設定の結合テスト。
//!
//! 対応する要求: `REQ-MYP-002`、`REQ-MYP-003`、`REQ-MYP-005`、`REQ-NFR-019`。
//! 未登録の利用者には既定値を返し、更新は利用者ごとに 1 行を置き換えること、
//! 範囲外の値と未知の項目を拒否することを確かめる。

mod support;

use serde_json::{json, Value};
use support::{DevServer, HttpResponse};
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::nutrition::MacroTarget;
use warrior_core::settings::{DisplayMode, Settings};

const ORIGIN: &str = "http://localhost:8787";
const PORT: u16 = 8890;
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a01";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a02";
const NOW: &str = "2026-11-12T00:00:00Z";

#[test]
fn the_settings_round_trip_per_user_and_reject_invalid_values() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let cookie = seed_user(&server);

    assert_eq!(
        server.request("GET", "/api/settings", &[]).status,
        401,
        "the settings require authentication"
    );
    assert_eq!(get_settings(&server, &cookie), Settings::default());

    let without_origin = server.request_with_body(
        "PUT",
        "/api/settings",
        &[("Cookie", cookie.as_str())],
        &serde_json::to_string(&Settings::default()).expect("the default settings"),
    );
    assert_eq!(without_origin.status, 403);

    let unauthenticated = server.request_with_body(
        "PUT",
        "/api/settings",
        &[("Origin", ORIGIN)],
        &serde_json::to_string(&Settings::default()).expect("the default settings"),
    );
    assert_eq!(unauthenticated.status, 401);

    let updated = Settings {
        rest_default_seconds: 120,
        display_mode: DisplayMode::Dark,
        macro_target: MacroTarget {
            protein_g_per_kg: 1.8,
            fat_ratio: 0.25,
        },
    };
    let response = put_settings(&server, &cookie, &serde_json::to_value(updated).unwrap());
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(get_settings(&server, &cookie), updated);
    assert_eq!(setting_count(&server, USER_ID), 1);

    for (field, value) in [
        ("rest_default_seconds", json!(0)),
        ("rest_default_seconds", json!(3601)),
        ("display_mode", json!("other")),
        (
            "macro_target",
            json!({"protein_g_per_kg": 1.5, "fat_ratio": 0.2}),
        ),
        (
            "macro_target",
            json!({"protein_g_per_kg": 2.0, "fat_ratio": 1.1}),
        ),
    ] {
        let mut input = serde_json::to_value(updated).expect("serializable settings");
        input[field] = value;
        let response = put_settings(&server, &cookie, &input);
        assert_eq!(
            response.status, 400,
            "the value {field} must be rejected: {}",
            response.body
        );
        assert_eq!(
            get_settings(&server, &cookie),
            updated,
            "a rejected update must not change the stored settings"
        );
    }

    let mut unknown = serde_json::to_value(updated).expect("serializable settings");
    unknown["unknown"] = json!(true);
    assert_eq!(put_settings(&server, &cookie, &unknown).status, 400);

    for seconds in [1, 3600] {
        let input = Settings {
            rest_default_seconds: seconds,
            ..updated
        };
        let response = put_settings(&server, &cookie, &serde_json::to_value(input).unwrap());
        assert_eq!(response.status, 200, "{}", response.body);
        assert_eq!(get_settings(&server, &cookie), input);
    }

    let other_cookie = seed_other_user(&server);
    assert_eq!(get_settings(&server, &other_cookie), Settings::default());
    assert_eq!(setting_count(&server, OTHER_USER_ID), 0);
    let other = Settings {
        rest_default_seconds: 30,
        display_mode: DisplayMode::Light,
        ..Settings::default()
    };
    assert_eq!(
        put_settings(
            &server,
            &other_cookie,
            &serde_json::to_value(other).unwrap()
        )
        .status,
        200
    );
    assert_eq!(get_settings(&server, &other_cookie), other);
    assert_eq!(
        get_settings(&server, &cookie),
        Settings {
            rest_default_seconds: 3600,
            ..updated
        },
        "another user's update must not change the first user's settings"
    );
}

fn seed_user(server: &DevServer) -> String {
    let token = base64url::encode(&[41u8; 32]);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-settings', 'settings@example.test', '設定の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5a03', '{USER_ID}', '{}', '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(&token)
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the settings fixture must be seeded");
    }
    format!("warrior_session={token}")
}

fn seed_other_user(server: &DevServer) -> String {
    let token = base64url::encode(&[42u8; 32]);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-other-settings', 'other-settings@example.test', '別の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5a04', '{OTHER_USER_ID}', '{}', '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(&token)
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the other settings fixture must be seeded");
    }
    format!("warrior_session={token}")
}

fn get_settings(server: &DevServer, cookie: &str) -> Settings {
    let response = server.request("GET", "/api/settings", &[("Cookie", cookie)]);
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the settings response")
}

fn put_settings(server: &DevServer, cookie: &str, body: &Value) -> HttpResponse {
    server.request_with_body(
        "PUT",
        "/api/settings",
        &[("Cookie", cookie), ("Origin", ORIGIN)],
        &body.to_string(),
    )
}

fn setting_count(server: &DevServer, user_id: &str) -> i64 {
    server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM user_settings WHERE user_id = '{user_id}'"
        ))
        .expect("the count must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(Value::as_i64)
        .expect("a count")
}
