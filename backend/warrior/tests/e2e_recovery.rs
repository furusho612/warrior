//! 部位区分ごとの回復状況の E2E。
//!
//! 対応する要求: `REQ-ANL-008`、`REQ-ANL-009`、`REQ-ANL-010`。
//! `/analysis` で部位区分ごとの回復の 3 段階と、目安であることの注記が表示されることを
//! 実ブラウザで確かめる。記録は D1 に置き、終了の時刻からの経過時間、日付だけの記録、
//! 最後のトレーニングが無い部位、部位区分「その他」を含む。

mod support;

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;
use warrior_core::jst::{JstDate, UtcInstant};

const BENCH_PRESS_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const ONE_HAND_ROW_ID: &str = "a7c4e2f0-0000-4000-8000-000000000018";
const SQUAT_ID: &str = "a7c4e2f0-0000-4000-8000-000000000030";
const SIDE_PLANK_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005a";
const BURPEE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000066";
const SERVER_PORT: u16 = 8874;
const PROVIDER_PORT: u16 = 8875;
const HOUR: i64 = 60 * 60;

const LATEST_CHEST: &str = "a7c4e2f0-0000-4000-8000-000000000401";
const OLDER_CHEST: &str = "a7c4e2f0-0000-4000-8000-000000000402";
const BACK: &str = "a7c4e2f0-0000-4000-8000-000000000403";
const LEGS: &str = "a7c4e2f0-0000-4000-8000-000000000404";
const DATE_ONLY_ABS: &str = "a7c4e2f0-0000-4000-8000-000000000405";
const OTHER: &str = "a7c4e2f0-0000-4000-8000-000000000406";

#[tokio::test]
async fn shows_the_recovery_status_and_the_note() {
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
    let fixture = Fixture::now();
    seed_recovery(&server, &user_id, &fixture);

    driver
        .goto(format!("{}/analysis", server.origin()))
        .await
        .expect("the analysis must open");
    browser::wait_for_route(&driver, "/analysis").await;
    browser::wait_for_attribute(&driver, "[data-recovery]", "data-recovery-part-count", "7").await;
    check_the_states(&driver).await;
    check_the_note(&driver).await;
    check_the_analysis_still_loads(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=recovery", provider.origin()))
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

async fn check_the_states(driver: &WebDriver) {
    assert_eq!(recovery_state(driver, "chest").await, "recovering");
    assert_eq!(recovery_state(driver, "back").await, "almost_recovered");
    assert_eq!(recovery_state(driver, "legs").await, "recovered");
    assert_eq!(
        recovery_state(driver, "abs").await,
        "recovering",
        "a date-only record uses the next midnight in Japan"
    );
    for body_part in ["shoulders", "biceps_forearm", "triceps"] {
        assert_eq!(
            recovery_state(driver, body_part).await,
            "none",
            "a part without a record has no state"
        );
    }
    browser::wait_for_absent(driver, "[data-recovery-part=\"other\"]").await;
    assert_eq!(recovery_text(driver, "legs").await, "回復済み");
    assert_eq!(recovery_text(driver, "shoulders").await, "記録がありません");
}

async fn check_the_note(driver: &WebDriver) {
    browser::wait_for_text(driver, "回復状況は目安です。体調に応じて判断してください。").await;
}

async fn check_the_analysis_still_loads(driver: &WebDriver) {
    browser::wait_for_attribute(
        driver,
        "[data-analysis-parts]",
        "data-analysis-part-count",
        "8",
    )
    .await;
}

async fn recovery_state(driver: &WebDriver, body_part: &str) -> String {
    browser::attribute(
        driver,
        &format!("[data-recovery-part=\"{body_part}\"]"),
        "data-recovery-state",
    )
    .await
}

async fn recovery_text(driver: &WebDriver, body_part: &str) -> String {
    let selector = format!("[data-recovery-part=\"{body_part}\"] span:last-child");
    driver
        .find(By::Css(selector.as_str()))
        .await
        .expect("the recovery row must exist")
        .text()
        .await
        .expect("the recovery row text must be readable")
}

struct Fixture {
    now: i64,
    today: JstDate,
}

impl Fixture {
    fn now() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_secs() as i64;
        let today = UtcInstant::from_epoch_seconds(seconds)
            .jst_date()
            .expect("a representable date");
        Self {
            now: seconds,
            today,
        }
    }
}

struct TrainingSeed<'a> {
    training_id: &'a str,
    date: JstDate,
    started_at: Option<i64>,
    finished_at: i64,
    exercise_id: &'a str,
    name: &'a str,
    body_part: &'a str,
}

fn seed_recovery(server: &DevServer, user_id: &str, fixture: &Fixture) {
    let seeds = vec![
        TrainingSeed {
            training_id: LATEST_CHEST,
            date: jst_date(fixture.now - 10 * HOUR),
            started_at: Some(fixture.now - 11 * HOUR),
            finished_at: fixture.now - 10 * HOUR,
            exercise_id: BENCH_PRESS_ID,
            name: "ベンチプレス",
            body_part: "chest",
        },
        TrainingSeed {
            training_id: OLDER_CHEST,
            date: jst_date(fixture.now - 100 * HOUR),
            started_at: Some(fixture.now - 101 * HOUR),
            finished_at: fixture.now - 100 * HOUR,
            exercise_id: BENCH_PRESS_ID,
            name: "ベンチプレス",
            body_part: "chest",
        },
        TrainingSeed {
            training_id: BACK,
            date: jst_date(fixture.now - 50 * HOUR),
            started_at: Some(fixture.now - 51 * HOUR),
            finished_at: fixture.now - 50 * HOUR,
            exercise_id: ONE_HAND_ROW_ID,
            name: "ワンハンドダンベルロウ",
            body_part: "back",
        },
        TrainingSeed {
            training_id: LEGS,
            date: jst_date(fixture.now - 80 * HOUR),
            started_at: Some(fixture.now - 81 * HOUR),
            finished_at: fixture.now - 80 * HOUR,
            exercise_id: SQUAT_ID,
            name: "スクワット",
            body_part: "legs",
        },
        TrainingSeed {
            training_id: DATE_ONLY_ABS,
            date: fixture
                .today
                .add_days(-1)
                .expect("yesterday is representable"),
            started_at: None,
            finished_at: fixture.now,
            exercise_id: SIDE_PLANK_ID,
            name: "サイドプランク",
            body_part: "abs",
        },
        TrainingSeed {
            training_id: OTHER,
            date: jst_date(fixture.now - 9 * HOUR),
            started_at: Some(fixture.now - 10 * HOUR),
            finished_at: fixture.now - 9 * HOUR,
            exercise_id: BURPEE_ID,
            name: "バーピー",
            body_part: "other",
        },
    ];
    for seed in seeds {
        seed_training(server, user_id, &seed);
    }
}

fn seed_training(server: &DevServer, user_id: &str, seed: &TrainingSeed<'_>) {
    let date = seed.date.to_string();
    let finished_at = iso(seed.finished_at);
    let started_at = seed.started_at.map_or_else(
        || "NULL".to_owned(),
        |seconds| format!("'{}'", iso(seconds)),
    );
    let training_exercise = format!("{}-exercise", seed.training_id);
    let snapshot = json!({
        "exercise_id": seed.exercise_id,
        "name": seed.name,
        "body_part": seed.body_part,
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, total_weight_kg, \
              created_at, updated_at) \
             VALUES ('{}', '{user_id}', '{date}', {started_at}, '', 'empty', \
              '{finished_at}', 0, '{finished_at}', '{finished_at}')",
            seed.training_id
        ))
        .expect("the training must be seeded");
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{training_exercise}', '{user_id}', '{}', '{}', '{}', 1, \
              '{finished_at}', '{finished_at}')",
            seed.training_id,
            seed.exercise_id,
            sql_text(&snapshot.to_string())
        ))
        .expect("the training exercise must be seeded");
    let set_id = format!("{}-set-1", seed.training_id);
    server
        .execute_d1_json(&format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, grip, body_weight_kg, \
              created_at, updated_at) \
             VALUES ('{set_id}', '{user_id}', '{training_exercise}', 1, 'normal', 1, NULL, \
              NULL, '{finished_at}', '{finished_at}')"
        ))
        .expect("the set must be seeded");
    server
        .execute_d1_json(&format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              duration_seconds, left_weight_kg, left_reps, left_duration_seconds, \
              right_weight_kg, right_reps, right_duration_seconds, created_at, updated_at) \
             VALUES ('{set_id}-segment-1', '{user_id}', '{set_id}', 1, 100.0, 5, 0, 0, \
              NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{finished_at}', '{finished_at}')"
        ))
        .expect("the set segment must be seeded");
}

fn jst_date(seconds: i64) -> JstDate {
    UtcInstant::from_epoch_seconds(seconds)
        .jst_date()
        .expect("a representable date")
}

fn iso(seconds: i64) -> String {
    UtcInstant::from_epoch_seconds(seconds).to_iso8601()
}

fn sql_text(text: &str) -> String {
    text.replace('\'', "''")
}
