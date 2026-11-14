//! 記録画面の E2E が共有する、ログイン、記録の開始、セットの操作。

use serde_json::{json, Value};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;

use super::{browser, DevServer, TestOidc};

/// テスト用の OIDC の利用者を選び、Google のログインを通す。
pub(crate) async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc, sub: &str) {
    driver
        .goto(format!("{}/test/user?sub={sub}", provider.origin()))
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

/// トレーニングを開始し、記録画面のトレーニング ID を返す。
pub(crate) async fn start_training(driver: &WebDriver) -> String {
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

/// セットを追加し、追加したセットの ID を返す。
pub(crate) async fn add_set(driver: &WebDriver) -> String {
    browser::wait_for_present(driver, "[data-add-set]").await;
    browser::click(driver, "[data-add-set]").await;
    browser::wait_for_present(driver, "[data-set-row]").await;
    let rows = set_ids(driver).await;
    rows.last().cloned().expect("the new set id")
}

/// セット種別の選択肢を選ぶ。変更が確定するのを待たない。
pub(crate) async fn select_kind(driver: &WebDriver, set_id: &str, kind: &str) {
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
}

/// セット種別を選び、画面へ反映されるまで待つ。
pub(crate) async fn change_kind(driver: &WebDriver, set_id: &str, kind: &str) {
    select_kind(driver, set_id, kind).await;
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"]"),
        "data-set-kind",
        kind,
    )
    .await;
}

/// 要素の `disabled` が真か返す。
pub(crate) async fn element_disabled(driver: &WebDriver, selector: &str) -> bool {
    browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const element = document.querySelector(arguments[0]); \
         done(JSON.stringify(element ? element.disabled : null));",
        vec![json!(selector)],
    )
    .await
    .as_bool()
    .unwrap_or(false)
}

/// セット種別の選択肢が無効か返す。
pub(crate) async fn option_disabled(driver: &WebDriver, set_id: &str, kind: &str) -> bool {
    let selector = format!("[data-set-id=\"{set_id}\"] select[data-set-kind]");
    browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const select = document.querySelector(arguments[0]); \
         const option = select ? select.querySelector('option[value=\"' + arguments[1] + '\"]') : null; \
         done(JSON.stringify(option ? option.disabled : null));",
        vec![json!(selector), json!(kind)],
    )
    .await
    .as_bool()
    .unwrap_or(false)
}

/// セットの ID を画面の順に返す。
pub(crate) async fn set_ids(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-set-row]')) \
           .map((element) => element.getAttribute('data-set-id'))));",
        Vec::new(),
    )
    .await;
    values(value)
}

/// セットの段の ID を画面の順に返す。
pub(crate) async fn segment_ids(driver: &WebDriver, set_id: &str) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll(arguments[0])) \
           .map((element) => element.getAttribute('data-segment-id'))));",
        vec![json!(format!(
            "[data-set-id=\"{set_id}\"] [data-segment-id]"
        ))],
    )
    .await;
    let mut segments = values(value);
    segments.dedup();
    segments
}

fn values(value: Value) -> Vec<String> {
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

/// セットの完了ボタンが押せるようになるまで待つ。
pub(crate) async fn wait_for_complete_ready(driver: &WebDriver, set_id: &str) {
    browser::wait_for_attribute(
        driver,
        &format!("[data-set-id=\"{set_id}\"] [data-set-complete]"),
        "data-set-complete-ready",
        "true",
    )
    .await;
}

/// セットの重量の入力のセレクタ。
pub(crate) fn weight_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-weight]")
}

/// セットの回数の入力のセレクタ。
pub(crate) fn reps_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-reps]")
}

/// セットの完了ボタンのセレクタ。
pub(crate) fn complete_selector(set_id: &str) -> String {
    format!("[data-set-id=\"{set_id}\"] [data-set-complete]")
}

/// 段の重量の入力のセレクタ。
pub(crate) fn segment_weight(segment_id: &str) -> String {
    format!("[data-segment-id=\"{segment_id}\"] [data-set-weight]")
}

/// 段の回数の入力のセレクタ。
pub(crate) fn segment_reps(segment_id: &str) -> String {
    format!("[data-segment-id=\"{segment_id}\"] [data-set-reps]")
}

/// ログイン中の利用者の ID を返す。
pub(crate) async fn current_user_id(driver: &WebDriver) -> String {
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

/// テスト用の身体情報を作る。
pub(crate) fn seed_profile(server: &DevServer, user_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO profiles (user_id, weight_kg, created_at, updated_at) \
             VALUES ('{user_id}', 70.0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the profile must be seeded");
}
