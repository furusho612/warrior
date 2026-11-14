//! 左右別のセットとスーパーセットの E2E。
//!
//! 対応する要求: `REQ-SET-004`、`REQ-SET-005`。
//! 左右別のセットに左右の値を入力し、スーパーセットを組み、組の途中の種目の完了では
//! レストタイマーが起動せず、最後の種目の完了で起動することを確かめる。

mod support;

use serde_json::{json, Value};
use std::time::{Duration, Instant};
use support::browser::{self, Browser};
use support::record_common;
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const UNILATERAL_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000018";
const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8828;
const PROVIDER_PORT: u16 = 8829;

#[tokio::test]
async fn records_a_unilateral_superset_and_starts_the_timer_on_the_last_member() {
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
    let training_id = start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, UNILATERAL_EXERCISE_ID).await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::select_exercise(&driver, UNILATERAL_EXERCISE_ID).await;

    let unilateral_set = add_set(&driver).await;
    fill_unilateral_set(&driver, &unilateral_set, (10, 5), (12, 4)).await;
    wait_for_complete_ready(&driver, &unilateral_set).await;

    create_superset_with(&driver, BENCH_EXERCISE_ID).await;
    assert_eq!(group_member_count(&driver).await, 2);

    complete_set(&driver, &unilateral_set).await;
    browser::wait_for_attribute(&driver, "[data-session-id]", "data-set-events", "1").await;
    assert_eq!(
        browser::attribute(&driver, "[data-session-id]", "data-rest-timer-events").await,
        "0",
        "a middle member of a superset must not start the rest timer"
    );
    wait_for_unilateral_set(&server, &training_id, &unilateral_set).await;

    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    let bench_set = add_set(&driver).await;
    browser::set_input(&driver, &weight_selector(&bench_set), "100").await;
    browser::set_input(&driver, &reps_selector(&bench_set), "5").await;
    wait_for_complete_ready(&driver, &bench_set).await;
    complete_set(&driver, &bench_set).await;
    browser::wait_for_attribute(&driver, "[data-session-id]", "data-set-events", "2").await;
    browser::wait_for_attribute(&driver, "[data-session-id]", "data-rest-timer-events", "1").await;
    wait_for_bench_set(&server, &bench_set).await;
    wait_for_superset(&server, &training_id).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    finish_and_check_the_summary(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=record-unilateral-superset",
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
    browser::attribute(driver, "[data-session-id]", "data-session-id").await
}

async fn add_set(driver: &WebDriver) -> String {
    browser::wait_for_present(driver, "[data-add-set]").await;
    browser::click(driver, "[data-add-set]").await;
    browser::wait_for_present(driver, "[data-set-row]").await;
    let rows = set_ids(driver).await;
    rows.last().cloned().expect("the new set id")
}

async fn fill_unilateral_set(
    driver: &WebDriver,
    set_id: &str,
    left: (u32, u32),
    right: (u32, u32),
) {
    browser::set_input(
        driver,
        &side_selector(set_id, "left", "weight"),
        &left.0.to_string(),
    )
    .await;
    browser::set_input(
        driver,
        &side_selector(set_id, "left", "reps"),
        &left.1.to_string(),
    )
    .await;
    assert_eq!(
        browser::attribute(
            driver,
            &format!("[data-set-id=\"{set_id}\"] [data-set-complete]"),
            "data-set-complete-ready"
        )
        .await,
        "false",
        "one side is not enough to complete a unilateral set"
    );
    browser::set_input(
        driver,
        &side_selector(set_id, "right", "weight"),
        &right.0.to_string(),
    )
    .await;
    browser::set_input(
        driver,
        &side_selector(set_id, "right", "reps"),
        &right.1.to_string(),
    )
    .await;
}

async fn create_superset_with(driver: &WebDriver, exercise_id: &str) {
    let candidate = format!("[data-superset-candidate][data-exercise-id=\"{exercise_id}\"]");
    browser::wait_for_present(driver, &candidate).await;
    browser::click(driver, &candidate).await;
    wait_for_enabled(driver, "[data-superset-create]").await;
    browser::click(driver, "[data-superset-create]").await;
    browser::wait_for_present(driver, "[data-superset-group]").await;
}

async fn complete_set(driver: &WebDriver, set_id: &str) {
    browser::click(
        driver,
        &format!("[data-set-id=\"{set_id}\"] [data-set-complete]"),
    )
    .await;
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"]"),
        "data-set-completed",
        "true",
    )
    .await;
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

async fn wait_for_enabled(driver: &WebDriver, selector: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let enabled = browser::eval_json(
            driver,
            "const done = arguments[arguments.length - 1]; \
             const element = document.querySelector(arguments[0]); \
             done(JSON.stringify(element ? !element.disabled : false));",
            vec![json!(selector)],
        )
        .await
        .as_bool()
        .unwrap_or(false);
        if enabled {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the element {selector} did not become enabled"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

async fn group_member_count(driver: &WebDriver) -> usize {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(document.querySelectorAll('[data-superset-member]').length));",
        Vec::new(),
    )
    .await;
    value.as_u64().unwrap_or(0) as usize
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

fn side_selector(set_id: &str, side: &str, field: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-side=\"{side}\"][data-set-side-{field}]")
}

fn weight_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-weight]")
}

fn reps_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-reps]")
}

async fn wait_for_unilateral_set(server: &DevServer, training_id: &str, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let set = record_common::optional_row(
            server,
            &format!(
                "SELECT s.completed, sg.left_weight_kg, sg.left_reps, \
                 sg.right_weight_kg, sg.right_reps \
                 FROM sets s JOIN set_segments sg ON sg.set_id = s.id \
                 WHERE s.id = '{set_id}' AND s.training_exercise_id IN \
                  (SELECT id FROM training_exercises WHERE training_id = '{training_id}')"
            ),
        );
        if set.as_ref().is_some_and(|row| {
            row["completed"].as_i64() == Some(1)
                && row["left_weight_kg"].as_f64() == Some(10.0)
                && row["left_reps"].as_i64() == Some(5)
                && row["right_weight_kg"].as_f64() == Some(12.0)
                && row["right_reps"].as_i64() == Some(4)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the unilateral set was not saved: {set:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

async fn wait_for_bench_set(server: &DevServer, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let set = record_common::optional_row(
            server,
            &format!("SELECT completed FROM sets WHERE id = '{set_id}'"),
        );
        if set
            .as_ref()
            .is_some_and(|row| row["completed"].as_i64() == Some(1))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the bench set was not saved: {set:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

async fn wait_for_superset(server: &DevServer, training_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let value = server
            .execute_d1_json(&format!(
                "SELECT te.exercise_id FROM superset_members sm \
                 JOIN training_exercises te ON te.id = sm.training_exercise_id \
                 JOIN supersets ss ON ss.id = sm.superset_id \
                 WHERE ss.training_id = '{training_id}' ORDER BY sm.position"
            ))
            .ok();
        let members = value
            .as_ref()
            .and_then(|value| value.get(0))
            .and_then(|entry| entry.get("results"))
            .and_then(|rows| rows.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row["exercise_id"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if members
            == vec![
                UNILATERAL_EXERCISE_ID.to_owned(),
                BENCH_EXERCISE_ID.to_owned(),
            ]
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the superset was not saved in order: {members:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
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
        "598"
    );
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
