//! 独自種目を追加し、一覧と詳細に表示され、記録の選択で使えることの E2E。
//!
//! 対応する要求: `REQ-LIB-005`。
//! 種目ライブラリの追加の入力から独自種目を作り、検索した一覧と詳細を開き、
//! トレーニングへ追加する種目の選択に現れることを確かめる。

mod support;

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8874;
const PROVIDER_PORT: u16 = 8875;
const CUSTOM_NAME: &str = "E2E ケーブルプレス";
const CUSTOM_KANA: &str = "いーつーいーけーぶるぷれす";

#[tokio::test]
async fn adds_a_custom_exercise_and_uses_it_in_the_record() {
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
            ("LIBRARY_PAGE_SIZE", "5"),
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
        .goto(format!("{}/exercises", server.origin()))
        .await
        .expect("the library must open");
    browser::wait_for_route(&driver, "/exercises").await;
    browser::wait_for_attribute(&driver, "[data-library]", "data-library-loaded", "true").await;

    browser::click(&driver, "[data-library-add]").await;
    browser::wait_for_present(&driver, "[data-exercise-form]").await;
    browser::set_input(&driver, "[data-exercise-form-input=\"name\"]", CUSTOM_NAME).await;
    browser::set_input(&driver, "[data-exercise-form-input=\"kana\"]", CUSTOM_KANA).await;
    browser::click(
        &driver,
        "[data-exercise-form-primary=\"pectoralis_major_middle\"]",
    )
    .await;
    browser::click(&driver, "[data-exercise-form-save]").await;
    browser::wait_for_absent(&driver, "[data-exercise-form]").await;

    browser::set_input(&driver, "[data-library-query]", "E2E").await;
    browser::wait_for_attribute(&driver, "[data-library]", "data-library-count", "1").await;
    let exercise_id =
        browser::attribute(&driver, "[data-library-exercise]", "data-exercise-id").await;
    assert!(
        !exercise_id.is_empty(),
        "the custom exercise must have an id"
    );
    browser::wait_for_text(&driver, CUSTOM_NAME).await;

    browser::click(&driver, "[data-library-open]").await;
    browser::wait_for_route(&driver, "/exercises/:exercise_id").await;
    browser::wait_for_attribute(
        &driver,
        "[data-exercise-detail]",
        "data-exercise-loaded",
        "true",
    )
    .await;
    browser::wait_for_text(&driver, CUSTOM_NAME).await;

    start_training(&driver, &server).await;
    browser::click(&driver, "[data-add-exercise]").await;
    browser::wait_for_route(&driver, "/record/:training_id/exercises").await;
    let selector = format!("[data-exercise-choose][data-exercise-id=\"{exercise_id}\"]");
    browser::wait_for_present(&driver, &selector).await;
    browser::click(&driver, &selector).await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    let row = format!("[data-exercise-row][data-exercise-id=\"{exercise_id}\"]");
    browser::wait_for_present(&driver, &row).await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=custom-exercise",
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

async fn start_training(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the home must open");
    browser::wait_for_route(driver, "/").await;
    browser::wait_for_present(driver, "[data-start]").await;
    browser::click(driver, "[data-start]").await;
    browser::wait_for_route(driver, "/record/:training_id").await;
}
