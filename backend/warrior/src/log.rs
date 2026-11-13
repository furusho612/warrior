//! リクエスト 1 件ごとの JSON のログ。
//!
//! ログに載せるのは経路のメタデータだけとする。
//! 身体情報、メールアドレス、メモ、AI の入出力、トークン、プッシュ購読の情報は載せない。

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use worker::{console_log, Date};

/// ログ 1 行のイベント名。
pub const EVENT_REQUEST: &str = "request";

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// リクエスト 1 件のログ。
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RequestLog<'a> {
    pub event: &'static str,
    pub route: &'a str,
    pub method: &'a str,
    pub status: u16,
    pub duration_ms: u64,
    pub error_code: Option<&'a str>,
    pub request_id: &'a str,
}

impl<'a> RequestLog<'a> {
    /// ログの値を組み立てる。
    pub fn new(
        route: &'a str,
        method: &'a str,
        status: u16,
        duration_ms: u64,
        error_code: Option<&'a str>,
        request_id: &'a str,
    ) -> Self {
        Self {
            event: EVENT_REQUEST,
            route,
            method,
            status,
            duration_ms,
            error_code,
            request_id,
        }
    }
}

/// ログ 1 行の JSON を組み立てる。
pub fn line(log: &RequestLog<'_>) -> String {
    serde_json::to_string(log).expect("a request log with plain fields is always serializable")
}

/// wrangler の出力に 1 行の JSON としてログを出す。
pub fn log_request(log: &RequestLog<'_>) {
    console_log!("{}", line(log));
}

/// 時刻と連番からリクエスト ID を組み立てる。
pub fn format_request_id(millis: u64, sequence: u64) -> String {
    format!("{millis}-{sequence}")
}

/// リクエストごとに一意な ID を作る。
pub fn next_request_id() -> String {
    format_request_id(
        Date::now().as_millis(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::{format_request_id, line, RequestLog};

    #[test]
    fn log_line_contains_the_request_metadata() {
        let log = RequestLog::new("health", "GET", 200, 12, None, "1-1");
        let value: serde_json::Value = serde_json::from_str(&line(&log)).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "event": "request",
                "route": "health",
                "method": "GET",
                "status": 200,
                "duration_ms": 12,
                "error_code": null,
                "request_id": "1-1"
            })
        );
    }

    #[test]
    fn log_line_has_exactly_the_metadata_fields() {
        let log = RequestLog::new("not_found", "POST", 404, 3, Some("not_found"), "1-2");
        let value: serde_json::Value = serde_json::from_str(&line(&log)).unwrap();
        let mut keys = value
            .as_object()
            .expect("a log line is a JSON object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<&str>>();
        keys.sort_unstable();

        assert_eq!(
            keys,
            vec![
                "duration_ms",
                "error_code",
                "event",
                "method",
                "request_id",
                "route",
                "status",
            ]
        );
    }

    #[test]
    fn log_line_does_not_include_private_fields() {
        let log = RequestLog::new("health", "GET", 200, 1, None, "1-3");
        let value: serde_json::Value = serde_json::from_str(&line(&log)).unwrap();
        let object = value.as_object().unwrap();

        for private in [
            "body",
            "email",
            "memo",
            "token",
            "push_subscription",
            "ai_input",
            "ai_output",
            "height",
            "weight",
        ] {
            assert!(
                !object.contains_key(private),
                "{private} must not be logged"
            );
        }
    }

    #[test]
    fn request_id_combines_time_and_sequence() {
        assert_eq!(format_request_id(1_700_000_000_000, 7), "1700000000000-7");
        assert_ne!(
            format_request_id(1_700_000_000_000, 7),
            format_request_id(1_700_000_000_000, 8)
        );
    }
}
