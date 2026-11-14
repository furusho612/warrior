//! 空の状態からトレーニングを開始し、終了してサマリーを表示する E2E。
//!
//! 対応する要求: `REQ-REC-001`、`REQ-REC-005`、`REQ-NFR-002`。
//! 種目とセットの入力の画面は後続の issue が担うため、このテストは D1 に種目、セット、段を置いて
//! 記録を作る。記録画面の枠、終了、サマリーの値、終了済みのトレーニングが復元されないことを
//! 実ブラウザで確かめる。

mod support;

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;
use warrior_core::exercise::Laterality;
use warrior_core::set_calc::RecordMode;
use warrior_core::training_session::{BestKind, PersonalBestItem};

const BENCH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4701";
const PLANK_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4702";

#[tokio::test]
async fn starts_finishes_and_shows_the_summary_in_the_browser() {
    let browser = Browser::from_env();
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;
    set_smartphone_viewport(&driver, browser).await;

    login(&driver, &server, &provider).await;
    let training_id = start_training(&driver, browser).await;
    sync_create_and_seed(&driver, &server, &training_id).await;
    if browser == Browser::Chrome {
        verify_offline_finish_is_rejected(&driver, &training_id).await;
    }
    finish_and_verify_summary(&driver, &server, &training_id).await;
    verify_home_does_not_restore(&driver, &server).await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn set_smartphone_viewport(driver: &WebDriver, browser: Browser) {
    if browser != Browser::Chrome {
        return;
    }
    driver
        .cdp()
        .send_raw(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": 390,
                "height": 844,
                "deviceScaleFactor": 1.0,
                "mobile": true
            }),
        )
        .await
        .expect("the smartphone viewport must be set");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=training-session",
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

async fn start_training(driver: &WebDriver, browser: Browser) -> String {
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
    check_record_layout(driver, browser).await;
    training_id
}

async fn sync_create_and_seed(driver: &WebDriver, server: &DevServer, training_id: &str) {
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(driver, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-session-id]", "data-session-id").await,
        training_id,
        "the record must be restored while it is active"
    );
    wait_for_session_row(server, training_id).await;

    let me = browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; fetch('/api/me').then((response) => response.text()).then((text) => done(text));",
        Vec::new(),
    )
    .await;
    let user_id = serde_json::from_str::<Value>(&me)
        .expect("the me response is JSON")
        .get("id")
        .and_then(|value| value.as_str())
        .expect("the user id")
        .to_owned();
    seed_sets(server, &user_id, training_id);
}

async fn verify_offline_finish_is_rejected(driver: &WebDriver, training_id: &str) {
    browser::offline(driver, true).await;
    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys("オフライン")
        .await
        .expect("the memo must be typed");
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "unsaved").await;

    driver
        .find(By::Css("[data-finish]"))
        .await
        .expect("the finish button must exist")
        .click()
        .await
        .expect("the finish button must be clickable");
    browser::wait_for_text(driver, "送信中の操作があります").await;
    assert_eq!(
        browser::attribute(driver, "[data-session-id]", "data-session-id").await,
        training_id,
        "a rejected finish must keep the record open"
    );

    browser::offline(driver, false).await;
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "saved").await;
}

async fn finish_and_verify_summary(driver: &WebDriver, server: &DevServer, training_id: &str) {
    driver
        .find(By::Css("[data-finish]"))
        .await
        .expect("the finish button must exist")
        .click()
        .await
        .expect("the finish button must be clickable");
    browser::wait_for_route(driver, "/summary/:training_id").await;

    let stored = wait_for_finished_row(server, training_id).await;
    browser::wait_for_attribute(
        driver,
        "[data-summary]",
        "data-duration-seconds",
        &stored["duration_seconds"]
            .as_i64()
            .expect("a stored duration")
            .to_string(),
    )
    .await;
    let total_weight = browser::attribute(driver, "[data-summary]", "data-total-weight-kg")
        .await
        .parse::<f64>()
        .expect("a rendered total weight");
    assert_eq!(Some(total_weight), stored["total_weight_kg"].as_f64());
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-exercise-count").await,
        "2"
    );
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-set-count").await,
        "3"
    );
    assert_eq!(stored["total_weight_kg"], 1_640.0);
    let training_date = stored["date"].as_str().expect("the training date");
    let expected_bests = session_bests();
    let best_rows = loaded_personal_bests(server, training_id);
    assert_eq!(best_rows.len(), expected_bests.len());
    for item in &expected_bests {
        assert_best_row(&best_rows, item, training_date);
    }
    let rendered = eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const items = Array.from(document.querySelectorAll('[data-personal-best]')).map((element) => { \
           const name = element.querySelector('[data-personal-best-name]'); \
           const label = element.querySelector('[data-personal-best-kind]'); \
           const amount = element.querySelector('[data-personal-best-amount]'); \
           return { \
             kind: element.getAttribute('data-personal-best'), \
             value: Number(element.getAttribute('data-personal-best-value')), \
             name: name ? name.textContent : '', \
             label: label ? label.textContent : '', \
             amount: amount ? amount.textContent : '', \
             align: amount ? getComputedStyle(amount).textAlign : '' \
           }; \
         }); \
         done(JSON.stringify(items));",
    )
    .await;
    assert_eq!(rendered.as_array().map(Vec::len), Some(4));
    for (index, item) in expected_bests.iter().enumerate() {
        assert_eq!(rendered[index]["kind"], item.kind.as_str());
        assert_eq!(rendered[index]["value"].as_f64(), Some(item.value));
        assert_eq!(rendered[index]["align"], "right");
    }
    assert_eq!(rendered[0]["name"], "ベンチプレス");
    assert_eq!(rendered[0]["label"], "最大重量");
    assert_eq!(rendered[0]["amount"], "100 kg");
    assert_eq!(rendered[1]["label"], "推定 1RM");
    assert_eq!(rendered[1]["amount"], "116.6 kg");
    assert_eq!(rendered[2]["label"], "種目別の総重量");
    assert_eq!(rendered[2]["amount"], "1640.0 kg");
    assert_eq!(rendered[3]["name"], "プランク");
    assert_eq!(rendered[3]["label"], "最長時間");
    assert_eq!(rendered[3]["amount"], "1分0秒");
    browser::wait_for_text(driver, "更新した自己ベスト").await;
}

async fn verify_home_does_not_restore(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must return to the home page");
    browser::wait_for_route(driver, "/").await;
    browser::wait_for_attribute(
        driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
    let pathname = driver
        .execute("return window.location.pathname;", Vec::<Value>::new())
        .await
        .expect("the path must be readable")
        .json()
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(pathname, "/", "a finished training must not be restored");
    let has_session = driver
        .execute(
            "return document.querySelector('[data-session-id]') !== null;",
            Vec::<Value>::new(),
        )
        .await
        .expect("the record must be queryable")
        .json()
        .as_bool()
        .unwrap_or(true);
    assert!(!has_session);
}

async fn check_record_layout(driver: &WebDriver, browser: Browser) {
    assert_eq!(
        browser::attribute(driver, "[data-exercise-rail]", "data-exercise-rail").await,
        "true"
    );
    assert_eq!(
        browser::attribute(driver, "[data-rest-timer]", "data-rest-timer").await,
        "true"
    );
    assert_eq!(
        browser::attribute(driver, "[data-set-table]", "data-set-rows").await,
        "5"
    );
    assert_eq!(
        browser::attribute(driver, "[data-set-table]", "data-set-row-height").await,
        "44"
    );
    let rail_width = driver
        .execute(
            "return document.querySelector('[data-exercise-rail]').getBoundingClientRect().width;",
            Vec::<Value>::new(),
        )
        .await
        .expect("the rail must be measurable")
        .json()
        .as_f64()
        .expect("a width");
    assert_eq!(rail_width, 60.0);
    let table_height = driver
        .execute(
            "return document.querySelector('[data-set-table]').getBoundingClientRect().height;",
            Vec::<Value>::new(),
        )
        .await
        .expect("the set table must be measurable")
        .json()
        .as_f64()
        .expect("a height");
    assert!(
        table_height >= 222.0,
        "the set table must fit five 44px rows and its borders: {table_height}"
    );
    let row_heights = eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const heights = Array.from(document.querySelectorAll('[data-set-row]')) \
           .map((row) => row.getBoundingClientRect().height); \
         done(JSON.stringify(heights));",
    )
    .await;
    assert_eq!(row_heights.as_array().map(Vec::len), Some(5));
    for height in row_heights
        .as_array()
        .expect("the row heights are an array")
    {
        assert_eq!(height.as_f64(), Some(44.0));
    }
    let overflow = driver
        .execute(
            "return document.documentElement.scrollWidth > window.innerWidth;",
            Vec::<Value>::new(),
        )
        .await
        .expect("the layout must be measurable")
        .json()
        .as_bool()
        .unwrap_or(true);
    assert!(!overflow, "the record screen must not scroll horizontally");
    if browser == Browser::Chrome {
        let width = driver
            .execute("return window.innerWidth;", Vec::<Value>::new())
            .await
            .expect("the viewport must be measurable")
            .json()
            .as_f64()
            .expect("a width");
        let height = driver
            .execute("return window.innerHeight;", Vec::<Value>::new())
            .await
            .expect("the viewport must be measurable")
            .json()
            .as_f64()
            .expect("a height");
        assert_eq!((width, height), (390.0, 844.0));
    }
}

async fn wait_for_session_row(server: &DevServer, training_id: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let row = session_row(server, training_id);
        if row["started_at"] != Value::Null {
            return row;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the session was not synced to D1: {row}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_finished_row(server: &DevServer, training_id: &str) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let row = session_row(server, training_id);
        if row["finished_at"] != Value::Null {
            return row;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the session was not finished in D1: {row}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn session_row(server: &DevServer, training_id: &str) -> Value {
    first_row(
        server,
        &format!(
            "SELECT date, started_at, finished_at, duration_seconds, total_weight_kg, exercise_count, \
             set_count FROM training_sessions WHERE id = '{training_id}'"
        ),
    )
}

fn session_bests() -> Vec<PersonalBestItem> {
    vec![
        session_item(
            BENCH_EXERCISE_ID,
            "ベンチプレス",
            RecordMode::WeightAndReps,
            BestKind::MaxWeight,
            100.0,
        ),
        session_item(
            BENCH_EXERCISE_ID,
            "ベンチプレス",
            RecordMode::WeightAndReps,
            BestKind::EstimatedOneRepMax,
            116.58250529118924,
        ),
        session_item(
            BENCH_EXERCISE_ID,
            "ベンチプレス",
            RecordMode::WeightAndReps,
            BestKind::TotalWeight,
            1_640.0,
        ),
        session_item(
            PLANK_EXERCISE_ID,
            "プランク",
            RecordMode::Time,
            BestKind::LongestDuration,
            60.0,
        ),
    ]
}

fn session_item(
    exercise_id: &str,
    exercise_name: &str,
    record_mode: RecordMode,
    kind: BestKind,
    value: f64,
) -> PersonalBestItem {
    PersonalBestItem {
        exercise_id: exercise_id.to_owned(),
        exercise_name: Some(exercise_name.to_owned()),
        record_mode,
        laterality: Laterality::Bilateral,
        grip: None,
        attachment: None,
        kind,
        value,
    }
}

fn loaded_personal_bests(server: &DevServer, training_id: &str) -> Vec<Value> {
    server
        .execute_d1_json(&format!(
            "SELECT exercise_id, grip, attachment, kind, value, achieved_on, training_id \
             FROM personal_bests WHERE training_id = '{training_id}' ORDER BY kind, exercise_id"
        ))
        .expect("the personal bests must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .expect("the personal best rows")
}

fn assert_best_row(rows: &[Value], expected: &PersonalBestItem, achieved_on: &str) {
    let row = rows
        .iter()
        .find(|row| {
            row["exercise_id"] == expected.exercise_id
                && row["kind"] == expected.kind.as_str()
                && row["grip"].as_str().unwrap_or_default()
                    == expected.grip.as_deref().unwrap_or_default()
                && row["attachment"].as_str().unwrap_or_default()
                    == expected.attachment.as_deref().unwrap_or_default()
        })
        .unwrap_or_else(|| panic!("the personal best {expected:?} must exist: {rows:?}"));
    assert!((row["value"].as_f64().expect("a value") - expected.value).abs() < 1e-9);
    assert_eq!(row["achieved_on"], achieved_on);
}

fn seed_sets(server: &DevServer, user_id: &str, training_id: &str) {
    let bench_snapshot = json!({
        "exercise_id": BENCH_EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    let plank_snapshot = json!({
        "exercise_id": PLANK_EXERCISE_ID,
        "name": "プランク",
        "body_part": "abs",
        "muscles": [],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    let bench_training_exercise_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4711";
    let plank_training_exercise_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4712";
    let warmup_set_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4721";
    let normal_set_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4722";
    let drop_set_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4723";
    let time_set_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4724";
    for sql in [
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{bench_training_exercise_id}', '{user_id}', '{training_id}', '{BENCH_EXERCISE_ID}', '{}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
            json_literal(&bench_snapshot)
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{plank_training_exercise_id}', '{user_id}', '{training_id}', '{PLANK_EXERCISE_ID}', '{}', 2, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')",
            json_literal(&plank_snapshot)
        ),
        set_sql(user_id, bench_training_exercise_id, warmup_set_id, 1, "warmup"),
        set_sql(user_id, bench_training_exercise_id, normal_set_id, 2, "normal"),
        set_sql(user_id, bench_training_exercise_id, drop_set_id, 3, "drop"),
        set_sql(user_id, plank_training_exercise_id, time_set_id, 1, "normal"),
        segment_sql(
            user_id,
            "3f2504e0-4f89-41d3-9a0c-0305e82c4731",
            warmup_set_id,
            1,
            Some(40.0),
            Some(10),
            None,
        ),
        segment_sql(
            user_id,
            "3f2504e0-4f89-41d3-9a0c-0305e82c4732",
            normal_set_id,
            1,
            Some(100.0),
            Some(5),
            None,
        ),
        segment_sql(
            user_id,
            "3f2504e0-4f89-41d3-9a0c-0305e82c4733",
            drop_set_id,
            1,
            Some(90.0),
            Some(8),
            None,
        ),
        segment_sql(
            user_id,
            "3f2504e0-4f89-41d3-9a0c-0305e82c4734",
            drop_set_id,
            2,
            Some(70.0),
            Some(6),
            None,
        ),
        segment_sql(
            user_id,
            "3f2504e0-4f89-41d3-9a0c-0305e82c4735",
            time_set_id,
            1,
            None,
            None,
            Some(60),
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the sets must be seeded");
    }
    server
        .execute_d1_json(&format!(
            "UPDATE sets SET rpe = 8.0 WHERE id = '{normal_set_id}'"
        ))
        .expect("the RPE must be stored");
}

fn set_sql(
    user_id: &str,
    training_exercise_id: &str,
    set_id: &str,
    position: i64,
    kind: &str,
) -> String {
    format!(
        "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
         VALUES ('{set_id}', '{user_id}', '{training_exercise_id}', {position}, '{kind}', 1, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
    )
}

fn segment_sql(
    user_id: &str,
    segment_id: &str,
    set_id: &str,
    position: i64,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> String {
    let weight = weight_kg.map_or("NULL".to_owned(), |value| value.to_string());
    let reps = reps.map_or("NULL".to_owned(), |value| value.to_string());
    let duration = duration_seconds.map_or("NULL".to_owned(), |value| value.to_string());
    format!(
        "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, duration_seconds, created_at, updated_at) \
         VALUES ('{segment_id}', '{user_id}', '{set_id}', {position}, {weight}, {reps}, 0, 0, {duration}, '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
    )
}

fn json_literal(value: &Value) -> String {
    value.to_string().replace('\'', "''")
}

fn first_row(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .expect("a row")
}

async fn eval_json(driver: &WebDriver, script: &str) -> Value {
    let text = driver
        .execute_async(script, Vec::<Value>::new())
        .await
        .expect("the script must run")
        .json()
        .as_str()
        .expect("the script must return a JSON string")
        .to_owned();
    serde_json::from_str(&text).expect("valid JSON")
}
