//! アカウントの管理画面の E2E。
//!
//! 対応する要求: `REQ-MYP-006`、`REQ-MYP-007`、`REQ-NFR-010`。
//! マイページからアカウントの画面を開き、Google アカウントの表示、パスキーの入口、
//! 再認証の案内、ログアウト、削除の確認と取り消し、削除の後始末を通す。

mod support;

use std::time::{Duration, Instant};

use serde_json::Value;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::prelude::*;

const USER_SUB: &str = "alice";
const MEMO: &str = "削除の前の記録";

#[tokio::test]
async fn chrome_account_settings_logs_out_and_deletes_all_device_data_after_confirmation() {
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    select_user(&driver, &provider).await;
    browser::login_with_google(&driver, &server).await;
    wait_for_start(&driver).await;
    let old_user_id = current_user_id(&driver).await;

    open_route(&driver, &server, "/mypage", "/mypage").await;
    browser::wait_for_text(&driver, "パスキーとアカウントの管理").await;
    click_link(&driver, "パスキーとアカウントの管理").await;
    browser::wait_for_route(&driver, "/account").await;

    browser::wait_for_text(&driver, "テスト利用者").await;
    browser::wait_for_text(&driver, "alice@example.test").await;
    browser::wait_for_present(&driver, "[data-passkey-settings]").await;
    browser::wait_for_text(&driver, "パスキーはまだ登録されていません").await;
    browser::click_text(&driver, "パスキーを追加").await;
    browser::wait_for_text(
        &driver,
        "操作の前に Google アカウントまたはパスキーで再認証してください",
    )
    .await;

    browser::click(&driver, "[data-account-logout]").await;
    browser::wait_for_route(&driver, "/login").await;
    let after_logout = me(&driver).await;
    assert_eq!(after_logout["error"]["code"], "unauthorized");

    select_user(&driver, &provider).await;
    browser::login_with_google(&driver, &server).await;
    wait_for_start(&driver).await;
    assert_eq!(current_user_id(&driver).await, old_user_id);

    browser::click(&driver, "[data-start]").await;
    browser::wait_for_route(&driver, "/record/:training_id").await;
    let training_id = browser::attribute(&driver, "[data-session-id]", "data-session-id").await;
    assert!(!training_id.is_empty(), "the record must have a session id");
    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea
        .send_keys(MEMO)
        .await
        .expect("the memo must be typed");
    browser::wait_for_attribute(
        &driver,
        "[data-persisted-memo]",
        "data-persisted-memo",
        MEMO,
    )
    .await;

    let cached = wait_for_cache(&driver).await;
    assert!(
        cached
            .iter()
            .any(|name| name.starts_with("warrior-static-")),
        "{cached:?}"
    );

    open_route(&driver, &server, "/mypage", "/mypage").await;
    click_link(&driver, "パスキーとアカウントの管理").await;
    browser::wait_for_route(&driver, "/account").await;
    browser::wait_for_text(&driver, "テスト利用者").await;

    browser::click(&driver, "[data-account-delete]").await;
    browser::wait_for_text(
        &driver,
        "身体情報、記録、目標、AI の出力、書き出し、種目のお気に入り、設定、パスキー、セッション、端末トークン、プッシュ購読、レスト通知の予約を含む、利用者に属するすべてのデータが消えます。",
    )
    .await;
    browser::wait_for_text(&driver, "この操作は取り消せません。").await;
    browser::wait_for_text(
        &driver,
        "サーバーのバックアップの保持期間中は、提供者のバックアップから復元できる可能性があります。",
    )
    .await;
    browser::click(&driver, "[data-account-delete-cancel]").await;
    browser::wait_for_absent(&driver, "[data-account-delete-confirmation]").await;
    let still_exists = me(&driver).await;
    assert_eq!(still_exists["id"], old_user_id);

    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the app must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    assert_eq!(
        browser::attribute(&driver, "[data-session-id]", "data-session-id").await,
        training_id,
        "the record must remain after the cancellation"
    );

    open_route(&driver, &server, "/account", "/account").await;
    let seeded = browser::eval_json(
        &driver,
        "const done = arguments[arguments.length - 1]; localStorage.setItem('warrior.settings.rest_seconds', '120'); sessionStorage.setItem('warrior.settings.display', 'dark'); done(JSON.stringify({ local: localStorage.length, session: sessionStorage.length }));",
        Vec::new(),
    )
    .await;
    assert!(seeded["local"].as_u64().unwrap_or(0) >= 1, "{seeded}");
    assert!(seeded["session"].as_u64().unwrap_or(0) >= 1, "{seeded}");
    browser::click(&driver, "[data-account-delete]").await;
    browser::offline(&driver, true).await;
    browser::click(&driver, "[data-account-delete-confirm]").await;
    browser::wait_for_text(&driver, "アカウントを削除できませんでした").await;
    browser::wait_for_present(&driver, "[data-account-delete-retry]").await;
    browser::offline(&driver, false).await;
    browser::click(&driver, "[data-account-delete-retry]").await;
    browser::wait_for_route(&driver, "/login").await;

    let local = local_state(&driver).await;
    assert_eq!(local["local"].as_u64(), Some(0), "{local}");
    assert_eq!(local["session"].as_u64(), Some(0), "{local}");
    assert_eq!(local["caches"].as_array().map(Vec::len), Some(0), "{local}");
    let databases = indexed_databases(&driver).await;
    assert!(
        !databases.iter().any(|name| name == "warrior"),
        "{databases:?}"
    );
    let after_delete = me(&driver).await;
    assert_eq!(after_delete["error"]["code"], "unauthorized");
    let cookies = driver
        .get_all_cookies()
        .await
        .expect("the browser cookies must be readable");
    for name in ["warrior_session", "warrior_terminal", "warrior_elevation"] {
        assert!(
            cookies.iter().all(|cookie| cookie.name != name),
            "the cookie {name} must be cleared"
        );
    }

    driver
        .goto(format!("{}/login", server.origin()))
        .await
        .expect("the login page must reload");
    browser::wait_for_route(&driver, "/login").await;
    let recached = wait_for_cache(&driver).await;
    assert!(
        recached
            .iter()
            .any(|name| name.starts_with("warrior-static-")),
        "{recached:?}"
    );
    wait_for_cache_url(&driver, "/").await;

    select_user(&driver, &provider).await;
    browser::login_with_google(&driver, &server).await;
    browser::wait_for_route(&driver, "/").await;
    wait_for_start(&driver).await;
    let new_user_id = current_user_id(&driver).await;
    assert_ne!(new_user_id, old_user_id);
    assert!(
        driver.find(By::Css("[data-session-id]")).await.is_err(),
        "the old record must not be restored"
    );

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn select_user(driver: &WebDriver, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub={USER_SUB}", provider.origin()))
        .await
        .expect("the test user must be selected");
}

async fn wait_for_start(driver: &WebDriver) {
    browser::wait_for_attribute(
        driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
}

async fn open_route(driver: &WebDriver, server: &DevServer, path: &str, route: &str) {
    driver
        .goto(format!("{}{}", server.origin(), path))
        .await
        .expect("the page must open");
    browser::wait_for_route(driver, route).await;
}

async fn click_link(driver: &WebDriver, label: &str) {
    let link = driver
        .find(By::XPath(format!(
            "//a[contains(normalize-space(.), '{}')]",
            label
        )))
        .await
        .unwrap_or_else(|error| panic!("the link {label} must exist: {error}"));
    link.click().await.expect("the link must be clickable");
}

async fn me(driver: &WebDriver) -> Value {
    let text = browser::eval_string(
        driver,
        "const done = arguments[arguments.length - 1]; fetch('/api/me').then((response) => response.text()).then((text) => done(text));",
        Vec::new(),
    )
    .await;
    serde_json::from_str(&text).expect("the me response must be JSON")
}

async fn local_state(driver: &WebDriver) -> Value {
    browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; (async () => { const keys = await caches.keys(); done(JSON.stringify({ local: localStorage.length, session: sessionStorage.length, caches: keys })); })().catch(() => done(JSON.stringify({ local: localStorage.length, session: sessionStorage.length, caches: [] })));",
        Vec::new(),
    )
    .await
}

async fn indexed_databases(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; indexedDB.databases().then((databases) => done(JSON.stringify(databases.map((database) => database.name)))).catch(() => done('[]'));",
        Vec::new(),
    )
    .await;
    string_array(&value)
}

async fn cache_keys(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; caches.keys().then((keys) => done(JSON.stringify(keys))).catch(() => done('[]'));",
        Vec::new(),
    )
    .await;
    string_array(&value)
}

async fn cache_urls(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; (async () => { const keys = await caches.keys(); const urls = []; for (const name of keys) { const cache = await caches.open(name); const requests = await cache.keys(); for (const request of requests) { urls.push(new URL(request.url).pathname); } } done(JSON.stringify(urls)); })().catch(() => done('[]'));",
        Vec::new(),
    )
    .await;
    string_array(&value)
}

async fn wait_for_cache(driver: &WebDriver) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let keys = cache_keys(driver).await;
        if !keys.is_empty() {
            return keys;
        }
        assert!(
            Instant::now() < deadline,
            "the service worker did not cache the app shell"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_cache_url(driver: &WebDriver, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let urls = cache_urls(driver).await;
        if urls.iter().any(|url| url == expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the cache did not contain {expected}: {urls:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
