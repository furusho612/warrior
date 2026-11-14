//! セットの追加、入力、種別の変更、完了、削除の E2E。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`、`REQ-LIB-004`、`REQ-NFR-003`、
//! `REQ-NFR-007`、`REQ-PRF-005`。
//! 通信を遅くした状態でも入力と完了が画面へ反映されること、通信の復帰後に D1 へ
//! 保存されること、自重種目のセットが体重と加重を別の列に保存することを確かめる。

mod support;

use serde_json::{json, Value};
use std::time::{Duration, Instant};
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const PUSHUP_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000012";
const PLANK_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005d";
const PREVIOUS_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5701";
const PREVIOUS_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5702";
const PREVIOUS_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5703";
const PREVIOUS_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5704";
const SERVER_PORT: u16 = 8825;
const PROVIDER_PORT: u16 = 8826;
const FAST_REFLECTION: Duration = Duration::from_secs(2);

/// D1 のセットと段に期待する値。
#[derive(Debug)]
struct SetCheck<'a> {
    kind: &'a str,
    completed: i64,
    body_weight_kg: Option<f64>,
    weight_kg: f64,
    reps: i64,
}

#[tokio::test]
async fn adds_a_set_edits_its_kind_completes_and_deletes_it() {
    let browser = Browser::from_env();
    assert_eq!(
        browser,
        Browser::Chrome,
        "the delayed network check requires Chrome"
    );
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
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    login(&driver, &server, &provider).await;
    let user_id = current_user_id(&driver).await;
    seed_profile(&server, &user_id);
    seed_previous_bench(&server, &user_id);
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
    check_table_height(&driver).await;
    browser::set_input(&driver, &weight_selector(&first_set), "100").await;
    browser::set_input(&driver, &reps_selector(&first_set), "5").await;
    wait_for_complete_ready(&driver, &first_set).await;
    change_kind(&driver, &first_set, "warmup").await;
    wait_for_diff(&driver, "data-exercise-weight-diff", "-500").await;
    wait_for_diff(&driver, "data-training-total-diff", "-500").await;
    wait_for_set(
        &driver,
        &server,
        &first_set,
        SetCheck {
            kind: "warmup",
            completed: 0,
            body_weight_kg: Some(70.0),
            weight_kg: 100.0,
            reps: 5,
        },
    )
    .await;
    change_kind(&driver, &first_set, "normal").await;
    wait_for_diff(&driver, "data-exercise-weight-diff", "0").await;
    wait_for_diff(&driver, "data-training-total-diff", "0").await;
    wait_for_set(
        &driver,
        &server,
        &first_set,
        SetCheck {
            kind: "normal",
            completed: 0,
            body_weight_kg: Some(70.0),
            weight_kg: 100.0,
            reps: 5,
        },
    )
    .await;
    check_fast_input_and_completion(&driver, &first_set).await;
    wait_for_diff(&driver, "data-exercise-weight-diff", "0").await;
    wait_for_diff(&driver, "data-training-total-diff", "0").await;
    wait_for_set(
        &driver,
        &server,
        &first_set,
        SetCheck {
            kind: "normal",
            completed: 1,
            body_weight_kg: Some(70.0),
            weight_kg: 100.0,
            reps: 5,
        },
    )
    .await;
    check_the_completed_set_is_locked(&driver, &first_set).await;

    browser::add_exercise(&driver, PUSHUP_EXERCISE_ID).await;
    browser::select_exercise(&driver, PUSHUP_EXERCISE_ID).await;
    browser::offline(&driver, true).await;
    let bodyweight_set = add_set(&driver).await;
    assert_eq!(
        browser::attribute(&driver, &weight_selector(&bodyweight_set), "placeholder").await,
        "加重"
    );
    assert!(driver
        .find(By::Css(format!(
            "[data-set-id=\"{bodyweight_set}\"] [data-set-duration]"
        )))
        .await
        .is_err());
    browser::set_input(&driver, &weight_selector(&bodyweight_set), "10").await;
    browser::set_input(&driver, &reps_selector(&bodyweight_set), "5").await;
    wait_for_complete_ready(&driver, &bodyweight_set).await;
    browser::click(&driver, &complete_selector(&bodyweight_set)).await;
    wait_for_completed(&driver, &bodyweight_set, "true").await;
    browser::offline(&driver, false).await;
    let bodyweight_row = wait_for_set(
        &driver,
        &server,
        &bodyweight_set,
        SetCheck {
            kind: "normal",
            completed: 1,
            body_weight_kg: Some(70.0),
            weight_kg: 10.0,
            reps: 5,
        },
    )
    .await;
    assert_eq!(bodyweight_row["body_weight_kg"].as_f64(), Some(70.0));
    assert_eq!(bodyweight_row["weight_kg"].as_f64(), Some(10.0));
    check_the_completed_set_is_locked(&driver, &bodyweight_set).await;

    browser::add_exercise(&driver, PLANK_EXERCISE_ID).await;
    browser::select_exercise(&driver, PLANK_EXERCISE_ID).await;
    let time_set = add_set(&driver).await;
    browser::wait_for_present(
        &driver,
        &format!("[data-set-id=\"{time_set}\"] [data-set-duration]"),
    )
    .await;
    assert!(driver
        .find(By::Css(format!(
            "[data-set-id=\"{time_set}\"] [data-set-weight]"
        )))
        .await
        .is_err());
    assert!(driver
        .find(By::Css(format!(
            "[data-set-id=\"{time_set}\"] [data-set-reps]"
        )))
        .await
        .is_err());
    delete_set(&driver, &server, &time_set).await;

    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    let second_set = add_set(&driver).await;
    browser::set_input(&driver, &weight_selector(&second_set), "80").await;
    browser::set_input(&driver, &reps_selector(&second_set), "5").await;
    wait_for_complete_ready(&driver, &second_set).await;
    check_reorder(&driver, &server, &first_set, &second_set).await;
    delete_set(&driver, &server, &second_set).await;

    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;
    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=record-set", provider.origin()))
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
    let rows = set_ids(driver).await;
    rows.last().cloned().expect("the new set id")
}

async fn check_fast_input_and_completion(driver: &WebDriver, set_id: &str) {
    browser::set_network(driver, false, 5000).await;
    browser::set_input(driver, &weight_selector(set_id), "100").await;
    browser::set_input(driver, &reps_selector(set_id), "5").await;
    browser::wait_for_value_within(driver, &weight_selector(set_id), "100", FAST_REFLECTION).await;
    browser::wait_for_value_within(driver, &reps_selector(set_id), "5", FAST_REFLECTION).await;
    wait_for_complete_ready(driver, set_id).await;
    browser::click(driver, &complete_selector(set_id)).await;
    browser::wait_for_attribute_within(
        driver,
        &format!("[data-set-id=\"{set_id}\"]"),
        "data-set-completed",
        "true",
        FAST_REFLECTION,
    )
    .await;
    browser::wait_for_attribute_within(
        driver,
        "[data-set-events]",
        "data-set-events",
        "1",
        FAST_REFLECTION,
    )
    .await;
    browser::set_network(driver, false, 0).await;
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

async fn check_the_completed_set_is_locked(driver: &WebDriver, set_id: &str) {
    browser::wait_for_present(driver, "[data-set-completed-locked]").await;
    let row = format!("[data-set-id=\"{set_id}\"]");
    let state = browser::eval_json(
        driver,
        &format!(
            "const done = arguments[arguments.length - 1]; \
             const row = document.querySelector('{row}'); \
             done(JSON.stringify({{\
               kind: row.querySelector('select[data-set-kind]').disabled, \
               weight: row.querySelector('[data-set-weight]').disabled, \
               reps: row.querySelector('[data-set-reps]').disabled, \
               delete: row.querySelector('[data-delete-set]').disabled, \
               complete: row.querySelector('[data-set-complete]').disabled, \
               notice: document.querySelector('[data-set-completed-locked]') !== null \
             }}));"
        ),
        Vec::new(),
    )
    .await;
    assert_eq!(state["kind"], true, "{state:?}");
    assert_eq!(state["weight"], true, "{state:?}");
    assert_eq!(state["reps"], true, "{state:?}");
    assert_eq!(state["delete"], true, "{state:?}");
    assert_eq!(state["complete"], true, "{state:?}");
    assert_eq!(state["notice"], true, "{state:?}");
}

async fn check_reorder(driver: &WebDriver, server: &DevServer, first_set: &str, second_set: &str) {
    browser::wait_for_present(driver, "[data-set-row]").await;
    let rows = set_ids(driver).await;
    assert_eq!(rows, vec![first_set.to_owned(), second_set.to_owned()]);
    let second_row = format!("[data-set-id=\"{second_set}\"]");
    browser::click(driver, &format!("{second_row} [data-set-move-up]")).await;
    let rows = set_ids(driver).await;
    assert_eq!(rows, vec![second_set.to_owned(), first_set.to_owned()]);
    wait_for_position(driver, server, second_set, 1).await;
    wait_for_position(driver, server, first_set, 2).await;
}

async fn delete_set(driver: &WebDriver, server: &DevServer, set_id: &str) {
    let row = format!("[data-set-id=\"{set_id}\"]");
    browser::click(driver, &format!("{row} [data-delete-set]")).await;
    browser::wait_for_absent(driver, &row).await;
    wait_for_deleted(driver, server, set_id).await;
}

async fn wait_for_set(
    driver: &WebDriver,
    server: &DevServer,
    set_id: &str,
    check: SetCheck<'_>,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    let sql = format!(
        "SELECT s.kind, s.completed, s.body_weight_kg, sg.weight_kg, sg.reps \
         FROM sets s JOIN set_segments sg ON sg.set_id = s.id \
         WHERE s.id = '{set_id}'"
    );
    let mut last = None;
    loop {
        browser::online(driver).await;
        if let Some(row) = optional_row(server, &sql) {
            if row["kind"] == check.kind
                && row["completed"].as_i64() == Some(check.completed)
                && row["body_weight_kg"].as_f64() == check.body_weight_kg
                && row["weight_kg"].as_f64() == Some(check.weight_kg)
                && row["reps"].as_i64() == Some(check.reps)
            {
                return row;
            }
            last = Some(row);
        }
        if Instant::now() >= deadline {
            panic!(
                "the set {set_id} did not become {check:?}: {sql}; last: {last:?}; diagnostics: {}",
                diagnostics(driver, server).await
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_position(driver: &WebDriver, server: &DevServer, set_id: &str, position: i64) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        browser::online(driver).await;
        let row = optional_row(
            server,
            &format!("SELECT position FROM sets WHERE id = '{set_id}'"),
        );
        if row.as_ref().and_then(|row| row["position"].as_i64()) == Some(position) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the set {set_id} did not become position {position}: {row:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_deleted(driver: &WebDriver, server: &DevServer, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let sql = format!("SELECT COUNT(*) AS count FROM sets WHERE id = '{set_id}'");
    let segment_sql =
        format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{set_id}'");
    loop {
        browser::online(driver).await;
        let sets = optional_row(server, &sql)
            .and_then(|row| row["count"].as_i64())
            .unwrap_or(0);
        let segments = optional_row(server, &segment_sql)
            .and_then(|row| row["count"].as_i64())
            .unwrap_or(0);
        if sets == 0 && segments == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the set {set_id} was not deleted: sets {sets}, segments {segments}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn diagnostics(driver: &WebDriver, server: &DevServer) -> String {
    let outbox = browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const request = indexedDB.open('warrior'); \
         request.onerror = () => done(JSON.stringify([])); \
         request.onsuccess = () => { \
           const db = request.result; \
           const all = db.transaction('outbox', 'readonly').objectStore('outbox').getAll(); \
           all.onsuccess = () => { \
             const rows = all.result.map((entry) => ({kind: entry.operation.kind, state: entry.state, payload: entry.operation.payload})); \
             db.close(); \
             done(JSON.stringify(rows)); \
           }; \
           all.onerror = () => { db.close(); done(JSON.stringify([])); }; \
         };",
        Vec::new(),
    )
    .await;
    let logs = server
        .log_lines()
        .into_iter()
        .filter(|line| line.contains("sync") || line.contains("error") || line.contains("/api"))
        .collect::<Vec<_>>();
    let applied = server
        .execute_d1_json(
            "SELECT kind, COUNT(*) AS count FROM applied_operations GROUP BY kind ORDER BY kind",
        )
        .unwrap_or_default();
    let browser_logs = driver
        .get_log("browser")
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.message)
        .collect::<Vec<_>>();
    format!("outbox: {outbox}; applied: {applied}; browser: {browser_logs:?}; logs: {logs:?}")
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

async fn wait_for_completed(driver: &WebDriver, set_id: &str, expected: &str) {
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"]"),
        "data-set-completed",
        expected,
    )
    .await;
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

fn weight_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-weight]")
}

fn reps_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-reps]")
}

fn complete_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-complete]")
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

fn seed_previous_bench(server: &DevServer, user_id: &str) {
    let snapshot = json!({
        "exercise_id": BENCH_EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    for sql in [
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, total_weight_kg, \
              created_at, updated_at) \
             VALUES ('{PREVIOUS_TRAINING_ID}', '{user_id}', '2026-01-01', \
              '2026-01-01T03:00:00Z', '', 'empty', '2026-01-01T04:00:00Z', 500.0, \
              '2026-01-01T03:00:00Z', '2026-01-01T04:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{PREVIOUS_ROW_ID}', '{user_id}', '{PREVIOUS_TRAINING_ID}', \
              '{BENCH_EXERCISE_ID}', '{}', 1, '2026-01-01T03:00:00Z', \
              '2026-01-01T03:00:00Z')",
            snapshot.to_string().replace('\'', "''")
        ),
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{PREVIOUS_SET_ID}', '{user_id}', '{PREVIOUS_ROW_ID}', 1, 'normal', 1, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              created_at, updated_at) \
             VALUES ('{PREVIOUS_SEGMENT_ID}', '{user_id}', '{PREVIOUS_SET_ID}', 1, 100.0, 5, 0, 0, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the previous training must be seeded");
    }
}

async fn wait_for_diff(driver: &WebDriver, attribute: &str, expected: &str) {
    browser::wait_for_attribute(driver, &format!("[{attribute}]"), attribute, expected).await;
}

async fn check_table_height(driver: &WebDriver) {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const table = document.querySelector('[data-set-table]'); \
         const rows = Array.from(document.querySelectorAll('[data-set-row]')) \
           .map((row) => row.getBoundingClientRect().height); \
         done(JSON.stringify({ \
           table: table ? table.getBoundingClientRect().height : 0, \
           rows \
         }));",
        Vec::new(),
    )
    .await;
    assert_eq!(value["table"].as_f64(), Some(222.0));
    let rows = value["rows"].as_array().expect("the row heights");
    assert!(!rows.is_empty(), "the selected table must have a row");
    for row in rows {
        assert_eq!(row.as_f64(), Some(44.0));
    }
}

fn optional_row(server: &DevServer, sql: &str) -> Option<Value> {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
}
