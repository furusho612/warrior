//! 固定のメニューの最適化を表示し、変更を承認してルーティンへ反映する E2E。
//!
//! 対応する要求: `REQ-AI-010`、`REQ-AI-011`、`REQ-AI-012`。

mod support;

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8888;
const PROVIDER_PORT: u16 = 8889;
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7d10";
const ROUTINE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7d11";
const MENU_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7d20";

#[tokio::test]
async fn a_fixed_menu_change_is_approved_and_applied_to_the_routine() {
    let browser = Browser::from_env();
    let origin = format!("http://localhost:{SERVER_PORT}");
    let provider_origin = format!("http://localhost:{PROVIDER_PORT}");
    let server = DevServer::start_with_vars(
        SERVER_PORT,
        &[
            ("ORIGIN", origin.as_str()),
            (
                "GOOGLE_OAUTH_AUTHORIZE_URL",
                &format!("{provider_origin}/authorize"),
            ),
            ("GOOGLE_OAUTH_ISSUER", provider_origin.as_str()),
            (
                "GOOGLE_OAUTH_TOKEN_URL",
                &format!("{provider_origin}/token"),
            ),
            (
                "GOOGLE_OAUTH_USERINFO_URL",
                &format!("{provider_origin}/userinfo"),
            ),
            (
                "GOOGLE_OAUTH_REDIRECT_URI",
                &format!("{origin}/api/auth/google/callback"),
            ),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    login(&driver, &server, &provider).await;
    let user_id = current_user_id(&driver).await;
    seed_routine(&server, &user_id);
    seed_menu(&server, &user_id);

    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    browser::wait_for_present(&driver, "[data-menu-optimization-changes]").await;
    browser::wait_for_text(&driver, "胸のセット数が週 8 セットのため").await;
    browser::click(&driver, "[data-menu-change-approve=\"0\"]").await;
    wait_for_set_count(&server, 4);

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=menu", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(driver, server).await;
    browser::wait_for_route(driver, "/").await;
    browser::wait_for_attribute(
        driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
}

fn seed_routine(server: &DevServer, user_id: &str) {
    let snapshot = json!({
        "exercise_id": BENCH,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    for sql in [
        format!(
            "INSERT INTO routines (id, user_id, name, position, created_at, updated_at) \
             VALUES ('{ROUTINE_ID}', '{user_id}', '胸の日', 1, '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercises (id, user_id, routine_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{ROUTINE_EXERCISE_ID}', '{user_id}', \
             '{ROUTINE_ID}', '{BENCH}', '{snapshot}', 1, '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercise_targets (routine_exercise_id, user_id, set_count, \
             target_reps, rest_seconds, created_at, updated_at) VALUES \
             ('{ROUTINE_EXERCISE_ID}', '{user_id}', 3, 8, 90, '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the routine must be seeded");
    }
}

fn seed_menu(server: &DevServer, user_id: &str) {
    let output_key = format!("users/{user_id}/ai/{MENU_ID}/output.json");
    for sql in [
        format!(
            "INSERT INTO ai_generation (id, user_id, kind, training_id, exercise_id, period_start, \
             period_end, unique_key, status, attempts, output_id, created_at, updated_at) VALUES \
             ('{MENU_ID}', '{user_id}', 'menu_optimization', NULL, NULL, '2026-11-04', \
             '2026-11-10', 'menu:2026-11-10', 'generated', 1, '{MENU_ID}', \
             '2026-11-10T10:00:00Z', '2026-11-10T10:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_outputs (id, user_id, kind, training_id, exercise_id, model, \
             prompt_version, input_key, output_key, created_at) VALUES ('{MENU_ID}', '{user_id}', \
             'menu_optimization', NULL, NULL, 'test-model', 'v1', \
             'users/{user_id}/ai/{MENU_ID}/input.json', '{output_key}', '2026-11-10T10:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the menu must be seeded");
    }
    let output = json!({
        "model": "test-model",
        "prompt_version": "v1",
        "output": {
            "menu_changes": [{"kind": "set_increase", "exercise_id": BENCH, "sets": 4,
                "reason": "胸のセット数が週 8 セットのため"}]
        }
    });
    server
        .put_r2_object(&output_key, output.to_string().as_bytes())
        .expect("the menu output must be stored");
}

fn wait_for_set_count(server: &DevServer, expected: i64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let row = server
            .execute_d1_json(&format!(
                "SELECT set_count FROM routine_exercise_targets \
                 WHERE routine_exercise_id = '{ROUTINE_EXERCISE_ID}'"
            ))
            .expect("the target must be readable");
        let count = row
            .get(0)
            .and_then(|entry| entry.get("results"))
            .and_then(|rows| rows.get(0))
            .and_then(|row| row.get("set_count"))
            .and_then(serde_json::Value::as_i64);
        if count == Some(expected) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the set count did not become {expected}: {row}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
