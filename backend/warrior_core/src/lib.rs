//! バインディングに依存しない純粋なロジック。

pub mod account_deletion;
pub mod ai;
pub mod auth;
pub mod base64url;
pub mod body_part_analysis;
pub mod calendar;
pub mod cbor;
pub mod cose;
pub mod cursor;
pub mod datetime;
pub mod duration;
pub mod exercise;
pub mod goal;
pub mod history;
pub mod ids;
pub mod jst;
pub mod library;
pub mod memo;
pub mod notification;
pub mod nutrition;
pub mod ownership;
pub mod personal_best;
pub mod previous;
pub mod profile;
pub mod push;
pub mod pwa;
pub mod rate_limit;
pub mod rest_timer;
pub mod rpe;
pub mod set_calc;
pub mod set_rules;
pub mod sql;
pub mod superset;
pub mod sync;
mod sync_exercise;
mod sync_set;
mod sync_superset;
pub mod training_record;
pub mod training_session;
pub mod webauthn;

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
