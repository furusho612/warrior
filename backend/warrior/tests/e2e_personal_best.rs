//! 推定 1RM の表示と、セットの完了で更新した自己ベストの E2E。
//!
//! 対応する要求: `REQ-SET-008`、`REQ-SET-009`。
//! 重量と回数を入力すると推定 1RM が変わり、回数 0 では消え、10 回を超えると参考値の
//! 印が付くことを確かめる。セットの完了の直後に更新した自己ベストを表示し、サマリーの
//! 一覧が同じ内容になることを確かめる。時間で記録する種目では推定 1RM を表示しない。

mod support;

use std::time::{Duration, Instant};

use serde_json::Value;
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;
use warrior_core::set_calc::estimated_one_rep_max;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const PLANK_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005d";
const SERVER_PORT: u16 = 8850;
const PROVIDER_PORT: u16 = 8851;

#[tokio::test]
async fn shows_the_estimated_one_rep_max_and_the_updated_personal_bests() {
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

    let bench = detail_selector(BENCH_EXERCISE_ID);
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    let warmup_set = add_set(&driver, &bench).await;
    let warmup_row = set_row(&bench, &warmup_set);
    change_kind(&driver, &warmup_row, "warmup").await;
    browser::set_input(&driver, &format!("{warmup_row} [data-set-weight]"), "100").await;
    browser::set_input(&driver, &format!("{warmup_row} [data-set-reps]"), "5").await;
    browser::wait_for_absent(&driver, &format!("{warmup_row} [data-one-rm]")).await;

    let first_set = add_set(&driver, &bench).await;
    let first_row = set_row(&bench, &first_set);
    check_the_estimated_one_rep_max(&driver, &first_row).await;
    complete_set(&driver, &first_row).await;
    wait_for_record_bests(
        &driver,
        &[
            ("max_weight", 100.0),
            (
                "estimated_one_rep_max",
                estimated_one_rep_max(100.0, 5).unwrap(),
            ),
            ("total_weight", 500.0),
        ],
    )
    .await;

    let second_set = add_set(&driver, &bench).await;
    let second_row = set_row(&bench, &second_set);
    browser::set_input(&driver, &format!("{second_row} [data-set-weight]"), "80").await;
    browser::set_input(&driver, &format!("{second_row} [data-set-reps]"), "10").await;
    wait_for_complete_ready(&driver, &second_row).await;
    complete_set(&driver, &second_row).await;
    wait_for_record_bests(
        &driver,
        &[
            ("max_weight", 100.0),
            (
                "estimated_one_rep_max",
                estimated_one_rep_max(100.0, 5).unwrap(),
            ),
            ("total_weight", 1_300.0),
        ],
    )
    .await;

    browser::add_exercise(&driver, PLANK_EXERCISE_ID).await;
    let plank = detail_selector(PLANK_EXERCISE_ID);
    browser::select_exercise(&driver, PLANK_EXERCISE_ID).await;
    let plank_set = add_set(&driver, &plank).await;
    let plank_row = set_row(&plank, &plank_set);
    assert!(
        driver
            .find(By::Css(format!("{plank_row} [data-one-rm]")))
            .await
            .is_err(),
        "a time set must not show the estimated one rep max"
    );
    browser::set_input(
        &driver,
        &format!("{plank_row} [data-set-duration]"),
        "01:30",
    )
    .await;
    wait_for_complete_ready(&driver, &plank_row).await;
    complete_set(&driver, &plank_row).await;
    wait_for_record_bests(
        &driver,
        &[
            ("max_weight", 100.0),
            (
                "estimated_one_rep_max",
                estimated_one_rep_max(100.0, 5).unwrap(),
            ),
            ("total_weight", 1_300.0),
            ("longest_duration", 90.0),
        ],
    )
    .await;

    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;
    let record_bests = record_bests(&driver).await;
    finish_and_check_the_summary(&driver, &record_bests).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn change_kind(driver: &WebDriver, set: &str, kind: &str) {
    let selector = format!("{set} select[data-set-kind]");
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
}

async fn check_the_estimated_one_rep_max(driver: &WebDriver, set: &str) {
    let badge = format!("{set} [data-one-rm]");
    let weight = format!("{set} [data-set-weight]");
    let reps = format!("{set} [data-set-reps]");
    browser::set_input(driver, &weight, "100").await;
    browser::set_input(driver, &reps, "1").await;
    browser::wait_for_attribute(driver, &badge, "data-one-rm-value", "100").await;
    assert_eq!(
        browser::attribute(driver, &badge, "data-one-rm-reference").await,
        "false"
    );

    browser::set_input(driver, &reps, "5").await;
    browser::wait_for_value(driver, &reps, "5").await;
    let expected = estimated_one_rep_max(100.0, 5).expect("the formula");
    wait_for_one_rep_max(driver, &badge, expected).await;

    browser::set_input(driver, &reps, "11").await;
    browser::wait_for_attribute(driver, &badge, "data-one-rm-reference", "true").await;
    browser::wait_for_present(driver, &format!("{badge} [data-one-rm-reference-mark]")).await;

    browser::set_input(driver, &reps, "0").await;
    browser::wait_for_absent(driver, &badge).await;
    browser::set_input(driver, &reps, "5").await;
    wait_for_one_rep_max(driver, &badge, expected).await;
}

async fn wait_for_one_rep_max(driver: &WebDriver, badge: &str, expected: f64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let value = browser::attribute(driver, badge, "data-one-rm-value").await;
        if value
            .parse::<f64>()
            .is_ok_and(|value| (value - expected).abs() < 1e-9)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the estimated one rep max of {badge} did not become {expected}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

async fn wait_for_record_bests(driver: &WebDriver, expected: &[(&str, f64)]) {
    let mut expected = expected
        .iter()
        .map(|(kind, value)| ((*kind).to_owned(), *value))
        .collect::<Vec<_>>();
    expected.sort_by(|left, right| left.0.cmp(&right.0));
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let actual = record_bests(driver).await;
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the record bests did not become {expected:?}: {actual:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn record_bests(driver: &WebDriver) -> Vec<(String, f64)> {
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-record-personal-best]')) \
           .map((element) => [element.getAttribute('data-record-personal-best'), \
             Number(element.getAttribute('data-record-personal-best-value'))])));",
        Vec::new(),
    )
    .await;
    let mut items = values
        .as_array()
        .expect("the record bests")
        .iter()
        .map(|item| {
            (
                item[0].as_str().expect("a kind").to_owned(),
                item[1].as_f64().expect("a value"),
            )
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.0.cmp(&right.0));
    items
}

async fn finish_and_check_the_summary(driver: &WebDriver, record_bests: &[(String, f64)]) {
    browser::click(driver, "[data-finish]").await;
    browser::wait_for_route(driver, "/summary/:training_id").await;
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-personal-best]')) \
           .map((element) => [element.getAttribute('data-personal-best'), \
             Number(element.getAttribute('data-personal-best-value'))])));",
        Vec::new(),
    )
    .await;
    let mut summary_bests = values
        .as_array()
        .expect("the summary bests")
        .iter()
        .map(|item| {
            (
                item[0].as_str().expect("a kind").to_owned(),
                item[1].as_f64().expect("a value"),
            )
        })
        .collect::<Vec<_>>();
    summary_bests.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(summary_bests, record_bests);
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=record-personal-best",
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
            "the added set did not appear in {detail}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn set_ids(driver: &WebDriver, detail: &str) -> Vec<String> {
    let values = browser::eval_json(
        driver,
        &format!(
            "const done = arguments[arguments.length - 1]; \
             done(JSON.stringify(Array.from(document.querySelectorAll( \
               '{detail} [data-set-row][data-set-id]')) \
               .map((element) => element.getAttribute('data-set-id'))));"
        ),
        Vec::new(),
    )
    .await;
    values
        .as_array()
        .expect("the set ids")
        .iter()
        .map(|value| value.as_str().expect("a set id").to_owned())
        .collect()
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
