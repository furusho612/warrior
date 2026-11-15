//! 種目ライブラリの検索、絞り込み、お気に入り、詳細、履歴の入口の E2E。
//!
//! 対応する要求: `REQ-LIB-002`、`REQ-LIB-006`、`REQ-LIB-007`、`REQ-LIB-008`。
//! 種目を検索し、部位区分と使用器具で絞り込み、お気に入りに登録し、詳細を開いて
//! 主働筋と補助筋、やり方、注意点、コツ、履歴とグラフの入口を確かめる。

mod support;

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::prelude::*;

const SERVER_PORT: u16 = 8861;
const PROVIDER_PORT: u16 = 8862;
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const INCLINE_BARBELL: &str = "a7c4e2f0-0000-4000-8000-000000000001";
const DECLINE_BARBELL: &str = "a7c4e2f0-0000-4000-8000-000000000002";
const DECLINE_DUMBBELL: &str = "a7c4e2f0-0000-4000-8000-000000000008";

#[tokio::test]
async fn searches_filters_favorites_and_opens_the_detail_and_history_entry() {
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
            ("LIBRARY_PAGE_SIZE", "5"),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_detail_text(&server);
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    login(&driver, &server, &provider).await;
    driver
        .goto(format!("{}/exercises", server.origin()))
        .await
        .expect("the library must open");
    browser::wait_for_route(&driver, "/exercises").await;
    browser::wait_for_attribute(&driver, "[data-library]", "data-library-loaded", "true").await;
    check_initial_page(&driver).await;
    check_search(&driver).await;
    check_filters(&driver).await;
    check_favorite_and_detail(&driver).await;
    check_favorite_only_removal(&driver).await;
    check_pagination(&driver).await;
    check_empty_search(&driver).await;

    if browser == Browser::Chrome {
        browser::assert_no_panic(&driver).await;
    }
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=library", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(driver, server).await;
    browser::wait_for_route(driver, "/").await;
}

async fn check_initial_page(driver: &WebDriver) {
    browser::wait_for_text(driver, "種目ライブラリ").await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "5").await;
    assert_eq!(
        library_ids(driver).await,
        vec![
            INCLINE_BARBELL.to_owned(),
            DECLINE_BARBELL.to_owned(),
            BENCH.to_owned(),
            "a7c4e2f0-0000-4000-8000-000000000004".to_owned(),
            "a7c4e2f0-0000-4000-8000-000000000005".to_owned(),
        ]
    );
}

async fn check_search(driver: &WebDriver) {
    browser::set_input(driver, "[data-library-query]", "デクライン").await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "2").await;
    assert_eq!(
        library_ids(driver).await,
        vec![DECLINE_BARBELL.to_owned(), DECLINE_DUMBBELL.to_owned()]
    );
    browser::set_input(driver, "[data-library-query]", "").await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "5").await;
}

async fn check_filters(driver: &WebDriver) {
    browser::click(driver, "[data-library-part=\"chest\"]").await;
    browser::wait_for_attribute(
        driver,
        "[data-library-part=\"chest\"]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::click(driver, "[data-library-equipment=\"barbell\"]").await;
    browser::wait_for_attribute(
        driver,
        "[data-library-equipment=\"barbell\"]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "3").await;
    assert_eq!(
        library_ids(driver).await,
        vec![
            INCLINE_BARBELL.to_owned(),
            DECLINE_BARBELL.to_owned(),
            BENCH.to_owned(),
        ]
    );
}

async fn check_favorite_and_detail(driver: &WebDriver) {
    let row = format!("[data-library-exercise][data-exercise-id=\"{BENCH}\"]");
    let favorite = format!("{row} [data-library-favorite]");
    browser::click(driver, &favorite).await;
    browser::wait_for_attribute(driver, &favorite, "aria-pressed", "true").await;

    browser::click(driver, "[data-library-favorite-only]").await;
    browser::wait_for_attribute(
        driver,
        "[data-library-favorite-only]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "1").await;
    assert_eq!(library_ids(driver).await, vec![BENCH.to_owned()]);

    browser::click(driver, &format!("{row} [data-library-open]")).await;
    browser::wait_for_route(driver, "/exercises/:exercise_id").await;
    browser::wait_for_attribute(
        driver,
        "[data-exercise-detail]",
        "data-exercise-loaded",
        "true",
    )
    .await;
    assert_eq!(
        browser::attribute(driver, "[data-exercise-detail]", "data-exercise-id").await,
        BENCH
    );
    assert_eq!(
        text_of(driver, "[data-exercise-name]").await,
        "ベンチプレス"
    );
    assert!(text_of(driver, "[data-exercise-field=\"body-part\"]")
        .await
        .contains("胸"));
    assert!(text_of(driver, "[data-exercise-field=\"primary\"]")
        .await
        .contains("大胸筋中部"));
    assert!(text_of(driver, "[data-exercise-field=\"secondary\"]")
        .await
        .contains("上腕三頭筋"));
    assert!(text_of(driver, "[data-exercise-field=\"equipment\"]")
        .await
        .contains("バーベル"));
    assert!(text_of(driver, "[data-exercise-field=\"instructions\"]")
        .await
        .contains("肩甲骨を寄せて押す"));
    assert!(text_of(driver, "[data-exercise-field=\"cautions\"]")
        .await
        .contains("肩をすくめない"));
    assert!(text_of(driver, "[data-exercise-field=\"tips\"]")
        .await
        .contains("足で床を押す"));
    let history = browser::attribute(driver, "[data-exercise-history]", "href").await;
    assert!(
        history.ends_with(&format!("/exercises/{BENCH}/history")),
        "the history link must point to the exercise history: {history}"
    );

    browser::click(driver, "[data-exercise-back]").await;
    browser::wait_for_route(driver, "/exercises").await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-loaded", "true").await;
}

async fn check_favorite_only_removal(driver: &WebDriver) {
    browser::click(driver, "[data-library-favorite-only]").await;
    browser::wait_for_attribute(
        driver,
        "[data-library-favorite-only]",
        "aria-pressed",
        "true",
    )
    .await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "1").await;
    let row = format!("[data-library-exercise][data-exercise-id=\"{BENCH}\"]");
    browser::click(driver, &format!("{row} [data-library-favorite]")).await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "0").await;
    browser::wait_for_present(driver, "[data-library-empty]").await;
    browser::wait_for_text(driver, "検索や絞り込みの条件を変えてください。").await;
}

async fn check_pagination(driver: &WebDriver) {
    browser::click(driver, "[data-library-favorite-only]").await;
    browser::wait_for_attribute(
        driver,
        "[data-library-favorite-only]",
        "aria-pressed",
        "false",
    )
    .await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "5").await;
    browser::wait_for_present(driver, "[data-library-more]").await;
    browser::click(driver, "[data-library-more]").await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "10").await;
    let ids = library_ids(driver).await;
    assert_eq!(ids.len(), 10);
    assert_eq!(ids[0], INCLINE_BARBELL);
    assert_eq!(ids[4], "a7c4e2f0-0000-4000-8000-000000000005");
    assert_eq!(ids[5], "a7c4e2f0-0000-4000-8000-000000000006");
}

async fn check_empty_search(driver: &WebDriver) {
    browser::set_input(driver, "[data-library-query]", "存在しない種目").await;
    browser::wait_for_attribute(driver, "[data-library]", "data-library-count", "0").await;
    browser::wait_for_present(driver, "[data-library-empty]").await;
    browser::wait_for_text(driver, "種目ライブラリに表示できる種目がまだありません。").await;
}

fn seed_detail_text(server: &DevServer) {
    server
        .execute_d1_json(&format!(
            "UPDATE master_exercises SET instructions = '肩甲骨を寄せて押す', \
             cautions = '肩をすくめない', tips = '足で床を押す' WHERE id = '{BENCH}'"
        ))
        .expect("the detail text must be stored");
}

async fn library_ids(driver: &WebDriver) -> Vec<String> {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-library-exercise]')) \
           .map((element) => element.getAttribute('data-exercise-id'))));",
        Vec::new(),
    )
    .await;
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
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
