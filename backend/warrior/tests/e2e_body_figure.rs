//! 種目詳細の人体図と主働筋、補助筋の E2E。
//!
//! 対応する要求: `REQ-LIB-003`、`REQ-LIB-012`。
//! 種目の詳細を開き、前面と背面の人体図が表示され、主働筋と補助筋が異なる色で
//! 示され、その名称が図の外に表示されることを確かめる。部位区分がその他の種目は、
//! 人体図の代わりに全身の表示を出す。

mod support;

use std::collections::BTreeSet;

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8863;
const PROVIDER_PORT: u16 = 8864;
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const CROSS_TRAINER: &str = "a7c4e2f0-0000-4000-8000-000000000061";

#[tokio::test]
async fn opens_an_exercise_and_shows_the_body_figure_and_the_muscle_names() {
    let browser = Browser::from_env();
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
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    login(&driver, &server, &provider).await;
    driver
        .goto(format!("{}/exercises/{BENCH}", server.origin()))
        .await
        .expect("the exercise detail must open");
    browser::wait_for_text(&driver, "ベンチプレス").await;
    browser::wait_for_attribute(
        &driver,
        "[data-exercise-detail]",
        "data-exercise-loaded",
        "true",
    )
    .await;
    check_figure(&driver).await;
    check_names(&driver).await;

    driver
        .goto(format!("{}/exercises/{CROSS_TRAINER}", server.origin()))
        .await
        .expect("the other exercise detail must open");
    browser::wait_for_text(&driver, "クロストレーナー").await;
    browser::wait_for_attribute(
        &driver,
        "[data-exercise-detail]",
        "data-exercise-loaded",
        "true",
    )
    .await;
    check_other(&driver).await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=body-figure", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(driver, server).await;
    browser::wait_for_route(driver, "/").await;
}

async fn check_figure(driver: &WebDriver) {
    browser::wait_for_present(driver, "[data-body-figure]").await;
    for side in ["front", "back"] {
        let selector = format!("[data-figure-side=\"{side}\"]");
        assert_eq!(
            browser::attribute(driver, &selector, "data-primary").await,
            "pec-mid"
        );
        let secondary = browser::attribute(driver, &selector, "data-secondary").await;
        assert_eq!(
            secondary.split_whitespace().collect::<BTreeSet<_>>(),
            BTreeSet::from(["triceps", "delt-front"])
        );
    }
    let result = driver
        .execute(
            r#"
            const units = ["pec-mid", "triceps", "pec-upper"];
            return units.map((unit) => {
                const element = document.querySelector(
                    `[data-figure-side="front"] .wfig-m[data-unit="${unit}"]`
                );
                return element ? getComputedStyle(element).fill : null;
            });
            "#,
            Vec::<serde_json::Value>::new(),
        )
        .await
        .expect("the figure colors must be readable");
    let colors = result.json();
    let colors = colors.as_array().expect("the colors must be an array");
    let color = |index: usize| {
        colors[index]
            .as_str()
            .expect("the color must be a string")
            .to_owned()
    };
    assert_ne!(color(0), color(1));
    assert_ne!(color(0), color(2));
    assert_ne!(color(1), color(2));
}

async fn check_names(driver: &WebDriver) {
    assert!(text_of(driver, "[data-exercise-field=\"primary\"]")
        .await
        .contains("大胸筋中部"));
    let secondary = text_of(driver, "[data-exercise-field=\"secondary\"]").await;
    assert!(secondary.contains("上腕三頭筋"));
    assert!(secondary.contains("三角筋前部"));
}

async fn check_other(driver: &WebDriver) {
    assert!(
        driver.find(By::Css("[data-body-figure]")).await.is_err(),
        "the other exercise must not show the muscle figure"
    );
    assert!(text_of(driver, "[data-exercise-field=\"other-scope\"]")
        .await
        .contains("全身"));
}

async fn text_of(driver: &WebDriver, selector: &str) -> String {
    driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the element {selector} must exist: {error}"))
        .text()
        .await
        .expect("the element text must be readable")
}
