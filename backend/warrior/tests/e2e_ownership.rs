//! 2 つのブラウザのプロファイルによる編集権と引き継ぎの E2E。
//!
//! 対応する要求: `REQ-NFR-015`、`REQ-NFR-006`。
//! Chrome の 2 つの WebDriver セッションは別のプロファイルを使うため、Cookie と IndexedDB を
//! 共有しない。開始した端末だけが編集でき、ほかの端末は閲覧のみになり、引き継いだ端末が
//! 編集できること、元の端末が閲覧のみになることを確かめる。
//! SafariDriver は 1 つのブラウザを共有し、プロファイルを分けられないため対象外とする。
//! ほかの E2E と同時に走れるよう、Worker は 8805、テスト専用の OIDC の提供者は 8806 を使う。

mod support;

use serde_json::Value;
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;
use warrior_core::auth::hash_terminal_token;

const SUB: &str = "ownership";
const OTHER_SUB: &str = "ownership-other";
const SERVER_PORT: u16 = 8805;
const PROVIDER_PORT: u16 = 8806;

#[tokio::test]
async fn the_starting_terminal_edits_and_the_other_terminal_takes_over() {
    let origin = format!("http://localhost:{SERVER_PORT}");
    let provider_origin = format!("http://localhost:{PROVIDER_PORT}");
    let authorize_url = format!("{provider_origin}/authorize");
    let token_url = format!("{provider_origin}/token");
    let userinfo_url = format!("{provider_origin}/userinfo");
    let redirect_uri = format!("{origin}/api/auth/google/callback");
    let server = DevServer::start_with_vars(
        SERVER_PORT,
        &[
            ("ORIGIN", origin.as_str()),
            ("GOOGLE_OAUTH_AUTHORIZE_URL", authorize_url.as_str()),
            ("GOOGLE_OAUTH_ISSUER", provider_origin.as_str()),
            ("GOOGLE_OAUTH_TOKEN_URL", token_url.as_str()),
            ("GOOGLE_OAUTH_USERINFO_URL", userinfo_url.as_str()),
            ("GOOGLE_OAUTH_REDIRECT_URI", redirect_uri.as_str()),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver_a = browser::open(Browser::Chrome, &driver_process).await;
    let driver_b = browser::open(Browser::Chrome, &driver_process).await;

    login(&driver_a, &server, &provider).await;
    login(&driver_b, &server, &provider).await;

    let training_id = start_recording(&driver_a, &server).await;
    type_memo(&driver_a, "A のメモ").await;
    wait_for_d1_memo(&server, &training_id, "A のメモ").await;

    open_record(&driver_b, &server, &training_id).await;
    browser::wait_for_value(&driver_b, "textarea[data-memo]", "A のメモ").await;
    type_memo(&driver_b, "B").await;
    browser::wait_for_attribute(&driver_b, "[data-view-only]", "data-view-only", "true").await;
    assert_eq!(
        local_session_count(&driver_b).await,
        1,
        "a rejected write must not delete the local record"
    );
    assert_eq!(
        d1_memo(&server, &training_id),
        "A のメモ",
        "a rejected write must not change the server"
    );

    takeover(&driver_b).await;
    browser::wait_for_route(&driver_b, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(&driver_b, "[data-session-id]", "data-session-id").await,
        training_id
    );
    browser::wait_for_value(&driver_b, "textarea[data-memo]", "A のメモ").await;
    type_memo(&driver_b, "B").await;
    wait_for_d1_memo(&server, &training_id, "A のメモB").await;

    type_memo(&driver_a, "A").await;
    browser::wait_for_attribute(&driver_a, "[data-view-only]", "data-view-only", "true").await;
    assert_eq!(
        d1_memo(&server, &training_id),
        "A のメモB",
        "the terminal that lost the editing right must not write"
    );

    check_terminal_tokens(&driver_a, &driver_b, &server).await;
    switch_account_and_drop_the_old_terminal(&driver_b, &server, &provider).await;

    browser::assert_no_panic(&driver_a).await;
    browser::assert_no_panic(&driver_b).await;
    driver_a.quit().await.expect("the first browser must close");
    driver_b
        .quit()
        .await
        .expect("the second browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub={SUB}", provider.origin()))
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

async fn start_recording(driver: &WebDriver, server: &DevServer) -> String {
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
    // 起動時の送信で作成をサーバーへ送る。
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(driver, "/record/:training_id").await;
    wait_for_session_row(server, &training_id).await;
    training_id
}

async fn open_record(driver: &WebDriver, server: &DevServer, training_id: &str) {
    driver
        .goto(format!("{}/record/{training_id}", server.origin()))
        .await
        .expect("the record must open");
    browser::wait_for_route(driver, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
}

async fn type_memo(driver: &WebDriver, text: &str) {
    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys(text)
        .await
        .expect("the memo must be typed");
}

async fn takeover(driver: &WebDriver) {
    driver
        .find(By::Css("[data-takeover]"))
        .await
        .expect("the takeover button must exist")
        .click()
        .await
        .expect("the takeover button must be clickable");
    browser::wait_for_text(driver, "元の端末にだけある未保存の変更は失われます").await;
    driver
        .find(By::Css("[data-takeover-confirm]"))
        .await
        .expect("the confirmation must exist")
        .click()
        .await
        .expect("the confirmation must be clickable");
}

async fn check_terminal_tokens(driver_a: &WebDriver, driver_b: &WebDriver, server: &DevServer) {
    let token_a = terminal_cookie(driver_a).await;
    let token_b = terminal_cookie(driver_b).await;
    assert_ne!(
        token_a, token_b,
        "two browser profiles must have different terminal tokens"
    );
    let user_id = first_value(
        &server
            .execute_d1_json(&format!("SELECT id FROM users WHERE google_sub = '{SUB}'"))
            .expect("the user must be readable"),
        "id",
    );
    let stored = server
        .execute_d1_json(&format!(
            "SELECT token_hash FROM terminal_tokens WHERE user_id = '{user_id}' ORDER BY id"
        ))
        .expect("the terminal rows must be readable");
    let hashes = stored
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.as_array())
        .cloned()
        .expect("the terminal results");
    assert_eq!(hashes.len(), 2);
    let hashes = hashes
        .iter()
        .map(|row| row["token_hash"].as_str().expect("a hash").to_owned())
        .collect::<Vec<_>>();
    assert!(hashes.contains(&hash_terminal_token(&token_a)));
    assert!(hashes.contains(&hash_terminal_token(&token_b)));
    assert!(!hashes.contains(&token_a), "D1 must not store the token");
    assert!(!hashes.contains(&token_b), "D1 must not store the token");
}

async fn switch_account_and_drop_the_old_terminal(
    driver: &WebDriver,
    server: &DevServer,
    provider: &TestOidc,
) {
    let old_token = terminal_cookie(driver).await;
    let old_user_id = first_value(
        &server
            .execute_d1_json(&format!("SELECT id FROM users WHERE google_sub = '{SUB}'"))
            .expect("the user must be readable"),
        "id",
    );
    browser::logout(driver).await;
    driver
        .goto(format!("{}/test/user?sub={OTHER_SUB}", provider.origin()))
        .await
        .expect("the other test user must be selected");
    driver
        .goto(format!("{}/login", server.origin()))
        .await
        .expect("the login page must open");
    browser::wait_for_route(driver, "/login").await;
    browser::click_text(driver, "Google でログイン").await;

    let new_token = wait_for_new_terminal_cookie(driver, &old_token).await;
    assert_ne!(
        old_token, new_token,
        "a login with another account must issue a new terminal token"
    );
    let old_hash = hash_terminal_token(&old_token);
    let remaining = server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM terminal_tokens WHERE token_hash = '{old_hash}'"
        ))
        .expect("the terminal rows must be readable");
    assert_eq!(
        first_count(&remaining),
        Some(0),
        "the terminal token of the previous account must be deleted"
    );
    let old_user_terminals = server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM terminal_tokens WHERE user_id = '{old_user_id}'"
        ))
        .expect("the terminal rows must be readable");
    assert_eq!(
        first_count(&old_user_terminals),
        Some(1),
        "the other terminal of the previous account must stay"
    );
    let other_user_id = first_value(
        &server
            .execute_d1_json(&format!(
                "SELECT id FROM users WHERE google_sub = '{OTHER_SUB}'"
            ))
            .expect("the other user must be readable"),
        "id",
    );
    let other_terminals = server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM terminal_tokens WHERE user_id = '{other_user_id}'"
        ))
        .expect("the terminal rows must be readable");
    assert_eq!(
        first_count(&other_terminals),
        Some(1),
        "the new account must have its own terminal token"
    );
}

async fn terminal_cookie(driver: &WebDriver) -> String {
    let cookies = driver
        .get_all_cookies()
        .await
        .expect("the browser cookies must be readable");
    let terminal = cookies
        .iter()
        .find(|cookie| cookie.name == "warrior_terminal")
        .expect("the terminal cookie must exist");
    assert_eq!(
        terminal.value.len(),
        43,
        "the terminal token must be 32 bytes in base64url"
    );
    terminal.value.clone()
}

/// 別のアカウントのログインで、端末トークンの Cookie が新しい値になるまで待つ。
async fn wait_for_new_terminal_cookie(driver: &WebDriver, old_token: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let cookies = driver.get_all_cookies().await.unwrap_or_default();
        if let Some(cookie) = cookies
            .iter()
            .find(|cookie| cookie.name == "warrior_terminal" && cookie.value != old_token)
        {
            return cookie.value.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the login of the other account did not issue a new terminal token"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn local_session_count(driver: &WebDriver) -> usize {
    let sessions = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const request = indexedDB.open('warrior'); \
         request.onerror = () => done(JSON.stringify([])); \
         request.onsuccess = () => { \
           const database = request.result; \
           const transaction = database.transaction('training_sessions', 'readonly'); \
           const all = transaction.objectStore('training_sessions').getAll(); \
           all.onsuccess = () => { database.close(); done(JSON.stringify(all.result)); }; \
           all.onerror = () => { database.close(); done(JSON.stringify([])); }; \
         };",
        Vec::new(),
    )
    .await;
    sessions.as_array().map(Vec::len).unwrap_or_default()
}

async fn wait_for_session_row(server: &DevServer, training_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if session_exists(server, training_id) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the session was not synced to D1: {training_id}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_d1_memo(server: &DevServer, training_id: &str, expected: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let memo = d1_memo(server, training_id);
        if memo == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the D1 memo did not become {expected}: {memo}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn d1_memo(server: &DevServer, training_id: &str) -> String {
    first_value(
        &server
            .execute_d1_json(&format!(
                "SELECT memo FROM training_sessions WHERE id = '{training_id}'"
            ))
            .expect("the session must be readable"),
        "memo",
    )
}

fn session_exists(server: &DevServer, training_id: &str) -> bool {
    let result = server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM training_sessions WHERE id = '{training_id}'"
        ))
        .expect("the session must be readable");
    first_count(&result) == Some(1)
}

fn first_count(value: &Value) -> Option<i64> {
    value.get(0)?.get("results")?.get(0)?.get("count")?.as_i64()
}

fn first_value(value: &Value, key: &str) -> String {
    value
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get(key))
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .unwrap_or_default()
}
