//! バインディングに依存しない純粋なロジック。

pub mod base64url;
pub mod cursor;
pub mod datetime;
pub mod ids;
pub mod jst;
pub mod sql;

/// ヘルスチェックが返す状態の値。
pub const HEALTH_STATUS: &str = "ok";

/// API のヘルスチェックの経路。
pub const HEALTH_PATH: &str = "/api/health";

#[cfg(test)]
mod tests {
    use super::{HEALTH_PATH, HEALTH_STATUS};

    #[test]
    fn health_status_is_ok() {
        assert_eq!(HEALTH_STATUS, "ok");
    }

    #[test]
    fn health_path_is_under_api() {
        assert_eq!(HEALTH_PATH, "/api/health");
    }
}
