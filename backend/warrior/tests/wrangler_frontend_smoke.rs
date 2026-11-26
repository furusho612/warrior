//! フロントエンドの画面の E2E スモーク。
//!
//! 対応する要求: `REQ-NFR-001`、`REQ-NFR-002`、`REQ-NFR-011`。
//! Chrome と Safari の実ブラウザで、経路、下部のタブ、記録中の枠、最小サイズを確かめる。

mod support;

use serde_json::Value;
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::WebDriver;

const SERVER_PORT: u16 = 8795;
const PROVIDER_PORT: u16 = 8796;

#[tokio::test]
async fn wrangler_frontend_smoke_opens_every_route_and_the_record_shell() {
    let browser = Browser::from_env();
    let origin = format!("http://localhost:{SERVER_PORT}");
    let provider_origin = format!("http://localhost:{PROVIDER_PORT}");
    let server = DevServer::start_with_vars(
        SERVER_PORT,
        &[
            ("ORIGIN", origin.as_str()),
            (
                "GOOGLE_OAUTH_AUTHORIZE_URL",
                format!("{provider_origin}/authorize").as_str(),
            ),
            ("GOOGLE_OAUTH_ISSUER", provider_origin.as_str()),
            (
                "GOOGLE_OAUTH_TOKEN_URL",
                format!("{provider_origin}/token").as_str(),
            ),
            (
                "GOOGLE_OAUTH_USERINFO_URL",
                format!("{provider_origin}/userinfo").as_str(),
            ),
            (
                "GOOGLE_OAUTH_REDIRECT_URI",
                format!("{origin}/api/auth/google/callback").as_str(),
            ),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;
    driver
        .goto(format!("{}/test/user?sub=smoke", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(&driver, &server).await;

    for (path, expected) in [
        ("/login", "/login"),
        ("/login/error", "/login"),
        ("/", "/"),
        ("/record", "/record"),
        ("/record/abc", "/record/:training_id"),
        ("/calendar", "/calendar"),
        ("/exercises", "/exercises"),
        ("/exercises/abc", "/exercises/:exercise_id"),
        ("/summary/abc", "/summary/:training_id"),
        ("/analysis", "/analysis"),
        ("/routines", "/routines"),
        ("/mypage", "/mypage"),
        ("/account", "/account"),
    ] {
        open_route(&driver, &server, path, expected).await;
    }

    open_route(&driver, &server, "/", "/").await;
    let tabs = eval_json(
        &driver,
        "return JSON.stringify(Array.from(document.querySelectorAll('nav a')).map((a) => a.textContent.trim()))",
    )
    .await;
    assert_eq!(
        tabs,
        serde_json::json!(["ホーム", "カレンダー", "種目", "分析", "マイページ"])
    );

    open_route(&driver, &server, "/record", "/record").await;
    let record_shell = eval_json(
        &driver,
        "return JSON.stringify({nav: document.querySelector('nav') !== null, rail: document.querySelector('aside')?.textContent.trim() ?? null, timer: document.querySelector('[data-rest-timer]') !== null})",
    )
    .await;
    assert_eq!(
        record_shell,
        serde_json::json!({ "nav": false, "rail": "種目", "timer": true })
    );

    open_route(&driver, &server, "/", "/").await;
    let sizes = eval_json(
        &driver,
        "return JSON.stringify({overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth, shortTargets: Array.from(document.querySelectorAll('a,button')).filter((el) => el.offsetParent !== null && el.getBoundingClientRect().height < 44).length, smallText: Array.from(document.querySelectorAll('body *')).filter((el) => el.offsetParent !== null && el.textContent.trim() !== '' && parseFloat(getComputedStyle(el).fontSize) < 11).length})",
    )
    .await;
    assert_eq!(
        sizes,
        serde_json::json!({ "overflow": 0, "shortTargets": 0, "smallText": 0 })
    );

    driver
        .set_window_rect(0, 0, 1280, 800)
        .await
        .expect("the desktop window must be sized");
    let desktop_sizes = eval_json(
        &driver,
        "return JSON.stringify({overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth, shortTargets: Array.from(document.querySelectorAll('a,button')).filter((el) => el.offsetParent !== null && el.getBoundingClientRect().height < 44).length, smallText: Array.from(document.querySelectorAll('body *')).filter((el) => el.offsetParent !== null && el.textContent.trim() !== '' && parseFloat(getComputedStyle(el).fontSize) < 11).length})",
    )
    .await;
    assert_eq!(
        desktop_sizes,
        serde_json::json!({ "overflow": 0, "shortTargets": 0, "smallText": 0 })
    );

    driver.quit().await.expect("the browser must close");
}

async fn open_route(driver: &WebDriver, server: &DevServer, path: &str, expected: &str) {
    driver
        .goto(format!("{}{}", server.origin(), path))
        .await
        .expect("the page must open");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let route = eval_value(
            driver,
            "const element = document.querySelector('[data-route]'); return element ? element.getAttribute('data-route') : null",
        )
        .await;
        if route == Value::String(expected.to_owned()) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{path}: expected data-route {expected}, got {route:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn eval_value(driver: &WebDriver, script: &str) -> Value {
    driver
        .execute(script, Vec::<Value>::new())
        .await
        .expect("the script must run")
        .json()
        .clone()
}

async fn eval_json(driver: &WebDriver, script: &str) -> Value {
    let result = driver
        .execute(script, Vec::<Value>::new())
        .await
        .expect("the script must run");
    let text = result
        .json()
        .clone()
        .as_str()
        .expect("the script must return a JSON string")
        .to_owned();
    serde_json::from_str(&text).expect("valid JSON")
}
