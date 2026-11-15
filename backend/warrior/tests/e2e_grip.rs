//! グリップの選択、保存、前回の記録、自己ベストの E2E。
//!
//! 対応する要求: `REQ-SET-011`、`REQ-LIB-011`。
//! 記録画面で握り方、手幅、アタッチメントを選ぶと、その選択がセットに保存される。
//! 前回の記録と自己ベストは、同じグリップと同じアタッチメントの記録だけを対象にする。
//! 握り方と手幅の両方、アタッチメントと順手か逆手かの両方が一致することを確かめる。

mod support;

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;
use warrior_core::exercise::GripSelection;
use warrior_core::set_calc::estimated_one_rep_max;

const GRIP_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000013";
const GRIP_NAME: &str = "T バーロウ";
const ATTACHMENT_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-00000000001a";
const ATTACHMENT_NAME: &str = "ラットプルダウン";
const GRIPLESS_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8858;
const PROVIDER_PORT: u16 = 8859;

const GRIP_PRONATED_WIDE: &str = r#"{"grip":{"style":"pronated","width":"wide"}}"#;
const GRIP_SUPINATED_NARROW: &str = r#"{"grip":{"style":"supinated","width":"narrow"}}"#;
const GRIP_SUPINATED_WIDE: &str = r#"{"grip":{"style":"supinated","width":"wide"}}"#;
const GRIP_PRONATED_NARROW: &str = r#"{"grip":{"style":"pronated","width":"narrow"}}"#;
const ATTACHMENT_WIDE_PRONATED: &str =
    r#"{"attachment":{"attachment":"wide_bar","style":"pronated"}}"#;
const ATTACHMENT_STRAIGHT_PRONATED: &str =
    r#"{"attachment":{"attachment":"straight_bar","style":"pronated"}}"#;
const ATTACHMENT_STRAIGHT_SUPINATED: &str =
    r#"{"attachment":{"attachment":"straight_bar","style":"supinated"}}"#;

#[tokio::test]
async fn records_the_grip_and_limits_the_previous_and_the_personal_bests() {
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
    seed_past(&server, &user_id);
    start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, GRIP_EXERCISE_ID).await;
    browser::add_exercise(&driver, ATTACHMENT_EXERCISE_ID).await;
    browser::add_exercise(&driver, GRIPLESS_EXERCISE_ID).await;

    check_the_grip_exercise(&driver, &server).await;
    check_the_attachment_exercise(&driver, &server).await;
    check_the_gripless_exercise(&driver, &server).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn check_the_grip_exercise(driver: &WebDriver, server: &DevServer) {
    let detail = detail_selector(GRIP_EXERCISE_ID);
    browser::select_exercise(driver, GRIP_EXERCISE_ID).await;
    wait_for_previous_total(driver, &detail, "500").await;

    let set = add_set(driver, &detail).await;
    let row = set_row(&detail, &set);
    wait_for_grip_label(driver, &row, "順手 ワイド").await;

    open_drawer(driver, &row).await;
    select_value(
        driver,
        "[data-set-drawer] [data-set-grip-style]",
        "supinated",
    )
    .await;
    select_value(driver, "[data-set-drawer] [data-set-grip-width]", "narrow").await;
    wait_for_grip_label(driver, &row, "逆手 ナロー").await;
    wait_for_previous_total(driver, &detail, "400").await;
    close_drawer(driver).await;

    browser::set_input(driver, &format!("{row} [data-set-weight]"), "85").await;
    browser::set_input(driver, &format!("{row} [data-set-reps]"), "5").await;
    browser::wait_for_attribute(
        driver,
        &format!("{row} [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
    browser::click(driver, &format!("{row} [data-set-complete]")).await;

    wait_for_set_grip(server, &set, GRIP_SUPINATED_NARROW).await;
    wait_for_personal_best(driver, GRIP_NAME, "max_weight", 85.0, "逆手 ナロー").await;
    wait_for_personal_best(driver, GRIP_NAME, "total_weight", 425.0, "逆手 ナロー").await;
    let one_rep_max = estimated_one_rep_max(85.0, 5).expect("the formula");
    wait_for_personal_best(
        driver,
        GRIP_NAME,
        "estimated_one_rep_max",
        one_rep_max,
        "逆手 ナロー",
    )
    .await;
}

async fn check_the_attachment_exercise(driver: &WebDriver, server: &DevServer) {
    let detail = detail_selector(ATTACHMENT_EXERCISE_ID);
    browser::select_exercise(driver, ATTACHMENT_EXERCISE_ID).await;
    wait_for_previous_total(driver, &detail, "600").await;

    let set = add_set(driver, &detail).await;
    let row = set_row(&detail, &set);
    wait_for_grip_label(driver, &row, "順手 / ワイドバー").await;

    open_drawer(driver, &row).await;
    select_value(
        driver,
        "[data-set-drawer] [data-set-grip-attachment]",
        "straight_bar",
    )
    .await;
    wait_for_grip_label(driver, &row, "順手 / ストレートバー").await;
    wait_for_previous_total(driver, &detail, "500").await;
    select_value(
        driver,
        "[data-set-drawer] [data-set-grip-style]",
        "supinated",
    )
    .await;
    wait_for_grip_label(driver, &row, "逆手 / ストレートバー").await;
    wait_for_previous_total(driver, &detail, "700").await;
    select_value(
        driver,
        "[data-set-drawer] [data-set-grip-style]",
        "pronated",
    )
    .await;
    wait_for_grip_label(driver, &row, "順手 / ストレートバー").await;
    wait_for_previous_total(driver, &detail, "500").await;
    close_drawer(driver).await;

    browser::set_input(driver, &format!("{row} [data-set-weight]"), "55").await;
    browser::set_input(driver, &format!("{row} [data-set-reps]"), "10").await;
    browser::wait_for_attribute(
        driver,
        &format!("{row} [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
    browser::click(driver, &format!("{row} [data-set-complete]")).await;

    wait_for_set_grip(server, &set, ATTACHMENT_STRAIGHT_PRONATED).await;
    wait_for_personal_best(
        driver,
        ATTACHMENT_NAME,
        "max_weight",
        55.0,
        "順手 / ストレートバー",
    )
    .await;
    wait_for_personal_best(
        driver,
        ATTACHMENT_NAME,
        "total_weight",
        550.0,
        "順手 / ストレートバー",
    )
    .await;
    let one_rep_max = estimated_one_rep_max(55.0, 10).expect("the formula");
    wait_for_personal_best(
        driver,
        ATTACHMENT_NAME,
        "estimated_one_rep_max",
        one_rep_max,
        "順手 / ストレートバー",
    )
    .await;
}

async fn check_the_gripless_exercise(driver: &WebDriver, server: &DevServer) {
    let detail = detail_selector(GRIPLESS_EXERCISE_ID);
    browser::select_exercise(driver, GRIPLESS_EXERCISE_ID).await;
    let set = add_set(driver, &detail).await;
    let row = set_row(&detail, &set);
    open_drawer(driver, &row).await;
    assert!(
        driver
            .find(By::Css("[data-set-drawer] [data-set-grip-input]"))
            .await
            .is_err(),
        "a grip-less exercise must not show the grip inputs"
    );
    close_drawer(driver).await;

    browser::set_input(driver, &format!("{row} [data-set-weight]"), "60").await;
    browser::set_input(driver, &format!("{row} [data-set-reps]"), "8").await;
    browser::wait_for_attribute(
        driver,
        &format!("{row} [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
    browser::click(driver, &format!("{row} [data-set-complete]")).await;
    wait_for_set_grip_null(server, &set).await;
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=record-grip", provider.origin()))
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

async fn open_drawer(driver: &WebDriver, row: &str) {
    browser::click(driver, &format!("{row} [data-set-open]")).await;
    browser::wait_for_present(driver, "[data-set-drawer]").await;
}

async fn close_drawer(driver: &WebDriver) {
    browser::click(driver, "[data-set-drawer-close]").await;
    browser::wait_for_absent(driver, "[data-set-drawer]").await;
}

async fn select_value(driver: &WebDriver, selector: &str, value: &str) {
    let select = driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the select {selector} must exist: {error}"));
    let select = SelectElement::new(&select)
        .await
        .unwrap_or_else(|error| panic!("the select {selector} must be a select: {error}"));
    select
        .select_by_value(value)
        .await
        .unwrap_or_else(|error| panic!("the value {value} must be selectable: {error}"));
}

async fn wait_for_previous_total(driver: &WebDriver, detail: &str, expected: &str) {
    browser::wait_for_attribute(
        driver,
        &format!("{detail} [data-previous]"),
        "data-previous-total-weight-kg",
        expected,
    )
    .await;
}

async fn wait_for_grip_label(driver: &WebDriver, row: &str, expected: &str) {
    let selector = format!("{row} [data-set-grip-label]");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let text = browser::eval_string(
            driver,
            "const done = arguments[arguments.length - 1]; \
             done(document.querySelector(arguments[0])?.textContent ?? '');",
            vec![json!(selector)],
        )
        .await;
        if text == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the grip label of {row} did not become {expected}: {text}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_personal_best(
    driver: &WebDriver,
    name: &str,
    kind: &str,
    value: f64,
    equipment: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let items = personal_bests(driver).await;
        if items.iter().any(|item| {
            item.kind == kind
                && item.name == name
                && item.equipment == equipment
                && (item.value - value).abs() < 1e-9
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the personal best {kind} of {name} did not become {value} with {equipment}: {items:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[derive(Debug)]
struct PersonalBest {
    kind: String,
    value: f64,
    name: String,
    equipment: String,
}

async fn personal_bests(driver: &WebDriver) -> Vec<PersonalBest> {
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-record-personal-best]')) \
           .map((element) => [element.getAttribute('data-record-personal-best'), \
             Number(element.getAttribute('data-record-personal-best-value')), \
             element.querySelector('[data-record-personal-best-name]')?.textContent ?? '', \
             element.querySelector('[data-record-personal-best-equipment]')?.textContent ?? ''])));",
        Vec::new(),
    )
    .await;
    values
        .as_array()
        .expect("the personal bests")
        .iter()
        .map(|item| PersonalBest {
            kind: item[0].as_str().expect("a kind").to_owned(),
            value: item[1].as_f64().expect("a value"),
            name: item[2].as_str().expect("a name").to_owned(),
            equipment: item[3].as_str().expect("equipment").to_owned(),
        })
        .collect()
}

async fn wait_for_set_grip(server: &DevServer, set_id: &str, expected: &str) {
    let expected = serde_json::from_str::<Value>(expected).expect("the expected grip JSON");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let row = optional_row(
            server,
            &format!("SELECT grip FROM sets WHERE id = '{set_id}'"),
        );
        let actual = row
            .as_ref()
            .and_then(|row| row["grip"].as_str())
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        if actual.as_ref() == Some(&expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the grip of {set_id} was not saved: {actual:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_set_grip_null(server: &DevServer, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let row = optional_row(
            server,
            &format!("SELECT grip FROM sets WHERE id = '{set_id}'"),
        );
        if row.as_ref().is_some_and(|row| row["grip"].is_null()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the grip of {set_id} must stay null: {row:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn seed_profile(server: &DevServer, user_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, weight_kg, created_at, updated_at) \
             VALUES ('{user_id}', 70.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}

struct PastExercise<'a> {
    training_id: &'a str,
    exercise_id: &'a str,
    name: &'a str,
    grip_mode: &'a str,
    grip: &'a str,
    weight_kg: f64,
    reps: i64,
    date: &'a str,
}

fn seed_past(server: &DevServer, user_id: &str) {
    for past in [
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000101",
            exercise_id: GRIP_EXERCISE_ID,
            name: GRIP_NAME,
            grip_mode: "grip",
            grip: GRIP_PRONATED_WIDE,
            weight_kg: 100.0,
            reps: 5,
            date: "2026-01-01",
        },
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000102",
            exercise_id: GRIP_EXERCISE_ID,
            name: GRIP_NAME,
            grip_mode: "grip",
            grip: GRIP_SUPINATED_NARROW,
            weight_kg: 80.0,
            reps: 5,
            date: "2026-01-02",
        },
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000103",
            exercise_id: GRIP_EXERCISE_ID,
            name: GRIP_NAME,
            grip_mode: "grip",
            grip: GRIP_SUPINATED_WIDE,
            weight_kg: 90.0,
            reps: 5,
            date: "2026-01-03",
        },
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000104",
            exercise_id: GRIP_EXERCISE_ID,
            name: GRIP_NAME,
            grip_mode: "grip",
            grip: GRIP_PRONATED_NARROW,
            weight_kg: 95.0,
            reps: 5,
            date: "2026-01-04",
        },
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000105",
            exercise_id: ATTACHMENT_EXERCISE_ID,
            name: ATTACHMENT_NAME,
            grip_mode: "attachment",
            grip: ATTACHMENT_WIDE_PRONATED,
            weight_kg: 60.0,
            reps: 10,
            date: "2026-01-05",
        },
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000106",
            exercise_id: ATTACHMENT_EXERCISE_ID,
            name: ATTACHMENT_NAME,
            grip_mode: "attachment",
            grip: ATTACHMENT_STRAIGHT_PRONATED,
            weight_kg: 50.0,
            reps: 10,
            date: "2026-01-06",
        },
        PastExercise {
            training_id: "a7c4e2f0-0000-4000-8000-000000000107",
            exercise_id: ATTACHMENT_EXERCISE_ID,
            name: ATTACHMENT_NAME,
            grip_mode: "attachment",
            grip: ATTACHMENT_STRAIGHT_SUPINATED,
            weight_kg: 70.0,
            reps: 10,
            date: "2026-01-07",
        },
    ] {
        seed_past_exercise(server, user_id, &past);
    }
    seed_empty_key_personal_bests(
        server,
        user_id,
        GRIP_EXERCISE_ID,
        "a7c4e2f0-0000-4000-8000-000000000101",
        "2026-01-01",
    );
    seed_empty_key_personal_bests(
        server,
        user_id,
        ATTACHMENT_EXERCISE_ID,
        "a7c4e2f0-0000-4000-8000-000000000105",
        "2026-01-05",
    );
}

fn seed_empty_key_personal_bests(
    server: &DevServer,
    user_id: &str,
    exercise_id: &str,
    training_id: &str,
    date: &str,
) {
    for (kind, value) in [
        ("max_weight", 999.0),
        ("estimated_one_rep_max", 999.0),
        ("total_weight", 9_999.0),
    ] {
        server
            .execute_d1_json(&format!(
                "INSERT INTO personal_bests \
                 (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id, \
                  created_at, updated_at) \
                 VALUES ('{user_id}', '{exercise_id}', '', '', '{kind}', {value}, '{date}', \
                  '{training_id}', '{date}T03:00:00Z', '{date}T03:00:00Z')"
            ))
            .expect("the empty-key personal best must be seeded");
    }
}

fn seed_past_exercise(server: &DevServer, user_id: &str, past: &PastExercise<'_>) {
    let row_id = format!("{}-exercise", past.training_id);
    let set_id = format!("{}-set", past.training_id);
    let segment_id = format!("{}-segment", past.training_id);
    let selection =
        serde_json::from_str::<GripSelection>(past.grip).expect("the past grip selection");
    let snapshot = json!({
        "exercise_id": past.exercise_id,
        "name": past.name,
        "body_part": "back",
        "muscles": [{"muscle": "latissimus_dorsi", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": past.grip_mode,
        "grip_options": [selection]
    });
    let (grip, attachment) = selection.comparison_key();
    let grip = grip.unwrap_or_default();
    let attachment = attachment.unwrap_or_default();
    for sql in [
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, total_weight_kg, \
              created_at, updated_at) \
             VALUES ('{}', '{user_id}', '{}', '{}T03:00:00Z', '', 'empty', \
              '{}T04:00:00Z', {}, '{}T03:00:00Z', '{}T04:00:00Z')",
            past.training_id,
            past.date,
            past.date,
            past.date,
            past.weight_kg * past.reps as f64,
            past.date,
            past.date,
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{row_id}', '{user_id}', '{}', '{}', '{}', 1, '{}T03:00:00Z', \
              '{}T03:00:00Z')",
            past.training_id,
            past.exercise_id,
            sql_text(&snapshot.to_string()),
            past.date,
            past.date,
        ),
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, grip, created_at, \
              updated_at) \
             VALUES ('{set_id}', '{user_id}', '{row_id}', 1, 'normal', 1, '{}', \
              '{}T03:00:00Z', '{}T03:00:00Z')",
            sql_text(past.grip),
            past.date,
            past.date,
        ),
        format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              duration_seconds, created_at, updated_at) \
             VALUES ('{segment_id}', '{user_id}', '{set_id}', 1, {}, {}, 0, 0, NULL, \
              '{}T03:00:00Z', '{}T03:00:00Z')",
            past.weight_kg, past.reps, past.date, past.date,
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the past training must be seeded");
    }
    let one_rep_max = estimated_one_rep_max(past.weight_kg, u32::try_from(past.reps).unwrap_or(1))
        .expect("the past one rep max");
    for (kind, value) in [
        ("max_weight", past.weight_kg),
        ("total_weight", past.weight_kg * past.reps as f64),
        ("estimated_one_rep_max", one_rep_max),
    ] {
        server
            .execute_d1_json(&format!(
                "INSERT INTO personal_bests \
                 (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id, \
                  created_at, updated_at) \
                 VALUES ('{user_id}', '{}', '{}', '{}', '{kind}', {value}, '{}', '{}', \
                  '{}T03:00:00Z', '{}T03:00:00Z')",
                past.exercise_id,
                sql_text(&grip),
                sql_text(&attachment),
                past.date,
                past.training_id,
                past.date,
                past.date,
            ))
            .expect("the past personal best must be seeded");
    }
}

fn sql_text(value: &str) -> String {
    value.replace('\'', "''")
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

fn detail_selector(exercise_id: &str) -> String {
    format!("[data-exercise-detail][data-exercise-id=\"{exercise_id}\"]")
}

fn set_row(detail: &str, set_id: &str) -> String {
    format!("{detail} [data-set-id=\"{set_id}\"]")
}
