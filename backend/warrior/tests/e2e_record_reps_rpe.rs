//! 反動と補助の回数、RPE の入力、保存、表示の E2E。
//!
//! 対応する要求: `REQ-SET-006`、`REQ-SET-007`、`REQ-REC-005`。
//! セット詳細の drawer で反動と補助の回数、RPE を入力し、保存した値を開き直して
//! 表示することを確かめる。回数の内訳と RPE の無効値を拒否してエラーを表示し、
//! 正しい値に直すとエラーが消えて保存されることを確かめる。別のセットを保存しても、
//! あるセットのエラーが消えないことを確かめる。完了したセットの RPE は編集できない
//! ことを確かめる。終了のサマリーで、補助の回数だけを除いた総重量を確かめる。

mod support;

use serde_json::{json, Value};
use std::time::{Duration, Instant};
use support::browser::{self, Browser};
use support::record_browser::element_disabled;
use support::record_common;
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8846;
const PROVIDER_PORT: u16 = 8847;

#[tokio::test]
async fn records_the_breakdown_and_rpe_from_the_set_drawer() {
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
    start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;

    let first_set = add_set(&driver).await;
    let first_label = assert_open_button(&driver, &first_set, "セット 1 の詳細を開く").await;
    browser::set_input(&driver, &weight_selector(&first_set), "100").await;
    browser::set_input(&driver, &reps_selector(&first_set), "10").await;
    open_drawer(&driver, &first_set).await;
    browser::set_input(&driver, &momentum_selector(), "6").await;
    browser::set_input(&driver, &assisted_selector(), "5").await;
    browser::wait_for_text(&driver, "反動と補助の回数の合計は回数以下にしてください。").await;
    browser::set_input(&driver, &momentum_selector(), "3").await;
    browser::set_input(&driver, &assisted_selector(), "1").await;
    browser::wait_for_absent(&driver, "[data-record-error]").await;
    browser::set_input(&driver, &rpe_selector(), "6.5").await;
    browser::wait_for_value(&driver, &rpe_selector(), "6.5").await;
    browser::click(&driver, &complete_selector(&first_set)).await;
    wait_for_set(&server, &first_set, Some("6.5"), 3, 1).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    driver.refresh().await.expect("the page must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_present(&driver, &open_selector(&first_set)).await;
    open_drawer(&driver, &first_set).await;
    browser::wait_for_value(&driver, &momentum_selector(), "3").await;
    browser::wait_for_value(&driver, &assisted_selector(), "1").await;
    browser::wait_for_value(&driver, &rpe_selector(), "6.5").await;
    browser::click(&driver, "[data-set-drawer-close]").await;
    browser::wait_for_absent(&driver, "[data-set-drawer]").await;

    let second_set = add_set(&driver).await;
    let second_label = assert_open_button(&driver, &second_set, "セット 2 の詳細を開く").await;
    assert_ne!(first_label, second_label);
    browser::set_input(&driver, &weight_selector(&second_set), "80").await;
    browser::set_input(&driver, &reps_selector(&second_set), "8").await;
    open_drawer(&driver, &second_set).await;
    browser::set_input(&driver, &rpe_selector(), "7.25").await;
    browser::wait_for_text(
        &driver,
        "RPE は 6.0 から 10.0 まで 0.5 刻みで入力してください。",
    )
    .await;

    let empty_rpe_set = add_set(&driver).await;
    let third_label = assert_open_button(&driver, &empty_rpe_set, "セット 3 の詳細を開く").await;
    assert_ne!(second_label, third_label);
    browser::set_input(&driver, &weight_selector(&empty_rpe_set), "60").await;
    browser::set_input(&driver, &reps_selector(&empty_rpe_set), "5").await;
    browser::click(&driver, &complete_selector(&empty_rpe_set)).await;
    wait_for_set(&server, &empty_rpe_set, None, 0, 0).await;
    browser::wait_for_present(
        &driver,
        &format!("[data-record-error][data-record-error-set-id=\"{second_set}\"]"),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    browser::set_input(&driver, &rpe_selector(), "10.0").await;
    browser::wait_for_absent(&driver, "[data-record-error]").await;
    browser::click(&driver, &complete_selector(&second_set)).await;
    wait_for_set(&server, &second_set, Some("10.0"), 0, 0).await;
    browser::wait_for_present(&driver, "[data-set-drawer-locked]").await;
    assert!(
        element_disabled(&driver, &rpe_selector()).await,
        "a completed set must lock the RPE input"
    );
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    finish_and_check_the_summary(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=record-reps-rpe",
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

async fn start_training(driver: &WebDriver) {
    driver
        .find(By::Css("[data-start]"))
        .await
        .expect("the start button must exist")
        .click()
        .await
        .expect("the start button must be clickable");
    browser::wait_for_route(driver, "/record/:training_id").await;
}

async fn add_set(driver: &WebDriver) -> String {
    browser::wait_for_present(driver, "[data-add-set]").await;
    browser::click(driver, "[data-add-set]").await;
    browser::wait_for_present(driver, "[data-set-row]").await;
    let rows = set_ids(driver).await;
    rows.last().cloned().expect("the new set id")
}

async fn open_drawer(driver: &WebDriver, set_id: &str) {
    browser::click(driver, &open_selector(set_id)).await;
    browser::wait_for_present(driver, "[data-set-drawer]").await;
}

async fn assert_open_button(driver: &WebDriver, set_id: &str, expected_label: &str) -> String {
    let selector = open_selector(set_id);
    let label = browser::attribute(driver, &selector, "aria-label").await;
    assert_eq!(label, expected_label, "the open button label");
    let rect = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const element = document.querySelector(arguments[0]); \
         const rect = element.getBoundingClientRect(); \
         done(JSON.stringify({width: rect.width, height: rect.height}));",
        vec![json!(selector)],
    )
    .await;
    assert!(
        rect["width"].as_f64().unwrap_or_default() >= 44.0,
        "the open button width must be at least 44px: {rect:?}"
    );
    assert!(
        rect["height"].as_f64().unwrap_or_default() >= 44.0,
        "the open button height must be at least 44px: {rect:?}"
    );
    label
}

fn open_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-open]")
}

async fn set_ids(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-set-row]')) \
           .map((element) => element.getAttribute('data-set-id'))));",
        Vec::new(),
    )
    .await;
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn wait_for_set(
    server: &DevServer,
    set_id: &str,
    rpe: Option<&str>,
    momentum_reps: i64,
    assisted_reps: i64,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let expected_rpe = rpe.and_then(|value| value.parse::<f64>().ok());
    let set_sql = format!("SELECT rpe, completed FROM sets WHERE id = '{set_id}'");
    let segment_sql =
        format!("SELECT momentum_reps, assisted_reps FROM set_segments WHERE set_id = '{set_id}'");
    loop {
        let set = record_common::optional_row(server, &set_sql);
        let segment = record_common::optional_row(server, &segment_sql);
        if set.as_ref().is_some_and(|row| {
            row["rpe"].as_f64() == expected_rpe && row["completed"].as_i64() == Some(1)
        }) && segment.as_ref().is_some_and(|row| {
            row["momentum_reps"].as_i64() == Some(momentum_reps)
                && row["assisted_reps"].as_i64() == Some(assisted_reps)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the set {set_id} was not saved: {set:?}, {segment:?}"
        );
    }
}

async fn finish_and_check_the_summary(driver: &WebDriver) {
    browser::click(driver, "[data-finish]").await;
    browser::wait_for_route(driver, "/summary/:training_id").await;
    browser::wait_for_attribute(driver, "[data-summary]", "data-set-count", "3").await;
    browser::wait_for_attribute(driver, "[data-summary]", "data-total-weight-kg", "1840").await;
}

async fn current_user_id(driver: &WebDriver) -> String {
    let me = browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; fetch('/api/me').then((response) => response.text()).then((text) => done(text));",
        Vec::new(),
    )
    .await;
    serde_json::from_str::<Value>(&me)
        .expect("the me response is JSON")
        .get("id")
        .and_then(|value| value.as_str())
        .expect("the user id")
        .to_owned()
}

fn seed_profile(server: &DevServer, user_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, weight_kg, created_at, updated_at) \
             VALUES ('{user_id}', 70.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}

fn weight_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-weight]")
}

fn reps_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-reps]")
}

fn complete_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-complete]")
}

fn momentum_selector() -> String {
    "[data-set-drawer] [data-set-momentum]".to_owned()
}

fn assisted_selector() -> String {
    "[data-set-drawer] [data-set-assisted]".to_owned()
}

fn rpe_selector() -> String {
    "[data-set-drawer] [data-set-rpe]".to_owned()
}
