//! ルーティンを作り、記録画面から開始し、種目とセットが並ぶことを確かめる E2E。
//!
//! 対応する要求: `REQ-RTN-001`、`REQ-RTN-003`、`REQ-RTN-004`。
//! ルーティンの画面で名前、種目、セット数、目標重量、目標回数、レスト時間を保存し、
//! 記録画面の入口から開始する。同じルーティンから 2 回開始しても、前回のトレーニングの
//! 編集が次のトレーニングに影響しないこと、削除しても開始済みのトレーニングが残ることを
//! 実ブラウザと D1 で確かめる。

mod support;

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8863;
const PROVIDER_PORT: u16 = 8864;

#[tokio::test]
async fn creates_a_routine_starts_it_twice_and_deletes_it_without_losing_trainings() {
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
    create_routine(&driver, &server).await;
    let routine_id = first_routine_id(&server);
    let first_training_id = start_from_record(&driver, &server, &routine_id).await;
    check_started_training(&server, &first_training_id, &routine_id);
    browser::wait_for_present(
        &driver,
        &format!("[data-exercise-row][data-exercise-id=\"{BENCH_EXERCISE_ID}\"]"),
    )
    .await;

    let second_training_id = start_from_routines(&driver, &server, &routine_id).await;
    check_started_training(&server, &second_training_id, &routine_id);
    assert_ne!(first_training_id, second_training_id);
    let first_row_id = first_training_exercise_id(&server, &first_training_id);
    let second_row_id = first_training_exercise_id(&server, &second_training_id);
    assert_ne!(first_row_id, second_row_id);
    server
        .execute_d1_json(&format!(
            "UPDATE set_segments SET weight_kg = 999 WHERE set_id IN \
             (SELECT id FROM sets WHERE training_exercise_id = '{first_row_id}')"
        ))
        .expect("the first training must be editable");
    assert_eq!(started_segment_weight(&server, &second_training_id), 100.0);

    delete_routine(&driver, &server, &routine_id).await;
    for training_id in [&first_training_id, &second_training_id] {
        let row = first_row(
            &server,
            &format!(
                "SELECT routine_id, source_kind FROM training_sessions WHERE id = '{training_id}'"
            ),
        );
        assert_eq!(row["routine_id"], Value::Null);
        assert_eq!(row["source_kind"], "routine");
    }

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=routine", provider.origin()))
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

async fn create_routine(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    browser::wait_for_present(driver, "[data-routines]").await;
    browser::click(driver, "[data-routine-new]").await;
    browser::wait_for_present(driver, "[data-routine-editor]").await;
    browser::set_input(driver, "[data-routine-name-input]", "胸の日").await;
    let select = driver
        .find(By::Css("[data-routine-exercise-choice]"))
        .await
        .expect("the exercise choice must exist");
    let select = SelectElement::new(&select)
        .await
        .expect("the exercise choice must be a select");
    select
        .select_by_value(BENCH_EXERCISE_ID)
        .await
        .expect("the bench press must be selectable");
    browser::click(driver, "[data-routine-add-exercise]").await;
    browser::wait_for_present(
        driver,
        &format!("[data-routine-exercise][data-routine-exercise-id=\"{BENCH_EXERCISE_ID}\"]"),
    )
    .await;
    browser::set_input(driver, "[data-routine-set-count]", "3").await;
    browser::set_input(driver, "[data-routine-target-weight]", "100").await;
    browser::set_input(driver, "[data-routine-target-reps]", "5").await;
    browser::set_input(driver, "[data-routine-rest]", "02:00").await;
    browser::click(driver, "[data-routine-save]").await;
    wait_for_routine_count(server, 1);
    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must reload");
    browser::wait_for_text(driver, "胸の日").await;
    browser::wait_for_present(driver, "[data-routine]").await;
}

async fn start_from_record(driver: &WebDriver, server: &DevServer, routine_id: &str) -> String {
    driver
        .goto(format!("{}/record", server.origin()))
        .await
        .expect("the record screen must open");
    browser::wait_for_present(driver, "[data-start-routine]").await;
    let selector = format!("[data-start-routine][data-routine-id=\"{routine_id}\"]");
    browser::click(driver, &selector).await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    browser::wait_for_present(driver, "[data-session-id]").await;
    browser::attribute(driver, "[data-session-id]", "data-session-id").await
}

async fn start_from_routines(driver: &WebDriver, server: &DevServer, routine_id: &str) -> String {
    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    let selector = format!("[data-routine-id=\"{routine_id}\"]");
    browser::wait_for_present(driver, &selector).await;
    browser::click(driver, &format!("{selector} [data-routine-start]")).await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    browser::wait_for_present(driver, "[data-session-id]").await;
    let training_id = browser::attribute(driver, "[data-session-id]", "data-session-id").await;
    browser::wait_for_present(
        driver,
        &format!("[data-exercise-row][data-exercise-id=\"{BENCH_EXERCISE_ID}\"]"),
    )
    .await;
    training_id
}

async fn delete_routine(driver: &WebDriver, server: &DevServer, routine_id: &str) {
    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    let selector = format!("[data-routine-id=\"{routine_id}\"]");
    browser::wait_for_present(driver, &selector).await;
    browser::click(driver, &format!("{selector} [data-routine-delete]")).await;
    browser::click(driver, &format!("{selector} [data-routine-delete-confirm]")).await;
    wait_for_routine_count(server, 0);
}

fn check_started_training(server: &DevServer, training_id: &str, routine_id: &str) {
    wait_for_training(server, training_id);
    let row = first_row(
        server,
        &format!(
            "SELECT routine_id, source_kind FROM training_sessions WHERE id = '{training_id}'"
        ),
    );
    assert_eq!(row["routine_id"], routine_id);
    assert_eq!(row["source_kind"], "routine");
    assert_eq!(
        count(
            server,
            &format!(
                "SELECT COUNT(*) AS count FROM training_exercises WHERE training_id = '{training_id}'"
            )
        ),
        1
    );
    assert_eq!(
        count(
            server,
            &format!(
                "SELECT COUNT(*) AS count FROM sets WHERE training_exercise_id IN \
                 (SELECT id FROM training_exercises WHERE training_id = '{training_id}')"
            )
        ),
        3
    );
    assert_eq!(started_segment_weight(server, training_id), 100.0);
}

fn started_segment_weight(server: &DevServer, training_id: &str) -> f64 {
    first_row(
        server,
        &format!(
            "SELECT sg.weight_kg FROM set_segments sg \
             JOIN sets s ON s.id = sg.set_id \
             JOIN training_exercises te ON te.id = s.training_exercise_id \
             WHERE te.training_id = '{training_id}' ORDER BY s.position LIMIT 1"
        ),
    )["weight_kg"]
        .as_f64()
        .expect("a target weight")
}

fn first_training_exercise_id(server: &DevServer, training_id: &str) -> String {
    first_row(
        server,
        &format!(
            "SELECT id FROM training_exercises WHERE training_id = '{training_id}' \
             ORDER BY position LIMIT 1"
        ),
    )["id"]
        .as_str()
        .expect("a training exercise id")
        .to_owned()
}

fn first_routine_id(server: &DevServer) -> String {
    first_row(server, "SELECT id FROM routines ORDER BY position LIMIT 1")["id"]
        .as_str()
        .expect("a routine id")
        .to_owned()
}

fn wait_for_routine_count(server: &DevServer, expected: i64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if count(server, "SELECT COUNT(*) AS count FROM routines") == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the routine count did not become {expected}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn wait_for_training(server: &DevServer, training_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if count(
            server,
            &format!("SELECT COUNT(*) AS count FROM training_sessions WHERE id = '{training_id}'"),
        ) == 1
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the training {training_id} did not appear"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn count(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}

fn first_row(server: &DevServer, sql: &str) -> Value {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .cloned()
        .unwrap_or_else(|| json!({}))
}
