//! オフラインの記録、通信の復帰、再読み込み、タブを開き直した後の復元の E2E。
//!
//! 対応する要求: `REQ-NFR-003`、`REQ-NFR-004`、`REQ-NFR-005`、`REQ-NFR-007`。
//! Chrome の CDP で通信を切り、IndexedDB への保存、復帰後の同期、未保存の表示を確かめる。

mod support;

use serde_json::json;
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const MEMO: &str = "オフラインのメモ";

#[tokio::test]
async fn chrome_records_offline_restores_after_reload_and_syncs_after_reconnect() {
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    driver
        .goto(format!("{}/test/user?sub=offline", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(&driver, &server).await;

    driver
        .goto(format!("{}/record", server.origin()))
        .await
        .expect("the record page must open");
    browser::wait_for_route(&driver, "/record").await;
    browser::click_text(&driver, "記録を開始").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    let training_id = attribute(&driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty());

    driver
        .cdp()
        .send_raw("Network.enable", json!({}))
        .await
        .expect("the network must be enabled");
    offline(&driver, true).await;

    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys(MEMO)
        .await
        .expect("the memo must be typed");
    wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "unsaved").await;
    wait_for_value(&driver, "textarea[data-memo]", MEMO).await;

    offline(&driver, false).await;
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    assert_eq!(
        attribute(&driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
    wait_for_value(&driver, "textarea[data-memo]", MEMO).await;
    wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;
    wait_for_d1_memo(&server, &training_id, MEMO).await;

    let window = driver.new_window().await.expect("a new window");
    driver
        .switch_to_window(window)
        .await
        .expect("the new window must be selected");
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must open in the new window");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    assert_eq!(
        attribute(&driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
    wait_for_value(&driver, "textarea[data-memo]", MEMO).await;
    wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    driver.quit().await.expect("the browser must close");
}

async fn offline(driver: &WebDriver, offline: bool) {
    driver
        .cdp()
        .send_raw(
            "Network.emulateNetworkConditions",
            json!({
                "offline": offline,
                "latency": 0,
                "downloadThroughput": if offline { 0 } else { -1 },
                "uploadThroughput": if offline { 0 } else { -1 }
            }),
        )
        .await
        .expect("the network condition must change");
}

async fn attribute(driver: &WebDriver, selector: &str, attribute: &str) -> String {
    driver
        .execute(
            "const element = document.querySelector(arguments[0]); return element ? element.getAttribute(arguments[1]) : '';",
            vec![json!(selector), json!(attribute)],
        )
        .await
        .expect("the attribute must be readable")
        .json()
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

async fn value(driver: &WebDriver, selector: &str) -> String {
    driver
        .execute(
            "const element = document.querySelector(arguments[0]); return element ? element.value : '';",
            vec![json!(selector)],
        )
        .await
        .expect("the value must be readable")
        .json()
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

async fn wait_for_attribute(
    driver: &WebDriver,
    selector: &str,
    attribute_name: &str,
    expected: &str,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if attribute(driver, selector, attribute_name).await == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the attribute {attribute_name} of {selector} did not become {expected}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_value(driver: &WebDriver, selector: &str, expected: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if value(driver, selector).await == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the value of {selector} did not become {expected}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_d1_memo(server: &DevServer, training_id: &str, expected: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let result = server
            .execute_d1_json(&format!(
                "SELECT memo FROM training_sessions WHERE id = '{training_id}'"
            ))
            .expect("the session must be readable");
        let memo = result
            .get(0)
            .and_then(|entry| entry.get("results"))
            .and_then(|rows| rows.get(0))
            .and_then(|row| row.get("memo"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
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
