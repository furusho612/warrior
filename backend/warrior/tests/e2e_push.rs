//! プッシュ通知の購読と Service Worker の通知の表示の E2E。
//!
//! 対応する要求: `REQ-TMR-008`、`REQ-NFR-008`。
//! ローカルの Chrome から実際のプッシュサービスへは接続しないため、`PushManager` の購読だけを
//! テストの中で差し替える。API と D1、Service Worker の表示は実物を使い、`push` のイベントは
//! CDP の `ServiceWorker.deliverPushMessage` で届ける。
//! 通知を押したときの画面の移動は、Chrome が合成した `notificationclick` に対して
//! ウィンドウのフォーカスと表示を拒否するため、このテストでは確かめない。

mod support;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::record_browser;
use support::{current_user_id, seed_profile, DevServer, TestOidc};
use thirtyfour::prelude::*;
use warrior_core::base64url;

const SERVER_PORT: u16 = 8872;
const PROVIDER_PORT: u16 = 8873;
const ENDPOINT: &str = "https://fcm.googleapis.com/fcm/send/warrior-e2e";
const NOTIFICATION_TITLE: &str = "レスト終了";
const NOTIFICATION_BODY: &str = "レストタイマーが終了しました。";
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::test]
async fn the_app_subscribes_and_the_service_worker_shows_the_push_notification() {
    let origin = format!("http://localhost:{SERVER_PORT}");
    let provider_origin = format!("http://localhost:{PROVIDER_PORT}");
    let public_key = "A".repeat(87);
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
            ("VAPID_PUBLIC_KEY", public_key.as_str()),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open_with_notification_permission(&driver_process).await;

    record_browser::login(&driver, &server, &provider, "record-push").await;
    let user_id = current_user_id(&driver).await;
    seed_profile(&server, &user_id);
    record_browser::start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::wait_for_present(&driver, "[data-rest-settings]").await;
    assert_eq!(
        notification_permission(&driver).await,
        "granted",
        "the test must run with the notification permission granted"
    );

    install_push_subscription_stub(&driver).await;
    browser::click(&driver, "[data-push-subscription]").await;
    browser::wait_for_attribute(&driver, "[data-push-setting]", "data-push-enabled", "true").await;

    let subscription = wait_for_subscription(&server, &user_id);
    assert_eq!(subscription["endpoint"], ENDPOINT);
    assert_eq!(
        base64url::decode(subscription["p256dh"].as_str().expect("the stored p256dh"))
            .map(|key| key.len()),
        Some(65)
    );
    assert_eq!(
        base64url::decode(subscription["auth"].as_str().expect("the stored auth"))
            .map(|key| key.len()),
        Some(16)
    );
    let expected_terminal = terminal_id(&server, &user_id);
    assert_eq!(
        subscription["terminal_token_id"].as_str(),
        Some(expected_terminal.as_str()),
        "the subscription must belong to the terminal that created it"
    );

    wait_for_service_worker(&driver).await;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock must be after the epoch")
        .as_secs() as i64;
    deliver_push(
        &driver,
        &origin,
        &json!({
            "reservation_id": "late-reservation",
            "version": 1,
            "end_at": now - 31,
            "exercise": "ベンチプレス",
            "weight": 100,
            "reps": 5,
        }),
    )
    .await;
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        notifications(&driver).await.is_empty(),
        "a notification later than 30 seconds after the end must be discarded"
    );
    deliver_push(
        &driver,
        &origin,
        &json!({
            "reservation_id": "fresh-reservation",
            "version": 1,
            "end_at": now,
            "exercise": "ベンチプレス",
            "weight": 100,
            "reps": 5,
        }),
    )
    .await;
    let notification = wait_for_notification(&driver).await;
    assert_eq!(notification["title"], NOTIFICATION_TITLE);
    assert_eq!(notification["body"], NOTIFICATION_BODY);
    for record_content in ["ベンチプレス", "100", "5"] {
        assert!(
            !notification["body"]
                .as_str()
                .unwrap_or_default()
                .contains(record_content),
            "the notification must not contain the record content {record_content}: {notification}"
        );
    }

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

/// 実際のプッシュサービスへ接続せず、購読の結果だけをテストの中で作る。
async fn install_push_subscription_stub(driver: &WebDriver) {
    let script = format!(
        r#"
        (function () {{
            const endpoint = {endpoint};
            const bytes = function (length, value) {{
                const array = new Uint8Array(length);
                array.fill(value);
                return array.buffer;
            }};
            const subscription = Object.create(PushSubscription.prototype);
            Object.defineProperty(subscription, 'endpoint', {{
                configurable: true,
                value: endpoint
            }});
            subscription.getKey = function (name) {{
                if (name === 'p256dh') {{
                    return bytes(65, 1);
                }}
                if (name === 'auth') {{
                    return bytes(16, 2);
                }}
                return null;
            }};
            subscription.unsubscribe = function () {{
                return Promise.resolve(true);
            }};
            PushManager.prototype.getSubscription = function () {{
                return Promise.resolve(subscription);
            }};
            PushManager.prototype.subscribe = function () {{
                return Promise.resolve(subscription);
            }};
        }})();
        "#,
        endpoint = serde_json::to_string(ENDPOINT).expect("a JSON endpoint"),
    );
    driver
        .execute(&script, Vec::<Value>::new())
        .await
        .expect("the push subscription stub must be installed");
}

async fn notification_permission(driver: &WebDriver) -> String {
    browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(String(window.Notification ? window.Notification.permission : 'unsupported'));",
        Vec::new(),
    )
    .await
}

async fn wait_for_service_worker(driver: &WebDriver) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let state = browser::eval_string(
            driver,
            "const done = arguments[arguments.length - 1]; \
             navigator.serviceWorker.ready.then((registration) => done(registration.active ? 'active' : 'waiting')).catch((error) => done('error:' + error));",
            Vec::new(),
        )
        .await;
        if state == "active" {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the service worker did not become active: {state}"
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

async fn deliver_push(driver: &WebDriver, origin: &str, payload: &Value) {
    driver
        .cdp()
        .send_raw(
            "Browser.grantPermissions",
            json!({ "origin": origin, "permissions": ["notifications"] }),
        )
        .await
        .expect("the notification permission must be granted");
    driver
        .cdp()
        .send_raw("ServiceWorker.enable", json!({}))
        .await
        .expect("the service worker domain must be enabled");
    // このテストの ChromeDriver 157 と新しいプロファイルでは、`ServiceWorker.enable` の後に
    // 観測される唯一の登録の ID は 0 である (実測)。ID が変わると通知が現れず、待機が失敗する。
    driver
        .cdp()
        .send_raw(
            "ServiceWorker.deliverPushMessage",
            json!({
                "origin": origin,
                "registrationId": "0",
                "data": payload.to_string(),
            }),
        )
        .await
        .expect("the push message must be delivered");
}

async fn notifications(driver: &WebDriver) -> Vec<Value> {
    let notifications = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         navigator.serviceWorker.ready \
           .then((registration) => registration.getNotifications()) \
           .then((list) => done(JSON.stringify(list.map((notification) => ({ title: notification.title, body: notification.body }))))) \
           .catch((error) => done(JSON.stringify({ error: String(error) })));",
        Vec::new(),
    )
    .await;
    notifications.as_array().cloned().unwrap_or_default()
}

async fn wait_for_notification(driver: &WebDriver) -> Value {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let notifications = browser::eval_json(
            driver,
            "const done = arguments[arguments.length - 1]; \
             navigator.serviceWorker.ready \
               .then((registration) => registration.getNotifications()) \
               .then((list) => done(JSON.stringify(list.map((notification) => ({ title: notification.title, body: notification.body }))))) \
               .catch((error) => done(JSON.stringify({ error: String(error) })));",
            Vec::new(),
        )
        .await;
        if let Some(notification) = notifications
            .as_array()
            .and_then(|list| list.iter().find(|item| item["title"] == NOTIFICATION_TITLE))
        {
            return notification.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the push notification did not appear: {notifications}"
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_subscription(server: &DevServer, user_id: &str) -> Value {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let rows = server
            .execute_d1_json(&format!(
                "SELECT endpoint, p256dh, auth, terminal_token_id \
                 FROM push_subscriptions WHERE user_id = '{user_id}'"
            ))
            .expect("the subscription must be readable");
        if let Some(row) = rows
            .get(0)
            .and_then(|entry| entry.get("results"))
            .and_then(|rows| rows.get(0))
        {
            return row.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the push subscription did not reach D1"
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn terminal_id(server: &DevServer, user_id: &str) -> String {
    server
        .execute_d1_json(&format!(
            "SELECT id FROM terminal_tokens WHERE user_id = '{user_id}'"
        ))
        .expect("the terminal token must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("id"))
        .and_then(Value::as_str)
        .expect("a terminal id")
        .to_owned()
}
