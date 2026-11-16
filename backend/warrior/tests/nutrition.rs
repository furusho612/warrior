//! 目標カロリーと PFC の取得と調整の結合テスト。
//!
//! 対応する要求: `REQ-PRG-010`、`REQ-PRG-013`、`REQ-PRG-015`、`REQ-PRG-016`、`REQ-MYP-002`。

mod support;

use serde_json::{json, Value};
use support::record_common::{cookie, seed_auth_session, seed_terminal, seed_user};
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8840;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5d01";
const SESSION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5d10";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5d20";
const SESSION_SEED: u8 = 101;
const TERMINAL_SEED: u8 = 102;

#[test]
fn the_nutrition_target_is_calculated_and_macro_adjustments_are_validated() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed(&server);
    let cookie_a = cookie(
        &base64url::encode(&[SESSION_SEED; 32]),
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );

    let nutrition = get(&server, "/api/nutrition", &cookie_a);
    assert_eq!(nutrition.status, 200, "{}", nutrition.body);
    let nutrition: Value = serde_json::from_str(&nutrition.body).expect("the nutrition is JSON");
    assert!(
        nutrition["result"]["target_kcal"].as_f64().unwrap_or(0.0) > 0.0,
        "{nutrition}"
    );
    assert_eq!(nutrition["missing_fields"], json!([]));
    assert!(nutrition["display"]["protein_g"].as_i64().unwrap_or(0) > 0);
    assert_eq!(nutrition["macro_target"]["protein_g_per_kg"], 2.0);
    assert_eq!(nutrition["macro_target"]["fat_ratio"], 0.20);

    let updated = put(
        &server,
        "/api/nutrition",
        &cookie_a,
        &json!({ "protein_g_per_kg": 2.2, "fat_ratio": 0.25 }),
    );
    assert_eq!(updated.status, 200, "{}", updated.body);
    let updated: Value = serde_json::from_str(&updated.body).expect("the nutrition is JSON");
    assert_eq!(updated["macro_target"]["protein_g_per_kg"], 2.2);
    assert_eq!(updated["macro_target"]["fat_ratio"], 0.25);

    let read_back = get(&server, "/api/nutrition", &cookie_a);
    let read_back: Value = serde_json::from_str(&read_back.body).expect("the nutrition is JSON");
    assert_eq!(read_back["macro_target"]["protein_g_per_kg"], 2.2);
    assert_eq!(read_back["macro_target"]["fat_ratio"], 0.25);

    let invalid = put(
        &server,
        "/api/nutrition",
        &cookie_a,
        &json!({ "protein_g_per_kg": 5.0, "fat_ratio": 0.5 }),
    );
    assert_eq!(invalid.status, 400, "{}", invalid.body);

    let excessive_fat = put(
        &server,
        "/api/nutrition",
        &cookie_a,
        &json!({ "protein_g_per_kg": 2.2, "fat_ratio": 0.9 }),
    );
    assert_eq!(excessive_fat.status, 400, "{}", excessive_fat.body);
}

fn seed(server: &DevServer) {
    seed_user(
        server,
        USER_ID,
        "nutrition-user",
        "nutrition@example.test",
        "利用者",
    );
    seed_auth_session(
        server,
        SESSION_ID,
        USER_ID,
        &base64url::encode(&[SESSION_SEED; 32]),
    );
    seed_terminal(
        server,
        TERMINAL_ID,
        USER_ID,
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
             training_experience, goal, training_days_per_week, activity_level, created_at, \
             updated_at) VALUES ('{USER_ID}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', \
             'diet', 4, 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}

fn get(server: &DevServer, path: &str, cookie: &str) -> support::HttpResponse {
    server.request("GET", path, &[("Origin", ORIGIN), ("Cookie", cookie)])
}

fn put(server: &DevServer, path: &str, cookie: &str, body: &Value) -> support::HttpResponse {
    server.request_with_body(
        "PUT",
        path,
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        &body.to_string(),
    )
}
