//! レストタイマーの起動、延長、短縮、スキップ、タブの復帰、再読み込み、終了の E2E。
//!
//! 対応する要求: `REQ-TMR-002`、`REQ-TMR-003`、`REQ-TMR-004`、`REQ-TMR-005`、`REQ-TMR-006`。
//! セットを完了するとタイマーが起動し、残り時間が減ることを待ってから延長と短縮を行う。
//! 別のタブを開いて戻り、再読み込みの後も保存した終了予定時刻からタイマーが続くことを
//! 確かめる。最後に 1 秒のレストを終わらせ、終了の表示を確かめる。

mod support;

use std::time::{Duration, Instant};

use support::browser::{self, Browser};
use support::record_browser;
use support::{current_user_id, seed_profile, DevServer, TestOidc};
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8862;
const PROVIDER_PORT: u16 = 8863;

#[tokio::test]
async fn runs_the_rest_timer_through_completion_and_controls() {
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

    record_browser::login(&driver, &server, &provider, "record-rest-timer").await;
    let user_id = current_user_id(&driver).await;
    seed_profile(&server, &user_id);
    let training_id = record_browser::start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::wait_for_present(&driver, "[data-rest-settings]").await;
    browser::wait_for_value(&driver, "[data-rest-default]", "90").await;
    browser::set_input(&driver, "[data-rest-default]", "15").await;
    browser::wait_for_value(&driver, "[data-rest-default]", "15").await;

    let first_set = record_browser::add_set(&driver).await;
    fill_set(&driver, &first_set).await;
    record_browser::wait_for_complete_ready(&driver, &first_set).await;
    browser::click(&driver, &record_browser::complete_selector(&first_set)).await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-active",
        "true",
    )
    .await;
    let initial = remaining_seconds(&driver).await;
    assert!(
        (14..=15).contains(&initial),
        "the timer must start with the configured rest time: {initial}"
    );

    let old_window = driver.window().await.expect("the current window");
    let new_window = driver.new_window().await.expect("a new window");
    driver
        .switch_to_window(new_window.clone())
        .await
        .expect("the new window must be selected");
    driver
        .goto(format!("{origin}/record/{training_id}"))
        .await
        .expect("the record must open in the new window");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-active",
        "true",
    )
    .await;
    wait_for_remaining_at_most(&driver, initial.saturating_sub(1)).await;
    driver
        .close_window()
        .await
        .expect("the new window must close");
    driver
        .switch_to_window(old_window)
        .await
        .expect("the old window must be selected");
    driver.refresh().await.expect("the page must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-active",
        "true",
    )
    .await;

    let before_extend = remaining_seconds(&driver).await;
    browser::click(&driver, "[data-rest-timer-extend]").await;
    wait_for_remaining_at_least(&driver, before_extend.saturating_add(10)).await;

    let before_shorten = remaining_seconds(&driver).await;
    browser::click(&driver, "[data-rest-timer-shorten]").await;
    wait_for_remaining_at_most(&driver, before_shorten.saturating_sub(10)).await;

    browser::click(&driver, "[data-rest-timer-skip]").await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-active",
        "false",
    )
    .await;

    browser::set_input(&driver, "[data-rest-default]", "1").await;
    browser::wait_for_value(&driver, "[data-rest-default]", "1").await;
    let second_set = record_browser::add_set(&driver).await;
    fill_set(&driver, &second_set).await;
    record_browser::wait_for_complete_ready(&driver, &second_set).await;
    browser::click(&driver, &record_browser::complete_selector(&second_set)).await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-finished",
        "true",
    )
    .await;
    assert_eq!(
        browser::attribute(
            &driver,
            "[data-rest-timer-remaining]",
            "data-rest-timer-remaining"
        )
        .await,
        "00:00"
    );

    driver.refresh().await.expect("the page must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-finished",
        "true",
    )
    .await;
    assert_eq!(
        browser::attribute(
            &driver,
            "[data-rest-timer-remaining]",
            "data-rest-timer-remaining"
        )
        .await,
        "00:00",
        "the finished timer must come back from the saved end time"
    );

    browser::click(&driver, "[data-rest-timer-skip]").await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-active",
        "false",
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn fill_set(driver: &WebDriver, set_id: &str) {
    browser::set_input(driver, &record_browser::weight_selector(set_id), "100").await;
    browser::set_input(driver, &record_browser::reps_selector(set_id), "5").await;
}

async fn remaining_seconds(driver: &WebDriver) -> u64 {
    browser::attribute(
        driver,
        "[data-rest-timer]",
        "data-rest-timer-remaining-seconds",
    )
    .await
    .parse()
    .unwrap_or(u64::MAX)
}

async fn wait_for_remaining_at_most(driver: &WebDriver, expected: u64) {
    wait_for_remaining(driver, "at most", expected, |remaining, expected| {
        remaining <= expected
    })
    .await;
}

async fn wait_for_remaining_at_least(driver: &WebDriver, expected: u64) {
    wait_for_remaining(driver, "at least", expected, |remaining, expected| {
        remaining >= expected
    })
    .await;
}

async fn wait_for_remaining(
    driver: &WebDriver,
    comparison: &str,
    expected: u64,
    matches: impl Fn(u64, u64) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let remaining = remaining_seconds(driver).await;
        if matches(remaining, expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the remaining seconds must be {comparison} {expected} but were {remaining}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
