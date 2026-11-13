//! PWA の manifest、Service Worker、キャッシュの E2E。
//!
//! 対応する要求: `REQ-NFR-001`、`REQ-TMR-008`。
//! Chrome で登録、キャッシュの対象、キャッシュミスの取得を確かめる。

mod support;

use serde_json::Value;
use support::browser::{self, Browser};
use support::DevServer;
use thirtyfour::prelude::*;

#[tokio::test]
async fn chrome_registers_the_service_worker_and_caches_static_assets() {
    let server = DevServer::start(8787);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let driver = browser::open(Browser::Chrome, &driver_process).await;

    driver
        .goto(server.origin())
        .await
        .expect("the app must open");
    wait_for_route(&driver, "/").await;

    let manifest = eval_json(
        &driver,
        "const done = arguments[arguments.length - 1]; fetch('/manifest.webmanifest').then((r) => r.json()).then((v) => done(JSON.stringify(v))).catch((error) => done(JSON.stringify({ error: String(error) })));",
    )
    .await;
    assert_eq!(manifest["name"], "WARRIOR");
    assert_eq!(manifest["display"], "standalone");
    assert_eq!(manifest["start_url"], "/");
    assert!(manifest["icons"]
        .as_array()
        .is_some_and(|icons| icons.len() >= 2));

    let ready = eval_string(
        &driver,
        "const done = arguments[arguments.length - 1]; navigator.serviceWorker.ready.then(() => { if (navigator.serviceWorker.controller) { done('ready'); } else { navigator.serviceWorker.addEventListener('controllerchange', () => done('ready'), { once: true }); } }).catch((error) => done('error:' + error));",
    )
    .await;
    assert_eq!(ready, "ready");
    driver
        .goto(server.origin())
        .await
        .expect("the app must reload under the service worker");
    wait_for_route(&driver, "/").await;

    let cached = eval_json(
        &driver,
        "const done = arguments[arguments.length - 1]; (async () => { const keys = await caches.keys(); const name = keys.find((key) => key.startsWith('warrior-static-')); if (!name) { done(JSON.stringify({ keys, urls: [] })); return; } const cache = await caches.open(name); const requests = await cache.keys(); done(JSON.stringify({ keys, urls: requests.map((request) => new URL(request.url).pathname) })); })();",
    )
    .await;
    assert!(
        cached["keys"]
            .as_array()
            .is_some_and(|keys| keys.iter().any(|key| key
                .as_str()
                .is_some_and(|key| key.starts_with("warrior-static-")))),
        "{cached}"
    );
    let urls = cached["urls"].as_array().expect("cached urls");
    assert!(
        urls.iter().any(|url| url == "/sw/warrior_sw_bg.wasm"),
        "{cached}"
    );
    assert!(urls.iter().any(|url| url == "/tailwind.css"), "{cached}");
    assert!(
        urls.iter()
            .any(|url| url.as_str().is_some_and(|url| url.ends_with(".wasm"))),
        "{cached}"
    );
    assert!(
        !urls.iter().any(|url| url
            .as_str()
            .is_some_and(|url| url.starts_with("/assets/") && url.ends_with(".js"))),
        "{cached}"
    );

    driver.quit().await.expect("the browser must close");
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

async fn eval_json(driver: &WebDriver, script: &str) -> Value {
    let result = driver
        .execute_async(script, Vec::<Value>::new())
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

async fn eval_string(driver: &WebDriver, script: &str) -> String {
    driver
        .execute_async(script, Vec::<Value>::new())
        .await
        .expect("the script must run")
        .json()
        .clone()
        .as_str()
        .expect("the script must return a string")
        .to_owned()
}
