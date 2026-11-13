//! アカウント削除の E2E。
//!
//! 対応する要求: `REQ-MYP-007`、`REQ-NFR-010`。
//! Google でログインした利用者の R2 と D1 の行を削除し、同じ Google アカウントで
//! ログインすると新しい利用者になることを確かめる。

mod support;

use serde_json::Value;
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

#[tokio::test]
async fn google_login_delete_removes_the_account_and_a_new_login_creates_a_new_user() {
    let server = DevServer::start(8787);
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
    login_with_google(&driver, &server).await;
    let old_user = me(&driver, &server).await;
    let old_id = old_user["id"].as_str().expect("the old user id").to_owned();
    server
        .put_r2_object(&format!("users/{old_id}/ai.json"), b"ai")
        .expect("the R2 object must be written");
    server
        .put_r2_object(&format!("exports/{old_id}/records.csv"), b"csv")
        .expect("the export must be written");
    server
        .execute_d1_json(&format!(
            "INSERT INTO passkeys (id, user_id, credential_id, public_key, sign_count, transports, name, created_at) \
             VALUES ('22222222-2222-4222-8222-222222222222', '{old_id}', 'credential', 'key', 0, '', 'パスキー', '2026-01-01T00:00:00Z')"
        ))
        .expect("the passkey must be seeded");

    let status = driver
        .execute_async(
            "const done = arguments[arguments.length - 1]; fetch('/api/account', { method: 'DELETE', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ confirm: 'delete-account' }) }).then((response) => done(response.status));",
            Vec::<Value>::new(),
        )
        .await
        .expect("the delete must complete")
        .json()
        .as_u64();
    assert_eq!(status, Some(204));
    driver
        .goto(format!("{}/api/me", server.origin()))
        .await
        .expect("the me page must open");
    let body = driver.source().await.expect("the page source");
    assert!(body.contains("\"code\":\"unauthorized\""), "{body}");
    assert_eq!(
        server
            .get_r2_object(&format!("users/{old_id}/ai.json"))
            .expect("the R2 read must work"),
        None
    );
    assert_eq!(
        server
            .get_r2_object(&format!("exports/{old_id}/records.csv"))
            .expect("the R2 read must work"),
        None
    );

    logout(&driver).await;
    login_with_google(&driver, &server).await;
    let new_user = me(&driver, &server).await;
    let new_id = new_user["id"].as_str().expect("the new user id");
    assert_ne!(old_id, new_id);

    driver.quit().await.expect("the browser must close");
}

async fn login_with_google(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/login", server.origin()))
        .await
        .expect("the login page must open");
    wait_for_route(driver, "/login").await;
    driver
        .find(By::XPath(
            "//button[contains(normalize-space(.), 'Google でログイン')]",
        ))
        .await
        .expect("the Google login button")
        .click()
        .await
        .expect("the Google login button must be clickable");
    wait_for_route(driver, "/").await;
}

async fn me(driver: &WebDriver, server: &DevServer) -> Value {
    driver
        .goto(format!("{}/api/me", server.origin()))
        .await
        .expect("the me page must open");
    let text = driver
        .execute(
            "return document.body ? document.body.innerText : ''",
            Vec::<Value>::new(),
        )
        .await
        .expect("the me text must be readable")
        .json()
        .as_str()
        .unwrap_or_default()
        .to_owned();
    serde_json::from_str(&text).expect("the me response must be JSON")
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

async fn wait_for_route(driver: &WebDriver, route: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let source = driver.source().await.unwrap_or_default();
        if source.contains(&format!("data-route=\"{route}\"")) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the route {route} did not appear"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
