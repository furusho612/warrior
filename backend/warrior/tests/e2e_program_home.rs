//! プログラムを選び、ホームから開始し、終了すると次の日へ進む E2E。
//!
//! 対応する要求: `REQ-PRG-002`、`REQ-PRG-003`、`REQ-PRG-004`、`REQ-PRG-005`、`REQ-PRG-019`、`REQ-PRG-020`。

mod support;

use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8876;
const PROVIDER_PORT: u16 = 8877;
const PROGRAM_ID: &str = "program-hypertrophy-intermediate";

#[tokio::test]
async fn a_program_is_selected_from_routines_started_on_home_and_advances_after_finishing() {
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
    seed_program_profile(&server, &user_id);

    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    browser::wait_for_present(&driver, "[data-programs]").await;
    browser::click(&driver, &format!("[data-program-select=\"{PROGRAM_ID}\"]")).await;
    browser::wait_for_present(&driver, "[data-current-program]").await;
    browser::wait_for_text(&driver, "1 / 4").await;

    driver
        .goto(server.origin())
        .await
        .expect("the home must open");
    browser::wait_for_present(&driver, "[data-home-program]").await;
    let shown = browser::attribute(&driver, "[data-home-program]", "data-home-program").await;
    assert_eq!(shown, PROGRAM_ID);
    browser::wait_for_text(&driver, "1 / 4").await;
    browser::click(&driver, "[data-home-program-start]").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_present(&driver, "[data-session-id]").await;
    let training_id = browser::attribute(&driver, "[data-session-id]", "data-session-id").await;

    browser::click(&driver, "[data-finish]").await;
    browser::wait_for_route(&driver, "/summary/:training_id").await;
    wait_for_finished(&server, &training_id);

    driver
        .goto(server.origin())
        .await
        .expect("the home must reopen");
    browser::wait_for_present(&driver, "[data-home-program]").await;
    browser::wait_for_text(&driver, "2 / 4").await;

    let routines = first_row(
        &server,
        &format!(
            "SELECT COUNT(*) AS count FROM routines WHERE user_id = '{user_id}' \
             AND source_program_day_id IS NOT NULL"
        ),
    );
    assert_eq!(routines["count"], 4);

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=program-home", provider.origin()))
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

fn seed_program_profile(server: &DevServer, user_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
             training_experience, goal, training_days_per_week, activity_level, created_at, \
             updated_at) VALUES ('{user_id}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', \
             'hypertrophy', 4, 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}

fn wait_for_finished(server: &DevServer, training_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let row = first_row(
            server,
            &format!("SELECT finished_at FROM training_sessions WHERE id = '{training_id}'"),
        );
        if row["finished_at"] != serde_json::Value::Null {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the training was not finished: {row}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn first_row(server: &DevServer, sql: &str) -> serde_json::Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .expect("a row")
}
