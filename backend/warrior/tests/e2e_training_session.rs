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
use warrior_core::set_calc::estimated_one_rep_max;

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
    finish_and_verify_summary(&driver, &server, &training_id).await;
    verify_home_does_not_restore(&driver, &server).await;

    browser::assert_no_panic(&driver).await;
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
    let personal_bests = stored["personal_bests"]
        .as_str()
        .expect("the stored personal bests are JSON")
        .to_owned();
    let personal_bests: Value =
        serde_json::from_str(&personal_bests).expect("the stored personal bests are JSON");
    assert_eq!(
        personal_bests,
        json!([
            {
                "exercise_id": BENCH_EXERCISE_ID,
                "kind": "max_weight",
                "value": 100.0
            },
            {
                "exercise_id": BENCH_EXERCISE_ID,
                "kind": "estimated_one_rep_max",
                "value": estimated_one_rep_max(100.0, 5).expect("a value")
            },
            {
                "exercise_id": BENCH_EXERCISE_ID,
                "kind": "total_weight",
                "value": 1_140.0
            },
            {
                "exercise_id": PLANK_EXERCISE_ID,
                "kind": "longest_duration",
                "value": 60.0
            }
        ])
    );
    let rendered = eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const items = Array.from(document.querySelectorAll('[data-personal-best]')).map((element) => ({ \
           kind: element.getAttribute('data-personal-best'), \
           value: Number(element.getAttribute('data-personal-best-value')) \
         })); \
         done(JSON.stringify(items));",
    )
    .await;
    assert_eq!(rendered.as_array().map(Vec::len), Some(4));
    for (index, item) in personal_bests
        .as_array()
        .expect("the personal bests are an array")
        .iter()
        .enumerate()
    {
        assert_eq!(rendered[index]["kind"], item["kind"]);
        assert_eq!(rendered[index]["value"].as_f64(), item["value"].as_f64());
    }
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
        browser::attribute(driver, "[data-set-drawer]", "data-set-drawer").await,
        "true"
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
        table_height >= 220.0,
        "the set table must fit five rows: {table_height}"
    );
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
            "SELECT started_at, finished_at, duration_seconds, total_weight_kg, exercise_count, \
             set_count, personal_bests FROM training_sessions WHERE id = '{training_id}'"
        ),
    )
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
