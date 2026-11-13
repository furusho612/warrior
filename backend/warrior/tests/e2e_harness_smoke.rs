//! E2E のハーネスのスモーク。
//!
//! 対応する要求はない。ADR-024 が定める E2E の基盤を確かめる。
//! 実物の Worker とブラウザを起動し、health の JSON を開く。

mod support;

use support::browser::{self, Browser};
use support::DevServer;

#[tokio::test]
async fn wrangler_e2e_harness_opens_health_json_in_a_browser() {
    let browser = Browser::from_env();
    let server = DevServer::start(8792);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    driver
        .goto(format!("{}/api/health", server.origin()))
        .await
        .expect("the health page must open");
    let source = driver.source().await.expect("the page source");
    assert!(source.contains("\"status\":\"ok\""), "{source}");
    assert_eq!(server.request("GET", "/api/health", &[]).status, 200);

    driver.quit().await.expect("the browser must close");
}
