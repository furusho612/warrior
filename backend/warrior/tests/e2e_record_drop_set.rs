//! ドロップセットの選択、段の追加、表示、集計の E2E。
//!
//! 対応する要求: `REQ-SET-003`、`REQ-SET-009`、`REQ-REC-005`。
//! 先頭の行ではドロップセットを選べないことを確かめ、通常のセットの直後に
//! ドロップセットを選んで段を追加し、`D` の表示と段の行、終了サマリーのセット数を
//! 確かめる。

mod support;

use serde_json::{json, Value};
use std::time::{Duration, Instant};
use support::browser::{self, Browser};
use support::record_common;
use support::{current_user_id, seed_profile, DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8827;
const PROVIDER_PORT: u16 = 8828;

#[tokio::test]
async fn adds_a_drop_set_and_counts_the_record_sets() {
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
    let _training_id = start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;

    let first_set = add_set(&driver).await;
    assert!(
        option_disabled(&driver, &first_set, "drop").await,
        "the first set must not select a drop set"
    );
    browser::set_input(&driver, &weight_selector(&first_set), "100").await;
    browser::set_input(&driver, &reps_selector(&first_set), "5").await;
    wait_for_complete_ready(&driver, &first_set).await;
    browser::click(&driver, &complete_selector(&first_set)).await;
    browser::wait_for_attribute(
        &driver,
        &format!("[data-set-id=\"{first_set}\"]"),
        "data-set-completed",
        "true",
    )
    .await;

    let drop_set = add_set(&driver).await;
    change_kind(&driver, &drop_set, "drop").await;
    assert_eq!(
        browser::attribute(
            &driver,
            &format!("[data-set-id=\"{drop_set}\"]"),
            "data-set-number"
        )
        .await,
        "D"
    );
    assert_eq!(
        browser::attribute(
            &driver,
            &format!("[data-set-id=\"{drop_set}\"]"),
            "data-set-drop"
        )
        .await,
        "true"
    );
    browser::click(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-delete-segment]"),
    )
    .await;
    browser::wait_for_absent(&driver, &format!("[data-set-id=\"{drop_set}\"]")).await;

    let drop_set = add_set(&driver).await;
    change_kind(&driver, &drop_set, "drop").await;
    browser::wait_for_present(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-add-segment]"),
    )
    .await;
    browser::click(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-add-segment]"),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-display-rows", "3").await;

    let extra_segments = segment_ids(&driver, &drop_set).await;
    assert_eq!(extra_segments.len(), 2);
    browser::click(
        &driver,
        &format!(
            "[data-segment-id=\"{}\"] [data-delete-segment]",
            extra_segments[1]
        ),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-display-rows", "2").await;
    browser::click(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-add-segment]"),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-display-rows", "3").await;

    let segments = segment_ids(&driver, &drop_set).await;
    assert_eq!(segments.len(), 2);
    browser::set_input(&driver, &segment_weight(&segments[0]), "80").await;
    browser::set_input(&driver, &segment_reps(&segments[0]), "8").await;
    browser::set_input(&driver, &segment_weight(&segments[1]), "60").await;
    browser::set_input(&driver, &segment_reps(&segments[1]), "10").await;
    wait_for_complete_ready(&driver, &drop_set).await;
    browser::click(&driver, &complete_selector(&drop_set)).await;
    browser::wait_for_attribute(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"]"),
        "data-set-completed",
        "true",
    )
    .await;
    wait_for_drop_set(&server, &drop_set).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    assert_eq!(
        browser::attribute(&driver, "[data-set-table]", "data-set-rows").await,
        "2"
    );
    assert_eq!(
        browser::attribute(&driver, "[data-set-table]", "data-set-display-rows").await,
        "3"
    );
    finish_and_check_the_summary(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=record-drop-set",
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

async fn start_training(driver: &WebDriver) -> String {
    driver
        .find(By::Css("[data-start]"))
        .await
        .expect("the start button must exist")
        .click()
        .await
        .expect("the start button must be clickable");
    browser::wait_for_route(driver, "/record/:training_id").await;
    let training_id = browser::attribute(driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty(), "the record must have a session id");
    training_id
}

async fn add_set(driver: &WebDriver) -> String {
    browser::wait_for_present(driver, "[data-add-set]").await;
    browser::click(driver, "[data-add-set]").await;
    browser::wait_for_present(driver, "[data-set-row]").await;
    let rows = browser::set_ids(driver).await;
    rows.last().cloned().expect("the new set id")
}

async fn change_kind(driver: &WebDriver, set_id: &str, kind: &str) {
    let selector = format!("[data-set-id=\"{set_id}\"] select[data-set-kind]");
    let select = driver
        .find(By::Css(&selector))
        .await
        .unwrap_or_else(|error| panic!("the select {selector} must exist: {error}"));
    let select = SelectElement::new(&select)
        .await
        .unwrap_or_else(|error| panic!("the select {selector} must be a select: {error}"));
    select
        .select_by_value(kind)
        .await
        .unwrap_or_else(|error| panic!("the kind {kind} must be selectable: {error}"));
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"]"),
        "data-set-kind",
        kind,
    )
    .await;
}

async fn option_disabled(driver: &WebDriver, set_id: &str, kind: &str) -> bool {
    let selector = format!("[data-set-id=\"{set_id}\"] select[data-set-kind]");
    browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const select = document.querySelector(arguments[0]); \
         const option = select ? select.querySelector('option[value=\"' + arguments[1] + '\"]') : null; \
         done(JSON.stringify(option ? option.disabled : null));",
        vec![json!(selector), json!(kind)],
    )
    .await
    .as_bool()
    .unwrap_or(false)
}

async fn segment_ids(driver: &WebDriver, set_id: &str) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll(arguments[0])) \
           .map((element) => element.getAttribute('data-segment-id'))));",
        vec![json!(format!(
            "[data-set-id=\"{set_id}\"] [data-segment-id]"
        ))],
    )
    .await;
    let mut segments = values(value);
    segments.dedup();
    segments
}

fn values(value: Value) -> Vec<String> {
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

async fn wait_for_drop_set(server: &DevServer, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let set_sql = format!("SELECT kind, completed FROM sets WHERE id = '{set_id}'");
    let segment_sql =
        format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{set_id}'");
    loop {
        let set = record_common::optional_row(server, &set_sql);
        let segments = record_common::optional_row(server, &segment_sql);
        if set
            .as_ref()
            .is_some_and(|row| row["kind"] == "drop" && row["completed"].as_i64() == Some(1))
            && segments
                .as_ref()
                .is_some_and(|row| row["count"].as_i64() == Some(2))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the drop set {set_id} was not saved: {set:?}, {segments:?}"
        );
    }
}

async fn finish_and_check_the_summary(driver: &WebDriver) {
    browser::click(driver, "[data-finish]").await;
    browser::wait_for_route(driver, "/summary/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-set-count").await,
        "2"
    );
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-total-weight-kg").await,
        "1740"
    );
}

async fn wait_for_complete_ready(driver: &WebDriver, set_id: &str) {
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"] [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
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

fn segment_weight(segment_id: &str) -> String {
    format!("[data-segment-id=\"{segment_id}\"] [data-set-weight]")
}

fn segment_reps(segment_id: &str) -> String {
    format!("[data-segment-id=\"{segment_id}\"] [data-set-reps]")
}
