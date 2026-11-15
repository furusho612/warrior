//! 時間で記録する種目のストップウォッチ、手入力、保存、サマリーの E2E。
//!
//! 対応する要求: `REQ-SET-010`、`REQ-REC-005`。
//! ストップウォッチを開始し、表示の経過時間が 1 秒以上になったことを条件に停止する。
//! 計測中にセットを完了したときは、開始時刻が消えて計測中の表示が再読み込みの後も
//! 残らないことを確かめる。
//! 停止した時間と手入力した時間を保存し、左右別の時間も保存する。
//! 種目を切り替えてもストップウォッチの状態が別の種目へ移らないことを確かめる。
//! 終了のサマリーで、総重量が 0、セット数が 3、最長時間の自己ベストが
//! 左右を区別しない最大値になることを確かめる。

mod support;

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::record_common;
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;
use warrior_core::duration;

const PLANK_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005d";
const SIDE_PLANK_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005a";
const BIKE_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000060";
const SERVER_PORT: u16 = 8856;
const PROVIDER_PORT: u16 = 8857;

#[tokio::test]
async fn records_time_sets_with_the_stopwatch_and_manual_input() {
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
    browser::add_exercise(&driver, PLANK_EXERCISE_ID).await;
    browser::add_exercise(&driver, SIDE_PLANK_EXERCISE_ID).await;
    browser::add_exercise(&driver, BIKE_EXERCISE_ID).await;

    let plank = detail_selector(PLANK_EXERCISE_ID);
    browser::select_exercise(&driver, PLANK_EXERCISE_ID).await;
    let stopwatch_set = add_set(&driver, &plank).await;
    let stopwatch_row = set_row(&plank, &stopwatch_set);
    assert_time_inputs(&driver, &stopwatch_row).await;
    wait_for_empty_time_set(&server, &stopwatch_set).await;
    start_the_stopwatch(&driver, &stopwatch_row).await;

    let bike = detail_selector(BIKE_EXERCISE_ID);
    browser::select_exercise(&driver, BIKE_EXERCISE_ID).await;
    let other_set = add_set(&driver, &bike).await;
    let other_row = set_row(&bike, &other_set);
    assert_time_inputs(&driver, &other_row).await;
    assert_no_running_stopwatch(&driver, &bike).await;
    let other_stop = format!("{other_row} [data-stopwatch-stop]");
    assert!(
        !is_enabled(&driver, &other_stop).await,
        "the stopwatch of another exercise must not be running"
    );
    press_disabled_stop(&driver, &other_stop).await;
    assert_eq!(
        browser::value(&driver, &format!("{other_row} [data-set-duration]")).await,
        ""
    );

    browser::select_exercise(&driver, PLANK_EXERCISE_ID).await;
    browser::wait_for_attribute(
        &driver,
        &format!("{stopwatch_row} [data-stopwatch-elapsed]"),
        "data-stopwatch-running",
        "true",
    )
    .await;
    wait_for_elapsed_at_least(&driver, &stopwatch_row, 1).await;
    assert_eq!(
        browser::value(&driver, &format!("{stopwatch_row} [data-set-duration]")).await,
        ""
    );
    stop_the_stopwatch(&driver, &stopwatch_row).await;

    open_drawer(&driver, &stopwatch_row).await;
    for selector in ["[data-set-momentum]", "[data-set-assisted]"] {
        assert!(
            driver
                .find(By::Css(format!("[data-set-drawer] {selector}")))
                .await
                .is_err(),
            "a time set must not show {selector} in the drawer"
        );
    }
    browser::set_input(&driver, "[data-set-drawer] [data-set-rpe]", "8.0").await;
    browser::wait_for_value(&driver, "[data-set-drawer] [data-set-rpe]", "8.0").await;
    browser::click(&driver, "[data-set-drawer-close]").await;
    browser::wait_for_absent(&driver, "[data-set-drawer]").await;
    let stopwatch_duration = duration::parse(
        &browser::value(&driver, &format!("{stopwatch_row} [data-set-duration]")).await,
    )
    .expect("the stopwatch duration");
    complete_set(&driver, &stopwatch_row).await;
    wait_for_duration(&server, &stopwatch_set, stopwatch_duration, Some(8.0)).await;

    let manual_set = add_set(&driver, &plank).await;
    let manual_row = set_row(&plank, &manual_set);
    assert_time_inputs(&driver, &manual_row).await;
    browser::set_input(
        &driver,
        &format!("{manual_row} [data-set-duration]"),
        "02:05",
    )
    .await;
    wait_for_complete_ready(&driver, &manual_row).await;
    start_the_stopwatch(&driver, &manual_row).await;
    assert_eq!(
        stopwatch_started_at_keys(&driver, &manual_set).await,
        vec![format!("stopwatch_started_at:{manual_set}")],
        "the running stopwatch must keep the start of the whole set"
    );
    complete_set(&driver, &manual_row).await;
    browser::wait_for_attribute(
        &driver,
        &format!("{manual_row} [data-stopwatch-elapsed]"),
        "data-stopwatch-running",
        "false",
    )
    .await;
    assert!(
        !is_enabled(&driver, &format!("{manual_row} [data-stopwatch-stop]")).await,
        "the stopwatch of a completed set must be stopped"
    );
    wait_for_duration(&server, &manual_set, 125, None).await;
    assert_eq!(
        stopwatch_started_at_keys(&driver, &manual_set).await,
        Vec::<String>::new(),
        "completing a time set must clear the stopwatch start"
    );

    driver.refresh().await.expect("the page must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::select_exercise(&driver, PLANK_EXERCISE_ID).await;
    browser::wait_for_present(&driver, &format!("{manual_row} [data-stopwatch-elapsed]")).await;
    assert_eq!(
        browser::attribute(
            &driver,
            &format!("{manual_row} [data-stopwatch-elapsed]"),
            "data-stopwatch-running"
        )
        .await,
        "false",
        "the stopwatch of a completed set must not run after a reload"
    );
    assert_eq!(
        stopwatch_started_at_keys(&driver, &manual_set).await,
        Vec::<String>::new(),
        "the stopwatch start of a completed set must not come back after a reload"
    );

    browser::select_exercise(&driver, SIDE_PLANK_EXERCISE_ID).await;
    let side_plank = detail_selector(SIDE_PLANK_EXERCISE_ID);
    let side_set = add_set(&driver, &side_plank).await;
    let side_row = set_row(&side_plank, &side_set);
    assert_time_inputs(&driver, &side_row).await;
    browser::set_input(
        &driver,
        &format!("{side_row} [data-set-side-duration][data-side=\"left\"]"),
        "00:30",
    )
    .await;
    browser::set_input(
        &driver,
        &format!("{side_row} [data-set-side-duration][data-side=\"right\"]"),
        "00:40",
    )
    .await;
    wait_for_complete_ready(&driver, &side_row).await;
    complete_set(&driver, &side_row).await;
    wait_for_side_durations(&server, &side_set, 30, 40).await;

    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;
    finish_and_check_the_summary(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=record-time", provider.origin()))
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

async fn add_set(driver: &WebDriver, detail: &str) -> String {
    let selector = format!("{detail} [data-add-set]");
    browser::wait_for_present(driver, &selector).await;
    browser::click(driver, &selector).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let ids = set_ids(driver, detail).await;
        if let Some(id) = ids.last() {
            let row = set_row(detail, id);
            if driver.find(By::Css(&row)).await.is_ok() {
                return id.clone();
            }
        }
        assert!(
            Instant::now() < deadline,
            "the set was not added to {detail}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn set_ids(driver: &WebDriver, detail: &str) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const detail = arguments[0]; \
         done(JSON.stringify(Array.from(document.querySelectorAll(detail + ' [data-set-row]')) \
           .map((element) => element.getAttribute('data-set-id'))));",
        vec![json!(detail)],
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

async fn start_the_stopwatch(driver: &WebDriver, set: &str) {
    browser::wait_for_present(driver, &format!("{set} [data-stopwatch-start]")).await;
    assert!(
        is_enabled(driver, &format!("{set} [data-stopwatch-start]")).await,
        "the stopwatch start button must be enabled"
    );
    browser::click(driver, &format!("{set} [data-stopwatch-start]")).await;
    wait_for_elapsed_at_least(driver, set, 1).await;
}

async fn stop_the_stopwatch(driver: &WebDriver, set: &str) {
    browser::click(driver, &format!("{set} [data-stopwatch-stop]")).await;
    browser::wait_for_attribute(
        driver,
        &format!("{set} [data-stopwatch-elapsed]"),
        "data-stopwatch-running",
        "false",
    )
    .await;
    let elapsed = browser::attribute(
        driver,
        &format!("{set} [data-stopwatch-elapsed]"),
        "data-stopwatch-elapsed",
    )
    .await;
    let text = browser::value(driver, &format!("{set} [data-set-duration]")).await;
    assert_eq!(text, elapsed);
    assert_ne!(duration::parse(&text), None);
}

async fn assert_no_running_stopwatch(driver: &WebDriver, detail: &str) {
    let running = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const detail = arguments[0]; \
         done(JSON.stringify(Array.from(document.querySelectorAll(detail + ' [data-stopwatch-running=\"true\"]')).length));",
        vec![json!(detail)],
    )
    .await;
    assert_eq!(running, json!(0));
}

async fn is_enabled(driver: &WebDriver, selector: &str) -> bool {
    driver
        .find(By::Css(selector))
        .await
        .expect("the stopwatch button must exist")
        .is_enabled()
        .await
        .expect("the stopwatch button state must be readable")
}

async fn press_disabled_stop(driver: &WebDriver, selector: &str) {
    browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const element = document.querySelector(arguments[0]); \
         if (element) { element.click(); } \
         done('clicked');",
        vec![json!(selector)],
    )
    .await;
}

async fn wait_for_elapsed_at_least(driver: &WebDriver, set: &str, seconds: u64) {
    let selector = format!("{set} [data-stopwatch-elapsed]");
    let state = browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const selector = arguments[0]; \
         const minimum = arguments[1]; \
         const deadline = Date.now() + 60000; \
         const timer = setInterval(() => { \
           const element = document.querySelector(selector); \
           const value = Number(element ? element.getAttribute('data-stopwatch-elapsed-seconds') : '0'); \
           if (value >= minimum) { clearInterval(timer); done('true'); } \
           else if (Date.now() > deadline) { clearInterval(timer); done('timeout'); } \
         }, 50);",
        vec![json!(selector), json!(seconds)],
    )
    .await;
    assert_eq!(state, "true");
}

async fn open_drawer(driver: &WebDriver, set: &str) {
    browser::click(driver, &format!("{set} [data-set-open]")).await;
    browser::wait_for_present(driver, "[data-set-drawer]").await;
}

async fn assert_time_inputs(driver: &WebDriver, set: &str) {
    for selector in [
        "[data-set-weight]",
        "[data-set-reps]",
        "[data-set-side-weight]",
        "[data-set-side-reps]",
    ] {
        assert!(
            driver
                .find(By::Css(format!("{set} {selector}")))
                .await
                .is_err(),
            "a time set must not show {selector}"
        );
    }
}

async fn wait_for_empty_time_set(server: &DevServer, set: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let sql = format!(
        "SELECT sg.weight_kg, sg.reps, sg.duration_seconds, sg.momentum_reps, \
         sg.assisted_reps \
         FROM sets s JOIN set_segments sg ON sg.set_id = s.id WHERE s.id = '{set}'"
    );
    loop {
        let row = record_common::optional_row(server, &sql);
        if row.as_ref().is_some_and(|row| {
            row["weight_kg"].is_null()
                && row["reps"].is_null()
                && row["duration_seconds"].is_null()
                && row["momentum_reps"].as_i64() == Some(0)
                && row["assisted_reps"].as_i64() == Some(0)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the empty time set {set} stored weight or reps: {row:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_complete_ready(driver: &WebDriver, set: &str) {
    browser::wait_for_attribute(
        driver,
        &format!("{set} [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
}

async fn complete_set(driver: &WebDriver, set: &str) {
    browser::click(driver, &format!("{set} [data-set-complete]")).await;
}

/// 端末内に保存された、セットのストップウォッチの開始時刻のキーを返す。
async fn stopwatch_started_at_keys(driver: &WebDriver, set_id: &str) -> Vec<String> {
    let keys = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const prefix = 'stopwatch_started_at:' + arguments[0]; \
         const request = indexedDB.open('warrior'); \
         request.onerror = () => done(JSON.stringify('open-error')); \
         request.onsuccess = () => { \
           const transaction = request.result.transaction('meta', 'readonly'); \
           const all = transaction.objectStore('meta').getAllKeys(); \
           all.onerror = () => done(JSON.stringify('keys-error')); \
           all.onsuccess = () => done(JSON.stringify( \
             Array.from(all.result).filter((key) => String(key).startsWith(prefix)))); \
         };",
        vec![json!(set_id)],
    )
    .await;
    keys.as_array()
        .unwrap_or_else(|| panic!("the stopwatch keys must be a list: {keys}"))
        .iter()
        .map(|key| {
            key.as_str()
                .expect("a stopwatch key must be a string")
                .to_owned()
        })
        .collect()
}

async fn wait_for_duration(server: &DevServer, set: &str, expected: u32, rpe: Option<f64>) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let sql = format!(
        "SELECT sg.duration_seconds, sg.weight_kg, sg.reps, sg.momentum_reps, \
         sg.assisted_reps, s.completed, s.rpe \
         FROM sets s JOIN set_segments sg ON sg.set_id = s.id WHERE s.id = '{set}'"
    );
    loop {
        let row = record_common::optional_row(server, &sql);
        if row.as_ref().is_some_and(|row| {
            row["duration_seconds"].as_i64() == Some(i64::from(expected))
                && row["weight_kg"].is_null()
                && row["reps"].is_null()
                && row["momentum_reps"].as_i64() == Some(0)
                && row["assisted_reps"].as_i64() == Some(0)
                && row["completed"].as_i64() == Some(1)
                && match rpe {
                    Some(value) => row["rpe"].as_f64() == Some(value),
                    None => row["rpe"].is_null(),
                }
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the time set {set} was not saved with {expected} seconds: {row:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_side_durations(server: &DevServer, set: &str, left: i64, right: i64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let sql = format!(
        "SELECT sg.left_duration_seconds, sg.right_duration_seconds, \
         sg.left_weight_kg, sg.left_reps, sg.right_weight_kg, sg.right_reps, \
         sg.momentum_reps, sg.assisted_reps, s.completed \
         FROM sets s JOIN set_segments sg ON sg.set_id = s.id WHERE s.id = '{set}'"
    );
    loop {
        let row = record_common::optional_row(server, &sql);
        if row.as_ref().is_some_and(|row| {
            row["left_duration_seconds"].as_i64() == Some(left)
                && row["right_duration_seconds"].as_i64() == Some(right)
                && row["left_weight_kg"].is_null()
                && row["left_reps"].is_null()
                && row["right_weight_kg"].is_null()
                && row["right_reps"].is_null()
                && row["momentum_reps"].as_i64() == Some(0)
                && row["assisted_reps"].as_i64() == Some(0)
                && row["completed"].as_i64() == Some(1)
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the side durations of {set} were not saved: {row:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn finish_and_check_the_summary(driver: &WebDriver) {
    browser::click(driver, "[data-finish]").await;
    browser::wait_for_route(driver, "/summary/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-total-weight-kg").await,
        "0"
    );
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-set-count").await,
        "3"
    );
    let kinds = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-personal-best]')) \
           .map((element) => element.getAttribute('data-personal-best'))));",
        Vec::new(),
    )
    .await;
    assert_eq!(kinds, json!(["longest_duration", "longest_duration"]));
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-personal-best=\"longest_duration\"]')) \
           .map((element) => Number(element.getAttribute('data-personal-best-value'))) \
           .sort((left, right) => left - right)));",
        Vec::new(),
    )
    .await;
    assert_eq!(values, json!([40, 125]));
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

fn detail_selector(exercise_id: &str) -> String {
    format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"]")
}

fn set_row(detail: &str, set_id: &str) -> String {
    format!("{detail} [data-set-id=\"{set_id}\"]")
}
