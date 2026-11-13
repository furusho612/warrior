//! Worker の HTTP 基盤の結合テスト。
//!
//! 対応する要求: `REQ-NFR-009` (通信の暗号化と本人だけの閲覧を支える同一オリジンの検証)。
//! ヘッダの組み立てと Origin の値の比較は単体テストが担う。
//! このテストは `wrangler dev` の実物に対する応答を確かめる。

mod support;

use serde_json::Value;
use support::DevServer;

const ORIGIN: &str = "http://localhost:8787";

#[test]
fn wrangler_http_foundation_api_responses_have_security_headers_and_the_shared_error_shape() {
    let server = DevServer::start(8791);

    let health = server.request("GET", "/api/health", &[]);
    assert_eq!(health.status, 200);
    assert_eq!(
        serde_json::from_str::<Value>(&health.body).unwrap(),
        serde_json::json!({ "status": "ok" })
    );
    assert_security_headers(&health);
    assert!(health.header("x-request-id").is_some());
    assert_eq!(health.header("access-control-allow-origin"), None);

    let unknown = server.request("GET", "/api/unknown", &[]);
    assert_eq!(unknown.status, 404);
    assert_eq!(
        serde_json::from_str::<Value>(&unknown.body).unwrap(),
        serde_json::json!({
            "error": { "code": "not_found", "message": "route not found" }
        })
    );
    assert_security_headers(&unknown);
    assert_eq!(unknown.header("access-control-allow-origin"), None);

    let method_not_allowed = server.request(
        "POST",
        "/api/health",
        &[("Origin", ORIGIN), ("Content-Type", "application/json")],
    );
    assert_eq!(method_not_allowed.status, 405);
    assert_eq!(
        serde_json::from_str::<Value>(&method_not_allowed.body).unwrap(),
        serde_json::json!({
            "error": { "code": "method_not_allowed", "message": "method not allowed" }
        })
    );

    let forbidden = server.request("POST", "/api/health", &[("Origin", "https://evil.example")]);
    assert_eq!(forbidden.status, 403);
    assert_eq!(
        serde_json::from_str::<Value>(&forbidden.body).unwrap(),
        serde_json::json!({
            "error": {
                "code": "forbidden",
                "message": "the request origin is not the app origin"
            }
        })
    );
    assert_security_headers(&forbidden);

    let get_with_foreign_origin =
        server.request("GET", "/api/health", &[("Origin", "https://evil.example")]);
    assert_eq!(get_with_foreign_origin.status, 200);

    let static_page = server.request("GET", "/", &[]);
    assert_eq!(static_page.status, 200);
    assert_security_headers(&static_page);
    assert_eq!(static_page.header("access-control-allow-origin"), None);

    let logs = server.wait_for_log("\"route\":\"health\"");
    let health_log = logs
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .find(|value| value["route"] == "health" && value["status"] == 200)
        .expect("a health request log line");
    assert_eq!(health_log["event"], "request");
    assert_eq!(health_log["method"], "GET");
    assert!(health_log["duration_ms"].is_number());
    assert_eq!(health_log["error_code"], Value::Null);
    assert!(health_log["request_id"]
        .as_str()
        .is_some_and(|id| !id.is_empty()));
    for private in [
        "email",
        "memo",
        "token",
        "push_subscription",
        "ai_input",
        "ai_output",
    ] {
        assert!(
            health_log.get(private).is_none(),
            "{private} must not be logged"
        );
    }
}

fn assert_security_headers(response: &support::HttpResponse) {
    assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        response.header("referrer-policy"),
        Some("strict-origin-when-cross-origin")
    );
    assert_eq!(
        response.header("strict-transport-security"),
        Some("max-age=31536000; includeSubDomains")
    );
    assert_eq!(
        response.header("permissions-policy"),
        Some("camera=(), microphone=(), geolocation=(), payment=(), usb=()")
    );
    let csp = response.header("content-security-policy").unwrap();
    assert!(csp.contains("script-src 'self' 'wasm-unsafe-eval'"));
    assert!(csp.contains("https://fonts.googleapis.com"));
    assert!(csp.contains("font-src https://fonts.gstatic.com"));
}
