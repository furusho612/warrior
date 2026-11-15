//! レスト終了のブラウザ通知の E2E。
//!
//! 対応する要求: `REQ-TMR-007`、`REQ-NFR-008`。
//! 通知を許可した Chrome でタイマーを動かし、別のタブを表示している間に通知が届くことを
//! 確かめる。ヘッドレスの Chrome からは OS の通知を観測できないため、テストが
//! `window.Notification` を包み、本物の通知を作ったうえで題名と本文、作成したときの
//! 画面の表示状態を記録する。記録は同じオリジンの別のタブから localStorage で読む。

mod support;

use std::time::{Duration, Instant};

use serde_json::Value;
use support::browser::{self, Browser};
use support::record_browser;
use support::{current_user_id, seed_profile, DevServer, TestOidc};
use thirtyfour::prelude::*;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8866;
const PROVIDER_PORT: u16 = 8867;
const NOTIFICATION_KEY: &str = "__warrior_notification";
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const WAIT_TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::test]
async fn notifies_the_rest_finish_while_another_tab_is_displayed() {
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
    let driver = browser::open_with_notification_permission(&driver_process).await;

    record_browser::login(&driver, &server, &provider, "record-notification").await;
    let user_id = current_user_id(&driver).await;
    seed_profile(&server, &user_id);
    let training_id = record_browser::start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::wait_for_present(&driver, "[data-rest-settings]").await;
    assert_eq!(
        notification_permission(&driver).await,
        "granted",
        "the test must run with the notification permission granted"
    );

    install_notification_spy(&driver).await;
    browser::click(&driver, "[data-notification]").await;
    browser::wait_for_attribute(
        &driver,
        "[data-notification-enabled]",
        "data-notification-enabled",
        "true",
    )
    .await;

    browser::set_input(&driver, "[data-rest-default]", "8").await;
    browser::wait_for_value(&driver, "[data-rest-default]", "8").await;
    let set_id = record_browser::add_set(&driver).await;
    browser::set_input(&driver, &record_browser::weight_selector(&set_id), "100").await;
    browser::set_input(&driver, &record_browser::reps_selector(&set_id), "5").await;
    record_browser::wait_for_complete_ready(&driver, &set_id).await;
    browser::click(&driver, &record_browser::complete_selector(&set_id)).await;
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-active",
        "true",
    )
    .await;
    let end_ms = browser::attribute(&driver, "[data-rest-timer]", "data-rest-timer-end-ms").await;
    assert!(!end_ms.is_empty(), "the timer must expose its end time");

    let timer_window = driver.window().await.expect("the timer window");
    open_other_tab(&driver, &origin, &timer_window).await;

    let notification = wait_for_notification(&driver).await;
    assert_eq!(
        notification["failure"], "",
        "the browser must create the notification: {notification}"
    );
    assert_eq!(notification["title"], "レスト終了");
    assert_eq!(notification["body"], "レストタイマーが終了しました。");
    assert_eq!(
        notification["hidden"], true,
        "the notification must be created while the timer tab is hidden"
    );
    for record_content in ["ベンチプレス", "100", "5 回"] {
        assert!(
            !notification["body"]
                .as_str()
                .unwrap_or_default()
                .contains(record_content),
            "the notification must not contain the record content {record_content}: {notification}"
        );
    }

    driver
        .switch_to_window(timer_window)
        .await
        .expect("the timer tab must be selected");
    browser::wait_for_attribute(
        &driver,
        "[data-rest-timer]",
        "data-rest-timer-finished",
        "true",
    )
    .await;
    assert_eq!(
        browser::attribute(
            &driver,
            "[data-rest-timer-remaining]",
            "data-rest-timer-remaining"
        )
        .await,
        "00:00"
    );
    assert_eq!(
        browser::attribute(&driver, "[data-rest-timer]", "data-rest-timer-end-ms").await,
        end_ms,
        "the notification must not overwrite the end time"
    );
    assert_eq!(
        browser::attribute(&driver, "[data-session-id]", "data-session-id").await,
        training_id
    );

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn install_notification_spy(driver: &WebDriver) {
    driver
        .execute(
            r#"
            const key = arguments[0];
            localStorage.removeItem(key);
            (function () {
                const Original = window.Notification;
                if (!Original) {
                    return;
                }
                function SpiedNotification(title, options) {
                    let created = null;
                    let failure = '';
                    try {
                        created = new Original(title, options);
                    } catch (error) {
                        failure = String(error);
                    }
                    const record = {
                        title: String(title),
                        body: options && options.body ? String(options.body) : '',
                        hidden: document.visibilityState === 'hidden',
                        failure: failure
                    };
                    try {
                        localStorage.setItem(key, JSON.stringify(record));
                    } catch (error) {
                        record.failure = String(error);
                    }
                    return created;
                }
                SpiedNotification.permission = Original.permission;
                SpiedNotification.requestPermission = function () {
                    return Original.requestPermission();
                };
                SpiedNotification.prototype = Original.prototype;
                Object.defineProperty(window, 'Notification', {
                    configurable: true,
                    value: SpiedNotification
                });
            })();
            "#,
            vec![serde_json::json!(NOTIFICATION_KEY)],
        )
        .await
        .expect("the notification spy must be installed");
}

async fn open_other_tab(driver: &WebDriver, origin: &str, timer_window: &WindowHandle) {
    let opened = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         const opened = window.open(arguments[0], '_blank'); \
         done(JSON.stringify(opened !== null));",
        vec![serde_json::json!(format!("{origin}/calendar"))],
    )
    .await
    .as_bool()
    .unwrap_or(false);
    assert!(opened, "the other tab must open");
    let deadline = Instant::now() + WAIT_TIMEOUT;
    let other_window = loop {
        let windows = driver
            .windows()
            .await
            .expect("the windows must be readable");
        if let Some(window) = windows.into_iter().find(|window| window != timer_window) {
            break window;
        }
        assert!(Instant::now() < deadline, "the other tab did not appear");
        std::thread::sleep(POLL_INTERVAL);
    };
    driver
        .switch_to_window(other_window)
        .await
        .expect("the other tab must be selected");
    browser::wait_for_route(driver, "/calendar").await;
    let visibility = browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; done(document.visibilityState);",
        Vec::new(),
    )
    .await;
    assert_eq!(
        visibility, "visible",
        "the other tab must be the displayed tab"
    );
}

async fn wait_for_notification(driver: &WebDriver) -> Value {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let raw = browser::eval_string(
            driver,
            "const done = arguments[arguments.length - 1]; \
             done(localStorage.getItem(arguments[0]) || '');",
            vec![serde_json::json!(NOTIFICATION_KEY)],
        )
        .await;
        if let Ok(notification) = serde_json::from_str::<Value>(&raw) {
            return notification;
        }
        assert!(
            Instant::now() < deadline,
            "the rest notification did not arrive"
        );
        std::thread::sleep(POLL_INTERVAL);
    }
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
