//! 目標カロリーと PFC の表示、ダイエットの目安、PFC の調整と既定値への復帰の E2E。
//!
//! 対応する要求: `REQ-PRG-010`、`REQ-PRG-011`、`REQ-PRG-013`、`REQ-PRG-014`、`REQ-PRG-015`、`REQ-PRG-016`、`REQ-PRG-018`、`REQ-MYP-002`。

mod support;

use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8880;
const PROVIDER_PORT: u16 = 8881;

#[tokio::test]
async fn the_nutrition_target_and_the_macro_settings_are_shown_and_reset() {
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
    seed_profile(&server, &user_id);

    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    browser::wait_for_present(&driver, "[data-nutrition-panel]").await;
    browser::wait_for_attribute(
        &driver,
        "[data-nutrition-panel]",
        "data-nutrition-loaded",
        "true",
    )
    .await;
    browser::wait_for_present(&driver, "[data-nutrition-figures]").await;
    browser::wait_for_present(&driver, "[data-nutrition-diet-range]").await;
    browser::wait_for_text(&driver, "0.5% から 1%").await;
    browser::wait_for_text(&driver, "医療上や栄養指導上の助言ではありません").await;

    driver
        .goto(format!("{}/mypage", server.origin()))
        .await
        .expect("the mypage must open");
    browser::wait_for_present(&driver, "[data-pfc-protein]").await;
    browser::set_input(&driver, "[data-pfc-protein]", "2.2").await;
    browser::set_input(&driver, "[data-pfc-fat]", "25").await;
    browser::wait_for_value(&driver, "[data-pfc-protein]", "2.2").await;
    browser::click(&driver, "[data-pfc-reset]").await;
    browser::wait_for_value(&driver, "[data-pfc-protein]", "2.0").await;
    browser::wait_for_value(&driver, "[data-pfc-fat]", "20").await;
    browser::wait_for_present(
        &driver,
        "[data-nutrition-panel][data-nutrition-loaded=\"true\"]",
    )
    .await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=nutrition", provider.origin()))
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

fn seed_profile(server: &DevServer, user_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
             training_experience, goal, training_days_per_week, activity_level, created_at, \
             updated_at) VALUES ('{user_id}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', \
             'diet', 4, 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}
