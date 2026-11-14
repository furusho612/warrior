//! Safari で記録中に再読み込みし、記録画面とメモとセットが復元されることの E2E。
//!
//! 対応する要求: `REQ-NFR-004`、`REQ-SET-001`。
//! Safari の WebDriver は CDP を持たないため、通信の切断は Chrome の E2E が担う。

mod support;

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const MEMO: &str = "サファリのメモ";
const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const WEIGHT: &str = "60";
const REPS: &str = "8";

#[tokio::test]
async fn safari_restores_the_record_screen_memo_and_sets_after_reload() {
    assert_eq!(
        Browser::from_env(),
        Browser::Safari,
        "this test requires WARRIOR_E2E_BROWSER=safari"
    );
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Safari, driver_port);
    let driver = browser::open(Browser::Safari, &driver_process).await;

    driver
        .goto(format!("{}/test/user?sub=safari", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(&driver, &server).await;

    driver
        .goto(format!("{}/record", server.origin()))
        .await
        .expect("the record page must open");
    browser::wait_for_route(&driver, "/record").await;
    browser::click_text(&driver, "記録を開始").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    let training_id = browser::attribute(&driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty());

    add_exercise(&driver, BENCH_EXERCISE_ID).await;
    click(&driver, "[data-add-set]").await;
    browser::wait_for_present(&driver, "[data-set-row]").await;
    set_input(&driver, "[data-set-row] [data-set-weight]", WEIGHT).await;
    set_input(&driver, "[data-set-row] [data-set-reps]", REPS).await;

    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys(MEMO)
        .await
        .expect("the memo must be typed");
    browser::wait_for_attribute(
        &driver,
        "[data-persisted-memo]",
        "data-persisted-memo",
        MEMO,
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    driver.refresh().await.expect("the page must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(&driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
    browser::wait_for_value(&driver, "textarea[data-memo]", MEMO).await;
    browser::wait_for_present(&driver, "[data-set-row]").await;
    browser::wait_for_value(&driver, "[data-set-row] [data-set-weight]", WEIGHT).await;
    browser::wait_for_value(&driver, "[data-set-row] [data-set-reps]", REPS).await;

    driver.quit().await.expect("the browser must close");
}

async fn add_exercise(driver: &WebDriver, exercise_id: &str) {
    browser::wait_for_present(driver, "[data-add-exercise]").await;
    click(driver, "[data-add-exercise]").await;
    browser::wait_for_route(driver, "/record/:training_id/exercises").await;
    browser::wait_for_present(driver, "[data-exercise-choose]").await;
    let selector = format!("[data-exercise-choose][data-exercise-id=\"{exercise_id}\"]");
    click(driver, &selector).await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    let row = format!("[data-exercise-row][data-exercise-id=\"{exercise_id}\"]");
    browser::wait_for_present(driver, &row).await;
}

async fn set_input(driver: &WebDriver, selector: &str, value: &str) {
    let input = driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the input {selector} must exist: {error}"));
    input
        .clear()
        .await
        .unwrap_or_else(|error| panic!("the input {selector} must be cleared: {error}"));
    input
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
