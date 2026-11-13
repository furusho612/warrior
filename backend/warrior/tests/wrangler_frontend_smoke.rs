//! フロントエンドの画面の E2E スモーク。
//!
//! 対応する要求: `REQ-NFR-001`、`REQ-NFR-002`、`REQ-NFR-011`。
//! Chrome と Safari の実ブラウザで、経路、下部のタブ、記録中の枠、最小サイズを確かめる。

mod support;

use serde_json::Value;
use support::browser::{self, Browser};
use support::DevServer;
use thirtyfour::WebDriver;

#[tokio::test]
async fn wrangler_frontend_smoke_opens_every_route_and_the_record_shell() {
    let browser = Browser::from_env();
    let server = DevServer::start(8795);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    for (path, expected) in [
        ("/login", "/login"),
        ("/", "/"),
        ("/record", "/record"),
        ("/record/abc", "/record/:training_id"),
        ("/calendar", "/calendar"),
        ("/library", "/library"),
        ("/library/abc", "/library/:exercise_id"),
        ("/summary/abc", "/summary/:training_id"),
        ("/analysis", "/analysis"),
        ("/routines", "/routines"),
        ("/mypage", "/mypage"),
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
        "return JSON.stringify({nav: document.querySelector('nav') !== null, rail: document.querySelector('aside')?.textContent.trim() ?? null, timer: Array.from(document.querySelectorAll('div')).some((el) => el.textContent.trim() === 'レストタイマー')})",
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
