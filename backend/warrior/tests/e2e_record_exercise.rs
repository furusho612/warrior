//! 記録に種目を追加し、前回の値をコピーし、種目ごとと全体の差を表示する E2E。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-REC-003`、`REQ-REC-004`。
//! 種目は種目ライブラリの選択から追加し、前回のセットの値をコピーして入力欄へ入れる。
//! 重量の種目は総重量の差、時間の種目は合計時間と最長時間の差を確かめる。
//! トレーニング全体の差は、時間で記録する種目を除いた総重量の差とする。

mod support;

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const PLANK_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005d";
const SQUAT_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000042";
const PAST_TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a01";
const PAST_BENCH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a02";
const PAST_PLANK_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a03";
const PAST_BENCH_SET_1_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a04";
const PAST_BENCH_SET_2_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a05";
const PAST_PLANK_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a06";
const PAST_BENCH_SEGMENT_1_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a07";
const PAST_BENCH_SEGMENT_2_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a08";
const PAST_PLANK_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a09";
const SERVER_PORT: u16 = 8815;
const PROVIDER_PORT: u16 = 8818;

#[tokio::test]
async fn adds_exercises_copies_the_previous_values_and_shows_the_differences() {
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
    let training_id = start_training(&driver).await;
    wait_for_session_row(&server, &training_id).await;
    let user_id = current_user_id(&driver).await;
    seed_past_session(&server, &user_id);
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the record must reload with the seeded previous training");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    wait_for_attribute(
        &driver,
        "[data-training-total-diff]",
        "data-training-total-diff",
        "-900",
    )
    .await;

    add_exercise(&driver, BENCH_EXERCISE_ID).await;
    add_exercise(&driver, PLANK_EXERCISE_ID).await;
    add_exercise(&driver, SQUAT_EXERCISE_ID).await;
    check_no_previous(&driver, SQUAT_EXERCISE_ID).await;
    check_previous_and_weight_difference(&driver, BENCH_EXERCISE_ID).await;
    check_previous_and_time_difference(&driver, PLANK_EXERCISE_ID).await;
    check_training_difference(&driver).await;

    if browser == Browser::Chrome {
        check_reorder_delete_and_outbox(&driver).await;
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=record-exercise",
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

async fn add_exercise(driver: &WebDriver, exercise_id: &str) {
    browser::wait_for_attribute(driver, "[data-exercises]", "data-exercises", "true").await;
    driver
        .find(By::Css("[data-add-exercise]"))
        .await
        .expect("the add exercise button must exist")
        .click()
        .await
        .expect("the add exercise button must be clickable");
    browser::wait_for_route(driver, "/record/:training_id/exercises").await;
    assert!(
        driver.find(By::Css("[data-exercise-rail]")).await.is_err(),
        "the exercise selection must not show the record rail"
    );
    assert!(
        driver.find(By::Css("[data-rest-timer]")).await.is_err(),
        "the exercise selection must not show the rest timer"
    );
    wait_for_selector(driver, "[data-exercise-choose]").await;
    let selector = format!("[data-exercise-choose][data-exercise-id=\"{exercise_id}\"]");
    let button = driver
        .find(By::Css(&selector))
        .await
        .unwrap_or_else(|error| panic!("the exercise {exercise_id} must exist: {error}"));
    button
        .click()
        .await
        .expect("the exercise must be selectable");
    browser::wait_for_route(driver, "/record/:training_id").await;
    let row = format!("[data-exercise-row][data-exercise-id=\"{exercise_id}\"]");
    wait_for_selector(driver, &row).await;
}

async fn check_previous_and_weight_difference(driver: &WebDriver, exercise_id: &str) {
    select_exercise(driver, exercise_id).await;
    let detail = format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"]");
    let previous = format!("{detail} [data-previous]");
    wait_for_selector(driver, &previous).await;
    wait_for_attribute(driver, &previous, "data-previous-total-weight-kg", "900").await;
    assert_eq!(
        count(driver, &format!("{detail} [data-previous-set]")).await,
        2,
        "the previous record must have two sets"
    );

    driver
        .find(By::Css(format!("{detail} [data-copy-previous]")))
        .await
        .expect("the copy button must exist")
        .click()
        .await
        .expect("the copy button must be clickable");
    let first_weight = format!("{detail} [data-set-row-index=\"0\"] [data-set-weight]");
    let second_weight = format!("{detail} [data-set-row-index=\"1\"] [data-set-weight]");
    browser::wait_for_value(driver, &first_weight, "100").await;
    browser::wait_for_value(driver, &second_weight, "80").await;
    let first_reps = format!("{detail} [data-set-row-index=\"0\"] [data-set-reps]");
    let second_reps = format!("{detail} [data-set-row-index=\"1\"] [data-set-reps]");
    browser::wait_for_value(driver, &first_reps, "5").await;
    browser::wait_for_value(driver, &second_reps, "5").await;
    wait_for_attribute(
        driver,
        &format!("{detail} [data-exercise-weight-diff]"),
        "data-exercise-weight-diff",
        "0",
    )
    .await;

    let input = driver
        .find(By::Css(&first_weight))
        .await
        .expect("the first weight input must exist");
    input.clear().await.expect("the weight must be clearable");
    input
        .send_keys("110")
        .await
        .expect("the weight must be typed");
    wait_for_attribute(
        driver,
        &format!("{detail} [data-exercise-weight-diff]"),
        "data-exercise-weight-diff",
        "50",
    )
    .await;
    wait_for_attribute(
        driver,
        "[data-training-total-diff]",
        "data-training-total-diff",
        "50",
    )
    .await;
}

async fn check_previous_and_time_difference(driver: &WebDriver, exercise_id: &str) {
    select_exercise(driver, exercise_id).await;
    let detail = format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"]");
    let previous = format!("{detail} [data-previous]");
    wait_for_selector(driver, &previous).await;
    wait_for_attribute(
        driver,
        &previous,
        "data-previous-total-duration-seconds",
        "60",
    )
    .await;
    wait_for_attribute(
        driver,
        &previous,
        "data-previous-max-duration-seconds",
        "60",
    )
    .await;

    driver
        .find(By::Css(format!("{detail} [data-copy-previous]")))
        .await
        .expect("the copy button must exist")
        .click()
        .await
        .expect("the copy button must be clickable");
    let duration = format!("{detail} [data-set-row-index=\"0\"] [data-set-duration]");
    browser::wait_for_value(driver, &duration, "01:00").await;
    wait_for_attribute(
        driver,
        &format!("{detail} [data-exercise-total-duration-diff]"),
        "data-exercise-total-duration-diff",
        "0",
    )
    .await;

    let input = driver
        .find(By::Css(&duration))
        .await
        .expect("the duration input must exist");
    input.clear().await.expect("the duration must be clearable");
    input
        .send_keys("90")
        .await
        .expect("the duration must be typed");
    wait_for_attribute(
        driver,
        &format!("{detail} [data-exercise-total-duration-diff]"),
        "data-exercise-total-duration-diff",
        "30",
    )
    .await;
    wait_for_attribute(
        driver,
        &format!("{detail} [data-exercise-longest-duration-diff]"),
        "data-exercise-longest-duration-diff",
        "30",
    )
    .await;
}

async fn check_training_difference(driver: &WebDriver) {
    wait_for_attribute(
        driver,
        "[data-training-total-diff]",
        "data-training-total-diff",
        "50",
    )
    .await;
}

async fn check_no_previous(driver: &WebDriver, exercise_id: &str) {
    select_exercise(driver, exercise_id).await;
    let detail = format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"]");
    assert!(
        driver
            .find(By::Css(format!("{detail} [data-previous]")))
            .await
            .is_err(),
        "an exercise without a previous record must not show the previous values"
    );
    assert!(
        driver
            .find(By::Css(format!("{detail} [data-exercise-weight-diff]")))
            .await
            .is_err(),
        "an exercise without a previous record must not show a difference"
    );
}

async fn select_exercise(driver: &WebDriver, exercise_id: &str) {
    let row = format!("[data-exercise-row][data-exercise-id=\"{exercise_id}\"]");
    wait_for_selector(driver, &row).await;
    driver
        .find(By::Css(&row))
        .await
        .expect("the exercise row must exist")
        .click()
        .await
        .expect("the exercise row must be clickable");
    browser::wait_for_attribute(driver, &row, "data-selected", "true").await;
}

async fn check_reorder_delete_and_outbox(driver: &WebDriver) {
    browser::offline(driver, true).await;

    select_exercise(driver, PLANK_EXERCISE_ID).await;
    driver
        .find(By::Css("[data-exercise-detail] [data-move-down]"))
        .await
        .expect("the move down button must exist")
        .click()
        .await
        .expect("the move down button must be clickable");
    wait_for_exercise_order(
        driver,
        &[BENCH_EXERCISE_ID, SQUAT_EXERCISE_ID, PLANK_EXERCISE_ID],
    )
    .await;
    assert!(
        outbox_kinds(driver)
            .await
            .iter()
            .any(|kind| kind == "training_exercise_reorder"),
        "the reorder must join the outbox"
    );

    select_exercise(driver, SQUAT_EXERCISE_ID).await;
    driver
        .find(By::Css("[data-exercise-detail] [data-delete-exercise]"))
        .await
        .expect("the delete button must exist")
        .click()
        .await
        .expect("the delete button must be clickable");
    wait_for_exercise_order(driver, &[BENCH_EXERCISE_ID, PLANK_EXERCISE_ID]).await;
    assert!(
        outbox_kinds(driver)
            .await
            .iter()
            .any(|kind| kind == "training_exercise_delete"),
        "the delete must join the outbox"
    );

    browser::offline(driver, false).await;
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "saved").await;
}

async fn wait_for_exercise_order(driver: &WebDriver, expected: &[&str]) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let order = exercise_order(driver).await;
        if order
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the exercise order did not become {expected:?}: {order:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn exercise_order(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-exercise-row]')) \
           .map((element) => element.getAttribute('data-exercise-id'))));",
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

async fn outbox_kinds(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const request = indexedDB.open('warrior'); \
         request.onerror = () => done(JSON.stringify([])); \
         request.onsuccess = () => { \
           const db = request.result; \
           const all = db.transaction('outbox', 'readonly').objectStore('outbox').getAll(); \
           all.onsuccess = () => { \
             const kinds = all.result.map((entry) => entry.operation.kind); \
             db.close(); \
             done(JSON.stringify(kinds)); \
           }; \
           all.onerror = () => { db.close(); done(JSON.stringify([])); }; \
         };",
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

async fn wait_for_selector(driver: &WebDriver, selector: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if driver.find(By::Css(selector)).await.is_ok() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the selector {selector} did not appear"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn count(driver: &WebDriver, selector: &str) -> usize {
    driver
        .execute(
            "return document.querySelectorAll(arguments[0]).length;",
            vec![json!(selector)],
        )
        .await
        .expect("the elements must be countable")
        .json()
        .as_u64()
        .unwrap_or(0) as usize
}

async fn wait_for_attribute(driver: &WebDriver, selector: &str, attribute: &str, expected: &str) {
    browser::wait_for_attribute(driver, selector, attribute, expected).await;
}

async fn wait_for_session_row(server: &DevServer, training_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let row = optional_row(
            server,
            &format!("SELECT started_at FROM training_sessions WHERE id = '{training_id}'"),
        );
        match row {
            Some(row) if row["started_at"] != Value::Null => return,
            _ => {}
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the session was not synced to D1: {row:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn seed_past_session(server: &DevServer, user_id: &str) {
    let bench_snapshot = json!({
        "exercise_id": BENCH_EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    let plank_snapshot = json!({
        "exercise_id": PLANK_EXERCISE_ID,
        "name": "プランク",
        "body_part": "abs",
        "muscles": [{"muscle": "rectus_abdominis_upper", "role": "primary"}],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    for sql in [
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, duration_seconds, \
              total_weight_kg, exercise_count, set_count, created_at, updated_at) \
             VALUES ('{PAST_TRAINING_ID}', '{user_id}', '2026-01-01', '2026-01-01T03:00:00Z', '', \
              'empty', '2026-01-01T04:00:00Z', 3600, 900.0, 2, 3, \
              '2026-01-01T03:00:00Z', '2026-01-01T04:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{PAST_BENCH_ROW_ID}', '{user_id}', '{PAST_TRAINING_ID}', '{BENCH_EXERCISE_ID}', \
              '{}', 1, '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')",
            json_literal(&bench_snapshot)
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{PAST_PLANK_ROW_ID}', '{user_id}', '{PAST_TRAINING_ID}', '{PLANK_EXERCISE_ID}', \
              '{}', 2, '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')",
            json_literal(&plank_snapshot)
        ),
        set_sql(user_id, PAST_BENCH_ROW_ID, PAST_BENCH_SET_1_ID, 1),
        set_sql(user_id, PAST_BENCH_ROW_ID, PAST_BENCH_SET_2_ID, 2),
        set_sql(user_id, PAST_PLANK_ROW_ID, PAST_PLANK_SET_ID, 1),
        segment_sql(
            user_id,
            PAST_BENCH_SEGMENT_1_ID,
            PAST_BENCH_SET_1_ID,
            Some(100.0),
            Some(5),
            None,
        ),
        segment_sql(
            user_id,
            PAST_BENCH_SEGMENT_2_ID,
            PAST_BENCH_SET_2_ID,
            Some(80.0),
            Some(5),
            None,
        ),
        segment_sql(
            user_id,
            PAST_PLANK_SEGMENT_ID,
            PAST_PLANK_SET_ID,
            None,
            None,
            Some(60),
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the past session must be seeded");
    }
}

fn set_sql(user_id: &str, row_id: &str, set_id: &str, position: i64) -> String {
    format!(
        "INSERT INTO sets \
         (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
         VALUES ('{set_id}', '{user_id}', '{row_id}', {position}, 'normal', 1, \
          '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
    )
}

fn segment_sql(
    user_id: &str,
    segment_id: &str,
    set_id: &str,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> String {
    let weight = weight_kg.map_or("NULL".to_owned(), |value| value.to_string());
    let reps = reps.map_or("NULL".to_owned(), |value| value.to_string());
    let duration = duration_seconds.map_or("NULL".to_owned(), |value| value.to_string());
    format!(
        "INSERT INTO set_segments \
         (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
          duration_seconds, created_at, updated_at) \
         VALUES ('{segment_id}', '{user_id}', '{set_id}', 1, {weight}, {reps}, 0, 0, {duration}, \
          '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
    )
}

fn json_literal(value: &Value) -> String {
    value.to_string().replace('\'', "''")
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
