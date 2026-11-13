//! パスキーの登録、ログイン、複数登録、削除、再認証の E2E。
//!
//! 対応する要求: `REQ-NFR-013`、`REQ-NFR-014`、`REQ-MYP-006`。
//! Chrome の仮想認証器を使い、Google のログインからパスキーの追加と削除まで通す。

mod support;

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

#[tokio::test]
async fn chrome_virtual_authenticator_registers_logs_in_and_deletes_passkeys() {
    let server = DevServer::start_with_vars(
        8787,
        &[
            ("RATE_LIMIT_AUTH_IP", "50"),
            ("RATE_LIMIT_PASSKEY_IP", "100"),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    driver
        .goto(format!("{}/test/user?sub=alice", provider.origin()))
        .await
        .expect("the test user must be selected");
    driver
        .cdp()
        .send_raw("WebAuthn.enable", json!({}))
        .await
        .expect("WebAuthn must be enabled");
    driver
        .cdp()
        .send_raw(
            "WebAuthn.addVirtualAuthenticator",
            json!({
                "options": {
                    "protocol": "ctap2",
                    "transport": "internal",
                    "hasResidentKey": true,
                    "hasUserVerification": true,
                    "isUserVerified": true,
                    "automaticPresenceSimulation": true
                }
            }),
        )
        .await
        .expect("the virtual authenticator must be added");

    login_with_google(&driver, &server).await;
    open_route(&driver, &server, "/mypage", "/mypage").await;
    wait_for_text(&driver, "パスキーはまだ登録されていません").await;

    add_passkey(&driver, &server, 0).await;
    add_passkey(&driver, &server, 1).await;
    assert_eq!(passkey_count(&driver).await, 2);

    logout(&driver).await;
    open_route(&driver, &server, "/login", "/login").await;
    click_text(&driver, "端末のパスキーでログイン").await;
    wait_for_route(&driver, &server, "/").await;

    logout(&driver).await;
    open_route(&driver, &server, "/login", "/login").await;
    driver
        .find(By::Css("input[type=email]"))
        .await
        .expect("the email input")
        .send_keys("alice@example.test")
        .await
        .expect("the email must be typed");
    click_text(&driver, "パスキーでログイン").await;
    wait_for_route(&driver, &server, "/").await;

    open_route(&driver, &server, "/mypage", "/mypage").await;
    wait_for_passkey_count(&driver, 2).await;
    click_text(&driver, "パスキーを追加").await;
    wait_for_text(
        &driver,
        "操作の前に Google アカウントまたはパスキーで再認証してください",
    )
    .await;
    assert_eq!(passkey_count(&driver).await, 2);

    click_text(&driver, "Google で再認証").await;
    wait_for_route(&driver, &server, "/mypage").await;
    wait_for_passkey_count(&driver, 2).await;
    click_text(&driver, "削除").await;
    wait_for_text(&driver, "パスキーを削除しました").await;
    assert_eq!(passkey_count(&driver).await, 1);

    click_text(&driver, "Google で再認証").await;
    wait_for_route(&driver, &server, "/mypage").await;
    wait_for_passkey_count(&driver, 1).await;
    click_text(&driver, "削除").await;
    wait_for_text(&driver, "パスキーを削除しました").await;
    assert_eq!(passkey_count(&driver).await, 0);

    logout(&driver).await;
    login_with_google(&driver, &server).await;
    open_route(&driver, &server, "/mypage", "/mypage").await;
    wait_for_text(&driver, "パスキーはまだ登録されていません").await;
    add_passkey(&driver, &server, 0).await;

    driver.quit().await.expect("the browser must close");
}

async fn add_passkey(driver: &WebDriver, server: &DevServer, expected_before: usize) {
    open_route(driver, server, "/mypage", "/mypage").await;
    click_text(driver, "Google で再認証").await;
    wait_for_route(driver, server, "/mypage").await;
    let before = wait_for_passkey_count(driver, expected_before).await;
    click_text(driver, "パスキーを追加").await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if passkey_count(driver).await == before + 1 {
            return;
        }
        if std::time::Instant::now() >= deadline {
            let error = driver
                .execute(
                    "return window.__lastPasskeyError || ''",
                    Vec::<Value>::new(),
                )
                .await
                .map(|value| value.json().clone())
                .unwrap_or(Value::Null);
            panic!(
                "the passkey must appear; body: {:?}; logs: {:?}; js: {error}",
                driver.source().await,
                server.log_lines()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_passkey_count(driver: &WebDriver, expected: usize) -> usize {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let count = passkey_count(driver).await;
        if count == expected {
            return count;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the passkey list must have {expected} items but had {count}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn login_with_google(driver: &WebDriver, server: &DevServer) {
    open_route(driver, server, "/login", "/login").await;
    click_text(driver, "Google でログイン").await;
    wait_for_route(driver, server, "/").await;
}

async fn logout(driver: &WebDriver) {
    driver
        .execute_async(
            "const done = arguments[arguments.length - 1]; fetch('/api/auth/logout', { method: 'POST' }).then((r) => r.text()).then((t) => done(t));",
            Vec::<Value>::new(),
        )
        .await
        .expect("the logout must complete");
}

async fn open_route(driver: &WebDriver, server: &DevServer, path: &str, route: &str) {
    driver
        .goto(format!("{}{}", server.origin(), path))
        .await
        .expect("the page must open");
    wait_for_route(driver, server, route).await;
}

async fn wait_for_route(driver: &WebDriver, server: &DevServer, route: &str) {
    wait_until(driver, server, &format!("data-route=\"{route}\"")).await;
}

async fn wait_for_text(driver: &WebDriver, needle: &str) {
    wait_until_text(driver, needle).await;
}

async fn wait_until(driver: &WebDriver, server: &DevServer, needle: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let source = driver.source().await.unwrap_or_default();
        if source.contains(needle) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the page did not contain {needle}: {source}; js: {:?}; logs: {:?}",
            driver
                .execute(
                    "return window.__lastPasskeyError || ''",
                    Vec::<Value>::new()
                )
                .await
                .map(|value| value.json().clone())
                .unwrap_or(Value::Null),
            server.log_lines()
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_until_text(driver: &WebDriver, needle: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let source = driver.source().await.unwrap_or_default();
        if source.contains(needle) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the page did not contain {needle}: {source}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn click_text(driver: &WebDriver, label: &str) {
    let button = driver
        .find(By::XPath(format!(
            "//button[contains(normalize-space(.), '{}')]",
            label
        )))
        .await
        .unwrap_or_else(|error| panic!("the button {label} must exist: {error}"));
    button.click().await.expect("the button must be clickable");
}

async fn passkey_count(driver: &WebDriver) -> usize {
    driver
        .execute(
            "return document.querySelectorAll('ul li').length",
            Vec::<Value>::new(),
        )
        .await
        .expect("the passkey list must be readable")
        .json()
        .as_u64()
        .unwrap_or(0) as usize
}
