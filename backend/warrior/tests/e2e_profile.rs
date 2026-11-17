//! 身体情報と目的の設定の E2E。
//!
//! 対応する要求: `REQ-PRF-001`、`REQ-PRF-002`、`REQ-PRF-003`、`REQ-PRF-004`、
//! `REQ-PRF-005`、`REQ-LIB-004`、`REQ-MYP-001`、`REQ-NFR-018`、`REQ-NFR-019`。
//! 初回の登録、更新、目的と日数の変更、注意書きの表示と非表示、身体情報を飛ばした記録、
//! マイページからの登録、身体情報の入力を促す案内を実ブラウザで確かめる。
//! Safari は対象外とする。Worker は 8820、テスト専用の OIDC の提供者は 8821 を使う。

mod support;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8820;
const PROVIDER_PORT: u16 = 8821;
const SUB_A: &str = "profile-a";
const SUB_B: &str = "profile-b";
const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(100);

#[tokio::test]
async fn registers_updates_and_skips_the_body_profile() {
    let origin = format!("http://localhost:{SERVER_PORT}");
    let provider_origin = format!("http://localhost:{PROVIDER_PORT}");
    let authorize_url = format!("{provider_origin}/authorize");
    let token_url = format!("{provider_origin}/token");
    let userinfo_url = format!("{provider_origin}/userinfo");
    let redirect_uri = format!("{origin}/api/auth/google/callback");
    let server = DevServer::start_with_vars(
        SERVER_PORT,
        &[
            ("ORIGIN", origin.as_str()),
            ("GOOGLE_OAUTH_AUTHORIZE_URL", authorize_url.as_str()),
            ("GOOGLE_OAUTH_ISSUER", provider_origin.as_str()),
            ("GOOGLE_OAUTH_TOKEN_URL", token_url.as_str()),
            ("GOOGLE_OAUTH_USERINFO_URL", userinfo_url.as_str()),
            ("GOOGLE_OAUTH_REDIRECT_URI", redirect_uri.as_str()),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    register_from_onboarding(&driver, &server, &provider).await;
    check_mypage_load_failure(&driver, &server).await;
    update_and_check_the_limits(&driver, &server).await;
    browser::logout(&driver).await;
    skip_and_register_from_mypage(&driver, &server, &provider).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn register_from_onboarding(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    login(driver, server, provider, SUB_A).await;
    wait_for_onboarding(driver, true).await;
    assert!(
        !is_enabled(driver, "[data-onboarding-save]").await,
        "the empty onboarding form must not be saved"
    );

    let birth_date = birth_date_years_ago(10);
    set_input(driver, "[data-profile-field=\"height\"] input", "170").await;
    set_input(driver, "[data-profile-field=\"weight\"] input", "60").await;
    set_birth_date(driver, &birth_date).await;
    click(driver, "[data-profile-sex] [data-choice-value=\"female\"]").await;
    click(
        driver,
        "[data-profile-experience] [data-choice-value=\"beginner\"]",
    )
    .await;
    click(driver, "[data-onboarding-save]").await;

    browser::wait_for_attribute(driver, "[data-onboarding]", "data-onboarding-saved", "true").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-safety]",
        "data-profile-safety",
        "true",
    )
    .await;
    browser::wait_for_text(driver, "トレーニングと食事の内容に注意").await;
    browser::wait_for_text(driver, "専門家に相談").await;
    click(driver, "[data-onboarding-done]").await;
    wait_for_selector_absent(driver, "[data-onboarding]").await;

    open_mypage(driver, server).await;
    browser::wait_for_value(driver, "[data-profile-field=\"height\"] input", "170").await;
    browser::wait_for_value(driver, "[data-profile-field=\"weight\"] input", "60").await;
    wait_birth_date(driver, &birth_date).await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-sex] [data-choice-value=\"female\"]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-experience] [data-choice-value=\"beginner\"]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-safety]",
        "data-profile-safety",
        "true",
    )
    .await;
}

async fn check_mypage_load_failure(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(server.origin())
        .await
        .expect("the home must open");
    browser::wait_for_route(driver, "/").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;

    browser::offline(driver, true).await;
    click(driver, "a[href=\"/mypage\"]").await;
    browser::wait_for_route(driver, "/mypage").await;
    browser::wait_for_attribute(driver, "[data-profile-retry]", "data-profile-retry", "true").await;
    assert_eq!(
        browser::attribute(driver, "[data-profile-settings]", "data-profile-loaded").await,
        "false",
        "a failed load must leave the profile not loaded"
    );
    assert!(
        !is_enabled(driver, "[data-profile-save]").await,
        "the save must be disabled after a failed load"
    );

    browser::offline(driver, false).await;
    click(driver, "[data-profile-retry]").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-settings]",
        "data-profile-loaded",
        "true",
    )
    .await;
    assert!(
        is_enabled(driver, "[data-profile-save]").await,
        "the save must be enabled after the retry"
    );
}

async fn update_and_check_the_limits(driver: &WebDriver, server: &DevServer) {
    let adult_birth_date = birth_date_years_ago(30);
    set_input(driver, "[data-profile-field=\"height\"] input", "175").await;
    set_input(driver, "[data-profile-field=\"weight\"] input", "70").await;
    set_birth_date(driver, &adult_birth_date).await;
    click(
        driver,
        "[data-profile-experience] [data-choice-value=\"intermediate\"]",
    )
    .await;
    click(driver, "[data-profile-save]").await;
    wait_for_save(driver, "1").await;
    wait_for_attribute_absent(driver, "[data-profile-safety]", "data-profile-safety").await;

    click(driver, "[data-profile-goal] [data-choice-value=\"diet\"]").await;
    click(driver, "[data-profile-days] [data-choice-value=\"0\"]").await;
    click(
        driver,
        "[data-profile-activity] [data-choice-value=\"high\"]",
    )
    .await;
    click(driver, "[data-profile-save]").await;
    wait_for_save(driver, "2").await;

    open_mypage(driver, server).await;
    browser::wait_for_value(driver, "[data-profile-field=\"height\"] input", "175").await;
    browser::wait_for_value(driver, "[data-profile-field=\"weight\"] input", "70").await;
    wait_birth_date(driver, &adult_birth_date).await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-goal] [data-choice-value=\"diet\"]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-days] [data-choice-value=\"0\"]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-activity] [data-choice-value=\"high\"]",
        "aria-pressed",
        "true",
    )
    .await;
    wait_for_attribute_absent(driver, "[data-profile-safety]", "data-profile-safety").await;

    set_input(driver, "[data-profile-field=\"height\"] input", "200").await;
    set_input(driver, "[data-profile-field=\"weight\"] input", "73.9").await;
    click(driver, "[data-profile-save]").await;
    wait_for_save(driver, "1").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-safety]",
        "data-profile-safety",
        "true",
    )
    .await;

    set_input(driver, "[data-profile-field=\"weight\"] input", "74").await;
    click(driver, "[data-profile-save]").await;
    wait_for_save(driver, "2").await;
    wait_for_attribute_absent(driver, "[data-profile-safety]", "data-profile-safety").await;

    click(driver, "[data-profile-days] [data-choice-value=\"7\"]").await;
    click(driver, "[data-profile-save]").await;
    wait_for_save(driver, "3").await;

    open_mypage(driver, server).await;
    browser::wait_for_value(driver, "[data-profile-field=\"height\"] input", "200").await;
    browser::wait_for_value(driver, "[data-profile-field=\"weight\"] input", "74").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-days] [data-choice-value=\"7\"]",
        "aria-pressed",
        "true",
    )
    .await;
    wait_for_attribute_absent(driver, "[data-profile-safety]", "data-profile-safety").await;
}

async fn skip_and_register_from_mypage(
    driver: &WebDriver,
    server: &DevServer,
    provider: &TestOidc,
) {
    login(driver, server, provider, SUB_B).await;
    wait_for_onboarding(driver, true).await;
    click(driver, "[data-onboarding-skip]").await;
    wait_for_selector_absent(driver, "[data-onboarding]").await;

    start_and_finish_training(driver, true).await;

    open_mypage(driver, server).await;
    let birth_date = birth_date_years_ago(28);
    set_input(driver, "[data-profile-field=\"height\"] input", "165").await;
    set_input(driver, "[data-profile-field=\"weight\"] input", "58").await;
    set_birth_date(driver, &birth_date).await;
    click(driver, "[data-profile-sex] [data-choice-value=\"other\"]").await;
    click(
        driver,
        "[data-profile-experience] [data-choice-value=\"advanced\"]",
    )
    .await;
    click(
        driver,
        "[data-profile-goal] [data-choice-value=\"strength\"]",
    )
    .await;
    click(driver, "[data-profile-days] [data-choice-value=\"3\"]").await;
    click(
        driver,
        "[data-profile-activity] [data-choice-value=\"normal\"]",
    )
    .await;
    click(driver, "[data-profile-save]").await;
    wait_for_save(driver, "1").await;
    wait_for_attribute_absent(driver, "[data-profile-safety]", "data-profile-safety").await;

    driver
        .goto(server.origin())
        .await
        .expect("the home must open");
    browser::wait_for_route(driver, "/").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    wait_for_selector_absent(driver, "[data-onboarding]").await;
    start_and_finish_training(driver, false).await;
}

async fn start_and_finish_training(driver: &WebDriver, prompt_expected: bool) {
    browser::wait_for_attribute(
        driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
    click(driver, "[data-start]").await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    browser::wait_for_attribute(driver, "[data-session-id]", "data-profile-checked", "true").await;
    if prompt_expected {
        browser::wait_for_attribute(
            driver,
            "[data-profile-prompt]",
            "data-profile-prompt",
            "true",
        )
        .await;
    } else {
        wait_for_selector_absent(driver, "[data-profile-prompt]").await;
    }
    click(driver, "[data-finish]").await;
    browser::wait_for_route(driver, "/summary/:training_id").await;
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc, sub: &str) {
    driver
        .goto(format!("{}/test/user?sub={sub}", provider.origin()))
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

async fn wait_for_onboarding(driver: &WebDriver, visible: bool) {
    browser::wait_for_attribute(
        driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    if visible {
        browser::wait_for_attribute(
            driver,
            "[data-onboarding]",
            "data-onboarding-saved",
            "false",
        )
        .await;
    } else {
        wait_for_selector_absent(driver, "[data-onboarding]").await;
    }
}

async fn open_mypage(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/mypage", server.origin()))
        .await
        .expect("the my page must open");
    browser::wait_for_route(driver, "/mypage").await;
    browser::wait_for_attribute(
        driver,
        "[data-profile-settings]",
        "data-profile-loaded",
        "true",
    )
    .await;
}

async fn set_input(driver: &WebDriver, selector: &str, value: &str) {
    let element = driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the input {selector} must exist: {error}"));
    element
        .clear()
        .await
        .unwrap_or_else(|error| panic!("the input {selector} must be cleared: {error}"));
    element
        .send_keys(value)
        .await
        .unwrap_or_else(|error| panic!("the input {selector} must be typed: {error}"));
}

async fn click(driver: &WebDriver, selector: &str) {
    let element = driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the element {selector} must exist: {error}"));
    element
        .click()
        .await
        .unwrap_or_else(|error| panic!("the element {selector} must be clickable: {error}"));
}

async fn is_enabled(driver: &WebDriver, selector: &str) -> bool {
    driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the element {selector} must exist: {error}"))
        .is_enabled()
        .await
        .unwrap_or_else(|error| panic!("the element {selector} must report enabled: {error}"))
}

async fn wait_for_save(driver: &WebDriver, expected: &str) {
    browser::wait_for_attribute(
        driver,
        "[data-profile-settings]",
        "data-profile-saved",
        expected,
    )
    .await;
    browser::wait_for_present(driver, "[data-profile-saved-dialog]").await;
    browser::wait_for_text(driver, "保存しました").await;
    browser::click(driver, "[data-profile-saved-close]").await;
    browser::wait_for_absent(driver, "[data-profile-saved-dialog]").await;
}

async fn wait_for_attribute_absent(driver: &WebDriver, selector: &str, attribute: &str) {
    wait_until(driver, || async {
        browser::attribute(driver, selector, attribute)
            .await
            .is_empty()
    })
    .await;
}

async fn wait_for_selector_absent(driver: &WebDriver, selector: &str) {
    wait_until(driver, || async {
        driver.find(By::Css(selector)).await.is_err()
    })
    .await;
}

async fn wait_until<F, Fut>(driver: &WebDriver, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + WAIT;
    loop {
        if condition().await {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the condition did not hold within {WAIT:?}; current URL: {:?}",
            driver.current_url().await
        );
        std::thread::sleep(POLL);
    }
}

async fn set_birth_date(driver: &WebDriver, value: &str) {
    let mut parts = value.split('-');
    let year = parts.next().expect("the year");
    let month = parts.next().expect("the month");
    let day = parts.next().expect("the day");
    browser::select_value(
        driver,
        "[data-date-input=\"profile-birth-date\"] [data-date-year]",
        year,
    )
    .await;
    browser::select_value(
        driver,
        "[data-date-input=\"profile-birth-date\"] [data-date-month]",
        month,
    )
    .await;
    browser::select_value(
        driver,
        "[data-date-input=\"profile-birth-date\"] [data-date-day]",
        day,
    )
    .await;
}

async fn wait_birth_date(driver: &WebDriver, value: &str) {
    let mut parts = value.split('-');
    let year = parts.next().expect("the year");
    let month = parts.next().expect("the month");
    let day = parts.next().expect("the day");
    browser::wait_for_value(
        driver,
        "[data-date-input=\"profile-birth-date\"] [data-date-year]",
        year,
    )
    .await;
    browser::wait_for_value(
        driver,
        "[data-date-input=\"profile-birth-date\"] [data-date-month]",
        month,
    )
    .await;
    browser::wait_for_value(
        driver,
        "[data-date-input=\"profile-birth-date\"] [data-date-day]",
        day,
    )
    .await;
}

fn birth_date_years_ago(years: i64) -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the current time")
        .as_secs() as i64;
    let today = warrior_core::datetime::japan_date(seconds);
    let year = today[0..4].parse::<i64>().expect("the Japan year");
    format!("{:04}-01-01", year - years)
}
