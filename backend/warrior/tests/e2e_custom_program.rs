//! 独自のプログラムをルーティンから作り、開始し、進め、削除する E2E。
//!
//! 対応する要求: `REQ-PRG-021`、`REQ-PRG-022`、`REQ-PRG-023`、`REQ-PRG-024`。

mod support;

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8878;
const PROVIDER_PORT: u16 = 8879;
const ROUTINE_A: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7a10";
const ROUTINE_B: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7a11";
const ROUTINE_EXERCISE_A: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7a20";
const ROUTINE_EXERCISE_B: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c7a21";
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";

#[tokio::test]
async fn a_custom_program_is_created_started_advanced_and_deleted() {
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
    seed_routine(&server, &user_id, ROUTINE_A, ROUTINE_EXERCISE_A, 1);
    seed_routine(&server, &user_id, ROUTINE_B, ROUTINE_EXERCISE_B, 2);

    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must open");
    browser::wait_for_present(&driver, "[data-custom-programs]").await;
    browser::set_input(&driver, "[data-custom-program-name]", "自分の二日").await;
    browser::click(
        &driver,
        &format!("[data-custom-program-routine=\"{ROUTINE_A}\"]"),
    )
    .await;
    browser::click(
        &driver,
        &format!("[data-custom-program-routine=\"{ROUTINE_B}\"]"),
    )
    .await;
    browser::click(&driver, "[data-custom-program-submit]").await;
    browser::wait_for_present(&driver, "[data-custom-program-list]").await;
    let id = first_row(
        &server,
        &format!(
            "SELECT id FROM custom_programs WHERE user_id = '{user_id}' AND name = '自分の二日'"
        ),
    )["id"]
        .as_str()
        .expect("the custom program id")
        .to_owned();

    browser::click(&driver, &format!("[data-custom-program-select=\"{id}\"]")).await;
    driver
        .goto(server.origin())
        .await
        .expect("the home must open");
    browser::wait_for_present(&driver, "[data-home-program]").await;
    browser::wait_for_text(&driver, "自分の二日").await;
    browser::wait_for_text(&driver, "1 / 2").await;
    browser::click(&driver, "[data-home-program-start]").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_present(&driver, "[data-session-id]").await;
    let training_id = browser::attribute(&driver, "[data-session-id]", "data-session-id").await;
    browser::click(&driver, "[data-finish]").await;
    browser::wait_for_route(&driver, "/summary/:training_id").await;
    wait_for_finished(&server, &training_id);

    driver
        .goto(server.origin())
        .await
        .expect("the home must reopen");
    browser::wait_for_present(&driver, "[data-home-program]").await;
    browser::wait_for_text(&driver, "2 / 2").await;

    driver
        .goto(format!("{}/routines", server.origin()))
        .await
        .expect("the routines screen must reopen");
    browser::wait_for_present(&driver, &format!("[data-custom-program-delete=\"{id}\"]")).await;
    browser::click(&driver, &format!("[data-custom-program-delete=\"{id}\"]")).await;
    browser::wait_for_absent(&driver, &format!("[data-custom-program-delete=\"{id}\"]")).await;
    driver
        .goto(server.origin())
        .await
        .expect("the home must reopen after deleting");
    browser::wait_for_absent(&driver, "[data-home-program]").await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!(
            "{}/test/user?sub=custom-program",
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

fn seed_routine(
    server: &DevServer,
    user_id: &str,
    routine_id: &str,
    exercise_row_id: &str,
    position: i64,
) {
    let snapshot = json!({
        "exercise_id": BENCH,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    for sql in [
        format!(
            "INSERT INTO routines (id, user_id, name, position, created_at, updated_at) \
             VALUES ('{routine_id}', '{user_id}', '胸の日', {position}, \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercises (id, user_id, routine_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{exercise_row_id}', '{user_id}', \
             '{routine_id}', '{BENCH}', '{snapshot}', 1, '2026-01-01T00:00:00Z', \
             '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercise_targets (routine_exercise_id, user_id, set_count, \
             target_reps, rest_seconds, created_at, updated_at) VALUES ('{exercise_row_id}', \
             '{user_id}', 3, 8, 90, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the routine must be seeded");
    }
}

fn wait_for_finished(server: &DevServer, training_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let row = first_row(
            server,
            &format!("SELECT finished_at FROM training_sessions WHERE id = '{training_id}'"),
        );
        if row["finished_at"] != Value::Null {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the training was not finished: {row}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
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
