//! 代わりの種目の候補の表示、入れ替え、後に回す操作の E2E。
//!
//! 対応する要求: `REQ-ALT-001`、`REQ-ALT-002`、`REQ-ALT-003`、`REQ-ALT-004`、
//! `REQ-ALT-005`、`REQ-ALT-006`。
//! 候補を端末内の種目ライブラリと過去に記録した種目の ID から並べ、入れ替えと
//! 後に回す操作が送信待ちの列を通ることを確かめる。通信を切った状態でも候補が
//! 同じように表示されることを確かめる。

mod support;

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8874;
const PROVIDER_PORT: u16 = 8875;
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const DUMBBELL_FLY: &str = "a7c4e2f0-0000-4000-8000-000000000006";
const DUMBBELL_PRESS: &str = "a7c4e2f0-0000-4000-8000-000000000007";
const SMITH_BENCH: &str = "a7c4e2f0-0000-4000-8000-00000000000b";
const CHEST_PRESS: &str = "a7c4e2f0-0000-4000-8000-00000000000c";
const PEC_FLY: &str = "a7c4e2f0-0000-4000-8000-00000000000d";
const CABLE_MIDDLE: &str = "a7c4e2f0-0000-4000-8000-000000000010";
const PUSH_UP: &str = "a7c4e2f0-0000-4000-8000-000000000012";
const PAST_TRAINING: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b01";
const PAST_ROW: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b02";
const PAST_SET: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b03";
const PAST_SEGMENT: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b04";

#[tokio::test]
async fn shows_replaces_and_defers_alternative_exercises_offline_and_online() {
    let browser = Browser::Chrome;
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
    seed_past_session(&server, &user_id);
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload after the past record is seeded");
    browser::wait_for_route(&driver, "/").await;
    wait_for_alternative_cache(&driver).await;

    start_training(&driver).await;
    browser::add_exercise(&driver, BENCH).await;
    check_candidate_order(&driver).await;

    browser::offline(&driver, true).await;
    choose_alternative(&driver, DUMBBELL_PRESS).await;
    wait_for_detail(&driver, DUMBBELL_PRESS).await;
    check_outbox_has(&driver, "training_exercise_create").await;
    browser::offline(&driver, false).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;
    reload_active_record(&driver, &server).await;
    browser::select_exercise(&driver, DUMBBELL_PRESS).await;
    check_previous_record(&driver, DUMBBELL_PRESS).await;

    browser::add_exercise(&driver, CABLE_MIDDLE).await;
    defer_and_check_the_order(&driver).await;
    check_outbox_has(&driver, "training_exercise_reorder").await;
    browser::offline(&driver, false).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    browser::select_exercise(&driver, DUMBBELL_PRESS).await;
    add_completed_set(&driver).await;
    check_completed_set_confirmation(&driver, DUMBBELL_PRESS, DUMBBELL_FLY).await;
    check_offline_candidates(&driver, DUMBBELL_FLY).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=record-alternative",
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
    browser::click(driver, "[data-start]").await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    let training_id = browser::attribute(driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty(), "the record must have a session id");
    training_id
}

async fn check_candidate_order(driver: &WebDriver) {
    open_alternatives(driver).await;
    let expected = [
        DUMBBELL_PRESS,
        DUMBBELL_FLY,
        SMITH_BENCH,
        CHEST_PRESS,
        PEC_FLY,
        CABLE_MIDDLE,
        PUSH_UP,
    ];
    let ids = alternative_ids(driver).await;
    assert_eq!(
        ids,
        expected
            .iter()
            .map(|id| (*id).to_owned())
            .collect::<Vec<_>>(),
        "the recorded candidate must come first and the rest must follow the library order"
    );
}

async fn check_previous_record(driver: &WebDriver, exercise_id: &str) {
    let previous =
        format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"] [data-previous]");
    browser::wait_for_present(driver, &previous).await;
    browser::wait_for_attribute(driver, &previous, "data-previous-total-weight-kg", "200").await;
    let sets_selector =
        format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"] [data-previous-set]");
    let sets = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(document.querySelectorAll(arguments[0]).length));",
        vec![json!(sets_selector)],
    )
    .await;
    assert_eq!(sets, json!(1), "the previous record must keep its set");
}

async fn reload_active_record(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(driver, "/record/:training_id").await;
}

async fn defer_and_check_the_order(driver: &WebDriver) {
    browser::select_exercise(driver, DUMBBELL_PRESS).await;
    browser::offline(driver, true).await;
    browser::click(driver, "[data-defer-exercise]").await;
    wait_for_exercise_order(driver, &[CABLE_MIDDLE, DUMBBELL_PRESS]).await;
}

async fn check_completed_set_confirmation(driver: &WebDriver, current: &str, next: &str) {
    open_alternatives(driver).await;
    choose_alternative(driver, next).await;
    browser::wait_for_present(driver, "[data-alternative-confirmation]").await;
    browser::click(driver, "[data-alternative-cancel]").await;
    wait_for_detail(driver, current).await;

    choose_alternative(driver, next).await;
    browser::wait_for_present(driver, "[data-alternative-confirmation]").await;
    browser::click(driver, "[data-alternative-confirm]").await;
    wait_for_detail(driver, next).await;
    browser::wait_for_present(driver, "[data-set-completed=\"true\"]").await;
}

async fn check_offline_candidates(driver: &WebDriver, origin: &str) {
    browser::offline(driver, true).await;
    open_alternatives(driver).await;
    let ids = alternative_ids(driver).await;
    assert_eq!(
        ids,
        vec![
            BENCH.to_owned(),
            SMITH_BENCH.to_owned(),
            CHEST_PRESS.to_owned(),
            PEC_FLY.to_owned(),
            CABLE_MIDDLE.to_owned(),
            PUSH_UP.to_owned(),
            DUMBBELL_PRESS.to_owned(),
        ],
        "the offline candidates must follow the same order as the cached library"
    );
    assert!(
        !ids.iter().any(|id| id == origin),
        "the origin must not be a candidate"
    );
    browser::offline(driver, false).await;
}

async fn open_alternatives(driver: &WebDriver) {
    if driver
        .find(By::Css("[data-alternatives-close]"))
        .await
        .is_ok()
    {
        browser::click(driver, "[data-alternatives-close]").await;
        browser::wait_for_absent(driver, "[data-alternatives]").await;
    }
    let selector = "[data-alternatives-open]";
    browser::wait_for_present(driver, selector).await;
    browser::click(driver, selector).await;
    browser::wait_for_present(driver, "[data-alternatives]").await;
}

async fn choose_alternative(driver: &WebDriver, exercise_id: &str) {
    let selector = format!("[data-alternative][data-exercise-id=\"{exercise_id}\"]");
    browser::wait_for_present(driver, &selector).await;
    browser::click(driver, &selector).await;
}

async fn wait_for_detail(driver: &WebDriver, exercise_id: &str) {
    let selector = format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"]");
    browser::wait_for_present(driver, &selector).await;
}

async fn alternative_ids(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-alternative]')) \
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

async fn wait_for_exercise_order(driver: &WebDriver, expected: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let value = browser::eval_json(
            driver,
            "const done = arguments[arguments.length - 1]; \
             done(JSON.stringify(Array.from(document.querySelectorAll('[data-exercise-row]')) \
               .map((element) => element.getAttribute('data-exercise-id'))));",
            Vec::new(),
        )
        .await;
        let order = value
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if order
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the exercise order did not become {expected:?}: {order:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn add_completed_set(driver: &WebDriver) {
    browser::wait_for_present(driver, "[data-add-set]").await;
    browser::click(driver, "[data-add-set]").await;
    browser::wait_for_present(driver, "[data-set-row]").await;
    let set_id = browser::set_ids(driver)
        .await
        .last()
        .cloned()
        .expect("the new set id");
    let weight = format!("[data-set-id=\"{set_id}\"] [data-set-weight]");
    let reps = format!("[data-set-id=\"{set_id}\"] [data-set-reps]");
    browser::set_input(driver, &weight, "20").await;
    browser::set_input(driver, &reps, "10").await;
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"] [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
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

async fn check_outbox_has(driver: &WebDriver, kind: &str) {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const request = indexedDB.open('warrior'); \
         request.onerror = () => done(JSON.stringify([])); \
         request.onsuccess = () => { \
           const db = request.result; \
           const transaction = db.transaction(['outbox'], 'readonly'); \
           const query = transaction.objectStore('outbox').getAll(); \
           query.onsuccess = () => done(JSON.stringify(\
             query.result.map((entry) => entry.operation.kind))); \
           query.onerror = () => done(JSON.stringify([])); \
         };",
        Vec::new(),
    )
    .await;
    let kinds = value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        kinds.iter().any(|value| value == kind),
        "the outbox must contain {kind}: {kinds:?}"
    );
}

async fn wait_for_alternative_cache(driver: &WebDriver) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let value = browser::eval_json(
            driver,
            "const done = arguments[arguments.length - 1]; \
             const request = indexedDB.open('warrior'); \
             request.onerror = () => done(JSON.stringify({ library: 0, recorded: 0 })); \
             request.onsuccess = () => { \
               const db = request.result; \
               const transaction = db.transaction(\
                 ['library_exercises', 'recorded_exercises'], 'readonly'); \
               const library = transaction.objectStore('library_exercises').count(); \
               const recorded = transaction.objectStore('recorded_exercises').count(); \
               const result = { library: 0, recorded: 0 }; \
               library.onsuccess = () => { result.library = library.result; }; \
               recorded.onsuccess = () => { result.recorded = recorded.result; }; \
               transaction.oncomplete = () => done(JSON.stringify(result)); \
               transaction.onerror = () => done(JSON.stringify({ library: 0, recorded: 0 })); \
             };",
            Vec::new(),
        )
        .await;
        let library = value.get("library").and_then(Value::as_u64).unwrap_or(0);
        let recorded = value.get("recorded").and_then(Value::as_u64).unwrap_or(0);
        if library > 0 && recorded > 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the alternative cache was not stored: {value:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn seed_past_session(server: &DevServer, user_id: &str) {
    let snapshot = json!({
        "exercise_id": DUMBBELL_PRESS,
        "name": "ダンベルプレス",
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
             (id, user_id, date, started_at, memo, source_kind, finished_at, duration_seconds, \
              total_weight_kg, exercise_count, set_count, created_at, updated_at) \
             VALUES ('{PAST_TRAINING}', '{user_id}', '2026-01-01', '2026-01-01T03:00:00Z', '', \
              'empty', '2026-01-01T04:00:00Z', 3600, 200.0, 1, 1, \
              '2026-01-01T03:00:00Z', '2026-01-01T04:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{PAST_ROW}', '{user_id}', '{PAST_TRAINING}', '{DUMBBELL_PRESS}', \
              '{}', 1, '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')",
            json_literal(&snapshot)
        ),
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{PAST_SET}', '{user_id}', '{PAST_ROW}', 1, 'normal', 1, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              duration_seconds, created_at, updated_at) \
             VALUES ('{PAST_SEGMENT}', '{user_id}', '{PAST_SET}', 1, 20.0, 10, 0, 0, NULL, \
              '2026-01-01T03:00:00Z', '2026-01-01T03:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the past session must be seeded");
    }
}

fn json_literal(value: &Value) -> String {
    value.to_string().replace('\'', "''")
}
