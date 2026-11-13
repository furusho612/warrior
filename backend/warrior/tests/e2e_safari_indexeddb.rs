//! Safari で記録中に再読み込みし、記録画面とメモが復元されることの E2E。
//!
//! 対応する要求: `REQ-NFR-004`。
//! Safari の WebDriver は CDP を持たないため、通信の切断は Chrome の E2E が担う。

mod support;

use serde_json::json;
use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const MEMO: &str = "サファリのメモ";

#[tokio::test]
async fn safari_restores_the_record_screen_and_memo_after_reload() {
    let browser = Browser::from_env();
    let server = DevServer::start(8787);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(8788);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    driver
        .goto(format!("{}/test/user?sub=safari", provider.origin()))
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

    let textarea = driver
        .find(By::Css("textarea[data-memo]"))
        .await
        .expect("the memo must exist");
    textarea.click().await.expect("the memo must be focusable");
    textarea
        .send_keys(MEMO)
        .await
        .expect("the memo must be typed");
    wait_for_value(&driver, "textarea[data-memo]", MEMO).await;

    driver.refresh().await.expect("the page must reload");
    browser::wait_for_route(&driver, "/record/:training_id").await;
    assert_eq!(
        attribute(&driver, "[data-session-id]", "data-session-id").await,
        training_id
    );
    wait_for_value(&driver, "textarea[data-memo]", MEMO).await;

    driver.quit().await.expect("the browser must close");
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
