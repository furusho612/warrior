//! 固定の AI の出力を投入し、日の評価が一覧に表示されることを確かめる E2E。
//!
//! 対応する要求: `REQ-AI-001`、`REQ-AI-003`、`REQ-AI-004`、`REQ-AI-013`。

mod support;

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8884;
const PROVIDER_PORT: u16 = 8885;
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7b10";
const EVALUATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7b20";

#[tokio::test]
async fn a_fixed_daily_evaluation_is_shown_with_its_evidence() {
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
    seed_evaluation(&server, &user_id);

    driver
        .goto(format!("{}/evaluations", server.origin()))
        .await
        .expect("the evaluations must open");
    browser::wait_for_present(&driver, "[data-ai-evaluation]").await;
    browser::wait_for_attribute(
        &driver,
        "[data-ai-evaluation]",
        "data-ai-evaluation-status",
        "generated",
    )
    .await;
    browser::wait_for_text(&driver, "70 kg で 8 回できた").await;
    browser::wait_for_text(&driver, "70 kg を 8 回").await;
    browser::click(&driver, "[data-ai-redo]").await;
    wait_for_status(&server, "pending");

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=daily-evaluation",
            provider.origin()
        ))
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

fn wait_for_status(server: &DevServer, expected: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let status = server
            .execute_d1_json(&format!(
                "SELECT status FROM ai_generation WHERE id = '{EVALUATION_ID}'"
            ))
            .expect("the status must be readable");
        if status
            .get(0)
            .and_then(|entry| entry.get("results"))
            .and_then(|rows| rows.get(0))
            .and_then(|row| row.get("status"))
            .and_then(serde_json::Value::as_str)
            == Some(expected)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the job did not become {expected}: {status}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn seed_evaluation(server: &DevServer, user_id: &str) {
    let output_key = format!("users/{user_id}/ai/{EVALUATION_ID}/output.json");
    for sql in [
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, finished_at, memo, \
             source_kind, created_at, updated_at) VALUES ('{TRAINING_ID}', '{user_id}', \
             '2026-11-10', '2026-11-10T09:00:00Z', '2026-11-10T10:00:00Z', '', 'empty', \
             '2026-11-10T09:00:00Z', '2026-11-10T09:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_generation (id, user_id, kind, training_id, exercise_id, period_start, \
             period_end, unique_key, status, attempts, output_id, created_at, updated_at) VALUES \
             ('{EVALUATION_ID}', '{user_id}', 'daily_evaluation', '{TRAINING_ID}', NULL, NULL, \
             NULL, 'daily:2026-11-10', 'generated', 1, '{EVALUATION_ID}', \
             '2026-11-10T10:00:00Z', '2026-11-10T10:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_outputs (id, user_id, kind, training_id, exercise_id, model, \
             prompt_version, input_key, output_key, created_at) VALUES ('{EVALUATION_ID}', \
             '{user_id}', 'daily_evaluation', '{TRAINING_ID}', NULL, 'test-model', 'v1', \
             'users/{user_id}/ai/{EVALUATION_ID}/input.json', '{output_key}', \
             '2026-11-10T10:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the evaluation must be seeded");
    }
    let output = json!({
        "model": "test-model",
        "prompt_version": "v1",
        "output": {
            "evaluation": {
                "good": [{"text": "70 kg で 8 回できた", "evidence": "70 kg を 8 回",
                    "certainty": "assertion"}],
                "improvements": [],
                "next_actions": []
            }
        }
    });
    server
        .put_r2_object(&output_key, output.to_string().as_bytes())
        .expect("the output must be stored");
}
