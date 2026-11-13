//! オフラインの記録、失敗した作成の破棄、通信の復帰、再読み込みの E2E。
//!
//! 対応する要求: `REQ-NFR-003`、`REQ-NFR-004`、`REQ-NFR-005`。
//! Chrome の CDP で通信を切り、IndexedDB への保存、復帰後の同期、未保存の表示を確かめる。

mod support;

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const MEMO: &str = "オフラインのメモ";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4501";

#[tokio::test]
async fn chrome_records_offline_discards_a_failed_create_and_syncs_after_reconnect() {
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_other_user(&server);
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    driver
        .goto(format!("{}/test/user?sub=offline", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(&driver, &server).await;

    discard_failed_create_scenario(&driver, &server).await;
    offline_memo_scenario(&driver, &server).await;

    driver.quit().await.expect("the browser must close");
}

async fn discard_failed_create_scenario(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/record", server.origin()))
        .await
        .expect("the record page must open");
    browser::wait_for_route(driver, "/record").await;
    offline(driver, true).await;
    browser::click_text(driver, "記録を開始").await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    let training_id = browser::attribute(driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty());

    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys("blocked")
        .await
        .expect("the memo must be typed");
    wait_for_blocked_update(driver, "pending").await;

    seed_conflicting_session(server, &training_id);
    offline(driver, false).await;
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(driver, "/record/:training_id").await;
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "failed").await;
    browser::wait_for_text(driver, "送信に失敗した操作").await;
    wait_for_blocked_update(driver, "failed").await;

    browser::click_text(driver, "破棄").await;
    browser::wait_for_route(driver, "/record").await;
    wait_for_empty_local_records(driver).await;
}

async fn offline_memo_scenario(driver: &WebDriver, server: &DevServer) {
    browser::click_text(driver, "記録を開始").await;
    browser::wait_for_route(driver, "/record/:training_id").await;
    let training_id = browser::attribute(driver, "[data-session-id]", "data-session-id").await;

    offline(driver, true).await;
    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys(MEMO)
        .await
        .expect("the memo must be typed");
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "unsaved").await;
    browser::wait_for_attribute(driver, "[data-persisted-memo]", "data-persisted-memo", MEMO).await;

    offline(driver, false).await;
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(driver, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
    browser::wait_for_value(driver, "textarea[data-memo]", MEMO).await;
    wait_for_d1_memo(server, &training_id, MEMO).await;
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "saved").await;

    let old_window = driver.window().await.expect("the current window");
    let new_window = driver.new_window().await.expect("a new window");
    driver
        .switch_to_window(new_window.clone())
        .await
        .expect("the new window must be selected");
    driver
        .switch_to_window(old_window)
        .await
        .expect("the old window must be selected");
    driver
        .close_window()
        .await
        .expect("the old window must close");
    driver
        .switch_to_window(new_window)
        .await
        .expect("the new window must be selected again");
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must open in the new window");
    browser::wait_for_route(driver, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
    browser::wait_for_value(driver, "textarea[data-memo]", MEMO).await;
    browser::wait_for_attribute(driver, "[data-sync-status]", "data-sync-status", "saved").await;
}

async fn offline(driver: &WebDriver, offline: bool) {
    driver
        .cdp()
        .send_raw("Network.enable", json!({}))
        .await
        .expect("the network must be enabled");
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

async fn wait_for_blocked_update(driver: &WebDriver, create_state: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let entries = outbox_entries(driver).await;
        let create = entries
            .iter()
            .find(|entry| entry["kind"] == "training_session_create");
        let update = entries
            .iter()
            .find(|entry| entry["kind"] == "training_session_update");
        if let (Some(create), Some(update)) = (create, update) {
            if create["state"] == create_state
                && update["state"] == "blocked"
                && update["parent"] == create["id"]
            {
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the update did not become blocked by the {create_state} create: {entries:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_empty_local_records(driver: &WebDriver) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        if outbox_entries(driver).await.is_empty() && sessions(driver).await.is_empty() {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the discarded session did not leave IndexedDB"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn outbox_entries(driver: &WebDriver) -> Vec<Value> {
    let value = browser::eval_json(
        driver,
        r#"
const done = arguments[arguments.length - 1];
const request = indexedDB.open('warrior');
request.onerror = () => done(JSON.stringify([]));
request.onsuccess = () => {
  const database = request.result;
  const transaction = database.transaction('outbox', 'readonly');
  const all = transaction.objectStore('outbox').getAll();
  all.onsuccess = () => {
    const rows = all.result.map((entry) => ({
      id: entry.operation.operation_id,
      kind: entry.operation.kind,
      state: entry.state,
      parent: entry.operation.parent_operation_id,
      memo: entry.operation.payload.memo
    }));
    database.close();
    done(JSON.stringify(rows));
  };
  all.onerror = () => { database.close(); done(JSON.stringify([])); };
};
"#,
        Vec::new(),
    )
    .await;
    value.as_array().cloned().unwrap_or_default()
}

async fn sessions(driver: &WebDriver) -> Vec<Value> {
    let value = browser::eval_json(
        driver,
        r#"
const done = arguments[arguments.length - 1];
const request = indexedDB.open('warrior');
request.onerror = () => done(JSON.stringify([]));
request.onsuccess = () => {
  const database = request.result;
  const transaction = database.transaction('training_sessions', 'readonly');
  const all = transaction.objectStore('training_sessions').getAll();
  all.onsuccess = () => {
    database.close();
    done(JSON.stringify(all.result));
  };
  all.onerror = () => { database.close(); done(JSON.stringify([])); };
};
"#,
        Vec::new(),
    )
    .await;
    value.as_array().cloned().unwrap_or_default()
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

fn seed_other_user(server: &DevServer) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-other-offline', 'other-offline@example.test', '別の利用者', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the other user must be seeded");
}

fn seed_conflicting_session(server: &DevServer, training_id: &str) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{training_id}', '{OTHER_USER_ID}', '2026-11-12', '2026-11-12T03:00:00Z', '別の記録', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ))
        .expect("the conflicting session must be seeded");
}
