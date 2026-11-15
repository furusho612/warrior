//! マイページの設定と表示モードの E2E。
//!
//! 対応する要求: `REQ-MYP-002`、`REQ-MYP-003`、`REQ-MYP-005`、`REQ-NFR-012`、`REQ-NFR-019`。
//! レスト時間の既定値、ブラウザ通知、表示モードを設定し、再読み込みの後も保たれること、
//! 端末の設定の変更に追従すること、別の利用者の設定と混ざらないことを実ブラウザで確かめる。
//! Worker は 8880、テスト専用の OIDC の提供者は 8881 を使う。

mod support;

use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::browser::{self, Browser};
use support::{record_browser, DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8880;
const PROVIDER_PORT: u16 = 8881;
const USER_A: &str = "settings-a";
const USER_B: &str = "settings-b";
const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(100);

#[tokio::test]
async fn keeps_the_settings_and_display_mode_across_reloads_and_users() {
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

    record_browser::login(&driver, &server, &provider, USER_A).await;
    let user_a = support::current_user_id(&driver).await;
    open_mypage(&driver, &server).await;

    assert_eq!(
        browser::attribute(&driver, "html", "data-display-mode").await,
        "system",
        "a user without settings must follow the device"
    );
    browser::wait_for_value(&driver, "[data-settings-rest-default]", "90").await;
    browser::wait_for_attribute(
        &driver,
        "[data-display-mode=\"system\"]",
        "aria-pressed",
        "true",
    )
    .await;

    browser::set_input(&driver, "[data-settings-rest-default]", "120").await;
    browser::wait_for_value(&driver, "[data-settings-rest-default]", "120").await;
    wait_for_setting(&server, &user_a, "rest_default_seconds", "120").await;

    browser::click(&driver, "[data-notification]").await;
    browser::wait_for_attribute(
        &driver,
        "[data-notification-enabled]",
        "data-notification-enabled",
        "true",
    )
    .await;

    browser::click(&driver, "[data-display-mode=\"dark\"]").await;
    browser::wait_for_attribute(&driver, "html", "data-theme", "dark").await;
    wait_for_setting(&server, &user_a, "display_mode", "dark").await;
    browser::click(&driver, "[data-pfc-protein]").await;
    browser::set_input(&driver, "[data-pfc-protein]", "1.8").await;
    browser::set_input(&driver, "[data-pfc-fat]", "25").await;
    wait_for_setting(&server, &user_a, "protein_g_per_kg", "1.8").await;
    wait_for_setting(&server, &user_a, "fat_ratio", "0.25").await;
    check_touch_targets_and_text_size(&driver).await;

    driver.refresh().await.expect("the page must reload");
    open_after_reload(&driver, &server).await;
    browser::wait_for_value(&driver, "[data-settings-rest-default]", "120").await;
    assert_eq!(
        browser::attribute(
            &driver,
            "[data-notification-enabled]",
            "data-notification-enabled"
        )
        .await,
        "true",
        "the browser notification setting must survive a reload"
    );
    assert_eq!(
        browser::attribute(&driver, "html", "data-theme").await,
        "dark",
        "the saved display mode must be applied before the settings are fetched"
    );
    assert_eq!(
        browser::attribute(&driver, "[data-display-mode=\"dark\"]", "aria-pressed").await,
        "true"
    );
    browser::wait_for_value(&driver, "[data-pfc-protein]", "1.8").await;
    browser::wait_for_value(&driver, "[data-pfc-fat]", "25").await;

    browser::click(&driver, "[data-display-mode=\"system\"]").await;
    browser::wait_for_attribute(&driver, "html", "data-display-mode", "system").await;
    wait_for_setting(&server, &user_a, "display_mode", "system").await;
    emulate_device_mode(&driver, "dark").await;
    browser::wait_for_attribute(&driver, "html", "data-theme", "dark").await;
    emulate_device_mode(&driver, "light").await;
    browser::wait_for_attribute(&driver, "html", "data-theme", "light").await;

    browser::click(&driver, "[data-display-mode=\"dark\"]").await;
    browser::wait_for_attribute(&driver, "html", "data-theme", "dark").await;
    wait_for_setting(&server, &user_a, "display_mode", "dark").await;

    browser::logout(&driver).await;
    record_browser::login(&driver, &server, &provider, USER_B).await;
    let user_b = support::current_user_id(&driver).await;
    open_mypage(&driver, &server).await;
    browser::wait_for_value(&driver, "[data-settings-rest-default]", "90").await;
    browser::wait_for_attribute(
        &driver,
        "[data-display-mode=\"system\"]",
        "aria-pressed",
        "true",
    )
    .await;
    assert_eq!(
        setting_value(&server, &user_b, "rest_default_seconds").await,
        None,
        "another user must not inherit the first user's settings"
    );
    browser::set_input(&driver, "[data-settings-rest-default]", "30").await;
    wait_for_setting(&server, &user_b, "rest_default_seconds", "30").await;
    assert_eq!(
        setting_value(&server, &user_a, "rest_default_seconds")
            .await
            .as_deref(),
        Some("120"),
        "changing another user's settings must not overwrite the first user's settings"
    );

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn open_mypage(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/mypage", server.origin()))
        .await
        .expect("the my page must open");
    browser::wait_for_route(driver, "/mypage").await;
    browser::wait_for_attribute(
        driver,
        "[data-route=\"/mypage\"]",
        "data-settings-loaded",
        "true",
    )
    .await;
}

async fn open_after_reload(driver: &WebDriver, server: &DevServer) {
    let _ = server;
    browser::wait_for_route(driver, "/mypage").await;
    browser::wait_for_attribute(
        driver,
        "[data-route=\"/mypage\"]",
        "data-settings-loaded",
        "true",
    )
    .await;
}

async fn emulate_device_mode(driver: &WebDriver, mode: &str) {
    driver
        .cdp()
        .send_raw(
            "Emulation.setEmulatedMedia",
            json!({
                "features": [{"name": "prefers-color-scheme", "value": mode}]
            }),
        )
        .await
        .expect("the device color scheme must be emulated");
}

async fn check_touch_targets_and_text_size(driver: &WebDriver) {
    let bad = browser::eval_json(
        driver,
        r#"
        const done = arguments[arguments.length - 1];
        const root = document.querySelector('[data-route="/mypage"]');
        const bad = [];
        root.querySelectorAll('button, label').forEach(function (element) {
            if (element.tagName === 'LABEL' && !element.querySelector('input[type="checkbox"]')) {
                return;
            }
            const box = element.getBoundingClientRect();
            if (box.width > 0 && box.height > 0 && box.height < 43.5) {
                bad.push('touch ' + element.tagName + ' ' + box.height);
            }
        });
        root.querySelectorAll('*').forEach(function (element) {
            if (element.children.length !== 0) {
                return;
            }
            const text = (element.textContent || '').trim();
            if (!text || element.tagName === 'INPUT') {
                return;
            }
            const size = parseFloat(getComputedStyle(element).fontSize);
            if (size < 10.5) {
                bad.push('text ' + element.tagName + ' ' + size);
            }
        });
        done(JSON.stringify(bad));
        "#,
        Vec::new(),
    )
    .await;
    assert_eq!(
        bad,
        json!([]),
        "the my page must meet the touch and text sizes"
    );
}

async fn wait_for_setting(server: &DevServer, user_id: &str, column: &str, expected: &str) {
    let deadline = Instant::now() + WAIT;
    loop {
        if setting_value(server, user_id, column).await.as_deref() == Some(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the user setting {column} did not become {expected}"
        );
        std::thread::sleep(POLL);
    }
}

async fn setting_value(server: &DevServer, user_id: &str, column: &str) -> Option<String> {
    let sql = format!("SELECT {column} AS value FROM user_settings WHERE user_id = '{user_id}'");
    let value = server.execute_d1_json(&sql).ok()?;
    scalar(value.get(0)?.get("results")?.get(0)?.get("value")?)
}

fn scalar(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_f64().map(|value| value.to_string()))
}
