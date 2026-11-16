//! 固定の次回セットの提案を表示し、採用し、修正して保存する E2E。
//!
//! 対応する要求: `REQ-AI-006`、`REQ-AI-007`、`REQ-AI-008`、`REQ-AI-018`。

mod support;

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8886;
const PROVIDER_PORT: u16 = 8887;
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const PROPOSAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7c10";

#[tokio::test]
async fn a_fixed_proposal_is_applied_and_edited_before_the_next_set() {
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
    browser::click(&driver, "[data-start]").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_present(&driver, "[data-session-id]").await;
    let training_id = browser::attribute(&driver, "[data-session-id]", "data-session-id").await;

    browser::click(&driver, "[data-add-exercise]").await;
    browser::wait_for_route(&driver, "/record/:training_id/exercises").await;
    browser::click(
        &driver,
        &format!("[data-exercise-choose][data-exercise-id=\"{BENCH}\"]"),
    )
    .await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_present(
        &driver,
        &format!("[data-exercise-row][data-exercise-id=\"{BENCH}\"]"),
    )
    .await;
    seed_proposal(&server, &user_id, &training_id);

    driver
        .goto(format!("{}/record/{training_id}", server.origin()))
        .await
        .expect("the record must reopen");
    browser::wait_for_present(&driver, "[data-ai-proposal]").await;
    browser::wait_for_text(
        &driver,
        "前回は全セットで目標回数に達したため 2.5 kg 増やす",
    )
    .await;
    browser::click(&driver, "[data-ai-proposal-apply]").await;
    browser::wait_for_present(&driver, "[data-set-row]").await;

    browser::click(&driver, "[data-ai-proposal-edit]").await;
    browser::wait_for_present(&driver, "[data-ai-proposal-editor]").await;
    browser::set_input(&driver, "[data-ai-proposal-weight]", "75").await;
    browser::set_input(&driver, "[data-ai-proposal-sets-edit]", "4").await;
    browser::set_input(
        &driver,
        "[data-ai-proposal-reason-edit]",
        "前回の記録に合わせて修正する",
    )
    .await;
    browser::click(&driver, "[data-ai-proposal-save]").await;
    browser::wait_for_absent(&driver, "[data-ai-proposal-editor]").await;
    driver
        .goto(format!("{}/record/{training_id}", server.origin()))
        .await
        .expect("the record must reopen after saving");
    browser::wait_for_present(&driver, "[data-ai-proposal]").await;
    browser::wait_for_text(&driver, "前回の記録に合わせて修正する").await;
    browser::wait_for_text(&driver, "75 kg").await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=next-set", provider.origin()))
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

fn seed_proposal(server: &DevServer, user_id: &str, training_id: &str) {
    let output_key = format!("users/{user_id}/ai/{PROPOSAL_ID}/output.json");
    for sql in [
        format!(
            "INSERT INTO ai_generation (id, user_id, kind, training_id, exercise_id, period_start, \
             period_end, unique_key, status, attempts, output_id, created_at, updated_at) VALUES \
             ('{PROPOSAL_ID}', '{user_id}', 'set_proposal', '{training_id}', '{BENCH}', NULL, \
             NULL, 'proposal:{training_id}:{BENCH}', 'generated', 1, '{PROPOSAL_ID}', \
             '2026-11-10T10:00:00Z', '2026-11-10T10:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_outputs (id, user_id, kind, training_id, exercise_id, model, \
             prompt_version, input_key, output_key, created_at) VALUES ('{PROPOSAL_ID}', \
             '{user_id}', 'set_proposal', '{training_id}', '{BENCH}', 'test-model', 'v1', \
             'users/{user_id}/ai/{PROPOSAL_ID}/input.json', '{output_key}', \
             '2026-11-10T10:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the proposal must be seeded");
    }
    let output = json!({
        "model": "test-model",
        "prompt_version": "v1",
        "output": {
            "proposals": [{"exercise_id": BENCH, "weight_kg": 72.5, "reps": 8, "sets": 3,
                "reason": "前回は全セットで目標回数に達したため 2.5 kg 増やす"}]
        }
    });
    server
        .put_r2_object(&output_key, output.to_string().as_bytes())
        .expect("the proposal output must be stored");
}
