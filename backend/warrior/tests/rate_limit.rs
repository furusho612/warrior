//! IP ごとのレート制限の結合テスト。
//!
//! 対応する要求: `REQ-NFR-017`。
//! 上限の直前と直後、IP の分離、CF-Connecting-IP の必須を確かめる。

mod support;

use support::DevServer;

#[test]
fn auth_and_passkey_are_limited_per_ip_and_require_cf_connecting_ip() {
    let server = DevServer::start_with_vars(
        8798,
        &[
            ("RATE_LIMIT_AUTH_IP", "1"),
            ("RATE_LIMIT_AUTH_WINDOW", "60"),
            ("RATE_LIMIT_PASSKEY_IP", "1"),
            ("RATE_LIMIT_PASSKEY_WINDOW", "60"),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let origin = ("Origin", "http://localhost:8787");

    let first = server.request(
        "GET",
        "/api/auth/google/start",
        &[("CF-Connecting-IP", "203.0.113.10")],
    );
    assert_eq!(first.status, 302);
    let limited = server.request(
        "GET",
        "/api/auth/google/start",
        &[("CF-Connecting-IP", "203.0.113.10")],
    );
    assert_eq!(limited.status, 429, "{:?}", server.log_lines());
    assert!(
        limited.body.contains("\"code\":\"rate_limited\""),
        "{}",
        limited.body
    );
    let retry = limited
        .header("retry-after")
        .and_then(|value| value.parse::<u64>().ok())
        .expect("Retry-After");
    assert!((1..=60).contains(&retry));

    let other_ip = server.request(
        "GET",
        "/api/auth/google/start",
        &[("CF-Connecting-IP", "203.0.113.11")],
    );
    assert_eq!(other_ip.status, 302);

    let passkey_first = server.request_with_body(
        "POST",
        "/api/auth/passkey/login/start",
        &[("CF-Connecting-IP", "203.0.113.10"), origin],
        "{}",
    );
    assert_eq!(passkey_first.status, 200);
    let passkey_limited = server.request_with_body(
        "POST",
        "/api/auth/passkey/login/start",
        &[("CF-Connecting-IP", "203.0.113.10"), origin],
        "{}",
    );
    assert_eq!(passkey_limited.status, 429);
    assert!(passkey_limited.header("retry-after").is_some());
}

#[test]
fn the_counter_restarts_in_the_next_window() {
    let server = DevServer::start_with_vars(
        8800,
        &[("RATE_LIMIT_AUTH_IP", "1"), ("RATE_LIMIT_AUTH_WINDOW", "1")],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let headers = [("CF-Connecting-IP", "203.0.113.20")];
    assert_eq!(
        server
            .request("GET", "/api/auth/google/start", &headers)
            .status,
        302
    );
    assert_eq!(
        server
            .request("GET", "/api/auth/google/start", &headers)
            .status,
        429
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let status = server
            .request("GET", "/api/auth/google/start", &headers)
            .status;
        if status == 302 {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the next window must restart the counter"
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
