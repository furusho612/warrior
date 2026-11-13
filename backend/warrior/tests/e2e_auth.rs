//! Google OAuth とセッションの E2E。
//!
//! 対応する要求: `REQ-NFR-006`、`REQ-NFR-013`。
//! Chrome と Safari の実ブラウザで、ログイン、`/api/me`、ログアウト、再ログインを通す。

mod support;

use support::browser::{self, Browser};
use support::{DevServer, TestOidc};
use thirtyfour::{SameSite, WebDriver};
use warrior_core::auth::hash_session_token;

#[tokio::test]
async fn wrangler_google_login_creates_and_revokes_a_session_in_a_browser() {
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
        .goto(format!("{}/test/user?sub=alice", provider.origin()))
        .await
        .expect("the test user must be selected");

    login(&driver, &server).await;
    let me = get_text(&driver, &format!("{}/api/me", server.origin())).await;
    assert!(me.contains("\"email\":\"alice@example.test\""), "{me}");
    assert!(me.contains("\"display_name\":\"テスト利用者\""), "{me}");
    let first_id = serde_json::from_str::<serde_json::Value>(&me)
        .expect("the me response is JSON")
        .get("id")
        .and_then(|value| value.as_str())
        .expect("the user id")
        .to_owned();

    let cookies = driver
        .get_all_cookies()
        .await
        .expect("the browser cookies must be readable");
    let session = cookies
        .iter()
        .find(|cookie| cookie.name == "warrior_session")
        .expect("the session cookie must exist");
    if browser == Browser::Chrome {
        assert_eq!(
            session.secure,
            Some(true),
            "the session cookie must be Secure"
        );
        assert_eq!(
            session.http_only,
            Some(true),
            "the session cookie must be HttpOnly"
        );
        assert_eq!(session.same_site, Some(SameSite::Lax));
    } else {
        // SafariDriver は Cookie の属性を返さないことがある。属性の正本は結合テストと単体テストで確かめる。
        assert_ne!(session.secure, Some(false));
        assert_ne!(session.http_only, Some(false));
        if let Some(same_site) = session.same_site {
            assert_eq!(same_site, SameSite::Lax);
        }
    }
    let stored = server
        .execute_d1_json("SELECT token_hash FROM auth_sessions")
        .expect("the session rows must be readable");
    let token_hash = stored
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("token_hash"))
        .and_then(|value| value.as_str())
        .expect("a stored token hash");
    assert_eq!(token_hash, hash_session_token(&session.value));
    assert_ne!(token_hash, session.value);

    logout(&driver, &server).await;
    let after_logout = get_text(&driver, &format!("{}/api/me", server.origin())).await;
    assert!(
        after_logout.contains("\"code\":\"unauthorized\""),
        "{after_logout}"
    );
    let remaining = server
        .execute_d1_json("SELECT COUNT(*) AS count FROM auth_sessions")
        .expect("the session rows must be readable");
    let count = remaining
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_i64())
        .expect("a session count");
    assert_eq!(count, 0, "logout must delete the session row");

    let error_page = get_text(&driver, &format!("{}/login/error", server.origin())).await;
    assert!(
        error_page.contains("ログインできませんでした"),
        "{error_page}; URL: {:?}",
        driver.current_url().await
    );

    login(&driver, &server).await;
    let again = get_text(&driver, &format!("{}/api/me", server.origin())).await;
    assert!(
        again.contains("\"email\":\"alice@example.test\""),
        "{again}"
    );
    let second_me =
        serde_json::from_str::<serde_json::Value>(&again).expect("the me response is JSON");
    let second_id = second_me
        .get("id")
        .and_then(|value| value.as_str())
        .expect("the user id");
    assert_eq!(
        first_id, second_id,
        "the same Google sub must keep the user"
    );

    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/login", server.origin()))
        .await
        .expect("the login page must open");
    wait_for_route(driver, server, "/login").await;
    driver
        .execute(
            "const button = document.querySelector('button'); if (!button) { return false; } button.click(); return true;",
            Vec::<serde_json::Value>::new(),
        )
        .await
        .expect("the login button must be clickable");
    wait_for_route(driver, server, "/").await;
}

async fn logout(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/", server.origin()))
        .await
        .expect("the home page must open");
    wait_for_route(driver, server, "/").await;
    driver
        .execute_async(
            "const done = arguments[arguments.length - 1]; fetch('/api/auth/logout', { method: 'POST' }).then((response) => response.text()).then((text) => done(text)).catch((error) => done(String(error)));",
            Vec::<serde_json::Value>::new(),
        )
        .await
        .expect("the logout request must complete");
}

async fn get_text(driver: &WebDriver, url: &str) -> String {
    driver.goto(url).await.expect("the page must open");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let text = driver
            .execute(
                "return document.body ? document.body.innerText : ''",
                Vec::<serde_json::Value>::new(),
            )
            .await
            .expect("the page text must be readable")
            .json()
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if !text.is_empty() {
            return text;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the page text did not appear: {url}"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

async fn wait_for_route(driver: &WebDriver, server: &DevServer, expected: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let source = driver.source().await.unwrap_or_default();
        if source.contains(&format!("data-route=\"{expected}\"")) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the route {expected} did not appear; current URL: {:?}; logs: {:?}",
            driver.current_url().await,
            server.log_lines()
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
