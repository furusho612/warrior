//! トレーニング全体と種目ごとのメモの入力、保存、表示、AI の入力への反映の E2E。
//!
//! 対応する要求: `REQ-REC-007`。
//! 記録画面でメモを入力し、端末内の保存と送信待ちの列を通して D1 へ保存する。
//! 読み直した画面と履歴でメモをテキストとして表示し、AI に渡す user メッセージへ
//! トレーニングと種目のメモが入ることを確かめる。

mod support;

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, seed_profile, DevServer, TestOidc};
use thirtyfour::prelude::*;
use warrior_core::ai::context::{
    AiContext, AiProfile, ContextExercise, ContextTraining, TrainingExperience,
};
use warrior_core::ai::{build_prompt, sanitize_memo};
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{Goal, Sex};
use warrior_core::set_calc::RecordMode;

const SERVER_PORT: u16 = 8870;
const PROVIDER_PORT: u16 = 8871;
const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SESSION_MEMO: &str = "腰に痛みがある\n今日は軽くする <b>タグ</b>";
const EXERCISE_MEMO: &str = "次回は軽くする\nフォームを確認する <i>注意</i>";

#[tokio::test]
async fn saves_and_shows_the_training_and_exercise_memos() {
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
    let cookie = cookie_header(&driver).await;
    seed_profile(&server, &current_user_id(&driver).await);
    driver
        .goto(format!("{origin}/record"))
        .await
        .expect("the record page must open");
    browser::wait_for_present(&driver, "[data-start-record]").await;
    let training_id = start_training(&driver).await;

    browser::set_text(&driver, "textarea[data-memo]", SESSION_MEMO).await;
    wait_for_session_memo(&server, &cookie, &training_id, SESSION_MEMO).await;

    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    let exercise_memo = format!(
        "[data-exercise-detail][data-exercise-id=\"{BENCH_EXERCISE_ID}\"] textarea[data-exercise-memo]"
    );
    browser::set_text(&driver, &exercise_memo, EXERCISE_MEMO).await;
    wait_for_exercise_memo(&server, &cookie, &training_id, EXERCISE_MEMO).await;

    // 読み直しても、端末内の保存から同じメモを読み出せる。
    driver
        .goto(format!("{origin}/record/{training_id}"))
        .await
        .expect("the record must open again");
    browser::wait_for_present(&driver, "textarea[data-memo]").await;
    browser::wait_for_value(&driver, "textarea[data-memo]", SESSION_MEMO).await;
    browser::wait_for_present(&driver, &exercise_memo).await;
    browser::wait_for_value(&driver, &exercise_memo, EXERCISE_MEMO).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    // 履歴に出すため、トレーニングを終了する。
    browser::click(&driver, "[data-finish]").await;
    browser::wait_for_route(&driver, "/summary/:training_id").await;

    // 履歴では、メモが HTML として解釈されず、タグを含む文字のまま表示される。
    driver
        .goto(format!("{origin}/history"))
        .await
        .expect("the history must open");
    let item = format!("[data-history-item=\"{training_id}\"]");
    browser::wait_for_present(&driver, &item).await;
    let memo_text = driver
        .find(By::Css(format!("{item} p")))
        .await
        .expect("the session memo must be displayed")
        .text()
        .await
        .expect("the session memo must be readable");
    assert!(
        memo_text.contains("<b>タグ</b>"),
        "the memo must be displayed as text: {memo_text}"
    );
    assert!(
        driver.find(By::Css(format!("{item} b"))).await.is_err(),
        "the memo must not create an HTML element"
    );

    // AI に渡す user メッセージに、トレーニングと種目のメモが入る。
    let record = training_record(&server, &cookie, &training_id);
    let session_memo = record["session"]["memo"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let exercise_memo_value = record["exercises"][0]["memo"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(session_memo, SESSION_MEMO);
    assert_eq!(exercise_memo_value, EXERCISE_MEMO);
    let prompt = ai_user_message(&session_memo, &exercise_memo_value);
    assert_eq!(
        prompt["records"][0]["memo"],
        json!(sanitize_memo(SESSION_MEMO))
    );
    assert_eq!(
        prompt["records"][0]["exercises"][0]["memo"],
        json!(sanitize_memo(EXERCISE_MEMO))
    );

    // 終了したトレーニングをもう一度開き、メモを編集できる。
    browser::click(&driver, &format!("{item} [data-history-open]")).await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    browser::wait_for_present(&driver, "[data-record-finished]").await;
    let edited_session_memo = format!("{SESSION_MEMO}\n編集した");
    let edited_exercise_memo = format!("{EXERCISE_MEMO}\n編集した");
    browser::set_text(&driver, "textarea[data-memo]", &edited_session_memo).await;
    wait_for_session_memo(&server, &cookie, &training_id, &edited_session_memo).await;
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::set_text(&driver, &exercise_memo, &edited_exercise_memo).await;
    wait_for_exercise_memo(&server, &cookie, &training_id, &edited_exercise_memo).await;
    driver
        .goto(format!("{origin}/record/{training_id}"))
        .await
        .expect("the finished record must open again");
    browser::wait_for_present(&driver, "textarea[data-memo]").await;
    browser::wait_for_value(&driver, "textarea[data-memo]", &edited_session_memo).await;
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::wait_for_value(&driver, &exercise_memo, &edited_exercise_memo).await;

    // メモの内容はログに出さない。
    let logs = server.log_lines().join("\n");
    assert!(!logs.contains("腰に痛みがある"), "the log leaked the memo");
    assert!(
        !logs.contains("次回は軽くする"),
        "the log leaked the exercise memo"
    );

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=memo", provider.origin()))
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
    browser::click(driver, "[data-start-record]").await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    let training_id = browser::attribute(driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty());
    browser::wait_for_present(driver, "textarea[data-memo]").await;
    training_id
}

async fn cookie_header(driver: &WebDriver) -> String {
    driver
        .get_all_cookies()
        .await
        .expect("the browser cookies must be readable")
        .iter()
        .map(|cookie| format!("{}={}", cookie.name, cookie.value))
        .collect::<Vec<_>>()
        .join("; ")
}

async fn wait_for_session_memo(
    server: &DevServer,
    cookie: &str,
    training_id: &str,
    expected: &str,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let record = training_record(server, cookie, training_id);
        let memo = record["session"]["memo"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if memo == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the session memo did not become {expected}: {memo}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_exercise_memo(
    server: &DevServer,
    cookie: &str,
    training_id: &str,
    expected: &str,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let record = training_record(server, cookie, training_id);
        let memo = record["exercises"]
            .as_array()
            .and_then(|exercises| exercises.first())
            .and_then(|exercise| exercise["memo"].as_str())
            .unwrap_or_default()
            .to_owned();
        if memo == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the exercise memo did not become {expected}: {memo}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn training_record(server: &DevServer, cookie: &str, training_id: &str) -> serde_json::Value {
    let response = server.request(
        "GET",
        &format!("/api/trainings/{training_id}"),
        &[("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the training record is JSON")
}

fn ai_user_message(session_memo: &str, exercise_memo: &str) -> serde_json::Value {
    let context = AiContext {
        goal: Goal::Hypertrophy,
        profile: AiProfile {
            height_cm: 175.0,
            weight_kg: 70.0,
            age: 36,
            sex: Sex::Male,
        },
        experience: TrainingExperience::Intermediate,
        training_days_per_week: 4,
        records: vec![ContextTraining {
            date: JstDate::new(2026, 11, 12).unwrap(),
            memo: Some(session_memo.to_owned()),
            exercises: vec![ContextExercise {
                exercise_id: BENCH_EXERCISE_ID.to_owned(),
                name: "ベンチプレス".to_owned(),
                grip: None,
                attachment: None,
                mode: RecordMode::WeightAndReps,
                memo: Some(exercise_memo.to_owned()),
                sets: Vec::new(),
            }],
        }],
        goals: Vec::new(),
        part_analysis: Vec::new(),
        assisted_set_reference: Vec::new(),
        recovery: Vec::new(),
        routine: Vec::new(),
        exercises: Vec::new(),
    };
    let prompt = build_prompt(&context);
    serde_json::from_str(&prompt.user).expect("the AI user message is JSON")
}
