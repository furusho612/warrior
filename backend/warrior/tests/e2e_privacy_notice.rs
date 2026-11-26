//! AI のプライバシー通知を、評価、記録、分析、マイページで確認する E2E。
//!
//! 対応する要求: `REQ-NFR-016`。

mod support;

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8882;
const PROVIDER_PORT: u16 = 8883;

#[tokio::test]
async fn the_ai_privacy_notice_is_visible_on_the_ai_screens_and_can_be_expanded() {
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

    driver
        .goto(format!("{}/evaluations", server.origin()))
        .await
        .expect("the evaluations must open");
    check_notice(&driver, true).await;

    driver
        .goto(format!("{}/analysis", server.origin()))
        .await
        .expect("the analysis must open");
    check_notice(&driver, false).await;

    driver
        .goto(format!("{}/mypage", server.origin()))
        .await
        .expect("the mypage must open");
    check_notice(&driver, false).await;

    driver
        .goto(server.origin())
        .await
        .expect("the home must open");
    browser::wait_for_attribute(
        &driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
    browser::click(&driver, "[data-start]").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    check_notice(&driver, false).await;
    browser::click(&driver, "[data-finish]").await;
    browser::wait_for_route(&driver, "/summary/:training_id").await;
    browser::wait_for_present(&driver, "[data-summary]").await;
    check_notice(&driver, false).await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=privacy", provider.origin()))
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

async fn check_notice(driver: &WebDriver, expand: bool) {
    browser::wait_for_present(driver, "[data-ai-notice]").await;
    browser::wait_for_text(driver, "アカウントの識別子").await;
    if expand {
        browser::click(driver, "[data-ai-notice-toggle]").await;
        browser::wait_for_present(driver, "[data-ai-notice-details]").await;
        browser::wait_for_text(driver, "Cloudflare").await;
        browser::wait_for_text(driver, "アカウントの削除まで保存").await;
    }
}
