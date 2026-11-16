//! レスト終了のプッシュ通知の予約の規則 (ADR-008)。
//!
//! 予約は版を持ち、延長と短縮では版を進めて古いメッセージを無視する。
//! 通知は終了予定時刻から 30 秒以内にだけ送り、同じ版のメッセージを複数回処理しても
//! 1 回だけ送る。このモジュールは時刻と状態の判断だけを持ち、D1 と Queue には触れない。

use serde::{Deserialize, Serialize};

/// 通知を送ってよい終了予定時刻からの猶予 (秒)。
pub const DELIVERY_WINDOW_SECONDS: i64 = 30;

/// レストの残り時間の上限 (秒)。
pub const MAX_REST_SECONDS: u32 = 3_600;

/// レストの残り時間の下限 (秒)。
pub const MIN_REST_SECONDS: u32 = 1;

/// Queue のメッセージ。予約 ID と版だけを持つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestPushMessage {
    /// 予約の ID。
    pub reservation_id: String,
    /// 予約の版。
    pub version: i64,
}

/// 通知の本文。記録の内容を含めない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestPushPayload {
    /// 予約の ID。
    pub reservation_id: String,
    /// 予約の版。
    pub version: i64,
    /// 終了予定時刻 (Unix 時刻の秒)。
    pub end_at: i64,
}

/// 通知の送信を判断する予約の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservationState {
    /// 予約が有効か。
    pub active: bool,
    /// 予約の版。
    pub version: i64,
    /// 終了予定時刻 (Unix 時刻の秒)。
    pub end_at: i64,
    /// 送信した時刻 (Unix 時刻の秒)。
    pub sent_at: Option<i64>,
}

/// 送信する条件を全て満たすか。
///
/// 有効であり、メッセージの版が一致し、現在時刻が終了予定時刻以上で、
/// 終了予定時刻から 30 秒以内であり、まだ送信していない場合だけ真になる。
pub fn should_send(reservation: ReservationState, message_version: i64, now: i64) -> bool {
    reservation.active
        && reservation.version == message_version
        && reservation.sent_at.is_none()
        && now >= reservation.end_at
        && now <= reservation.end_at.saturating_add(DELIVERY_WINDOW_SECONDS)
}

/// 要求の `TTL` を返す。終了予定時刻から 30 秒を上限とする。
pub fn ttl_seconds(reservation: ReservationState, now: i64) -> u32 {
    let elapsed = now.saturating_sub(reservation.end_at);
    let remaining = DELIVERY_WINDOW_SECONDS.saturating_sub(elapsed);
    u32::try_from(remaining.clamp(0, DELIVERY_WINDOW_SECONDS)).unwrap_or(0)
}

/// 終了予定時刻を返す。
pub fn end_at(now: i64, remaining_seconds: u32) -> i64 {
    now.saturating_add(i64::from(
        remaining_seconds.clamp(MIN_REST_SECONDS, MAX_REST_SECONDS),
    ))
}

/// 終了予定時刻までの Queue の遅延 (秒) を返す。
pub fn delay_seconds(end_at: i64, now: i64) -> u32 {
    u32::try_from(end_at.saturating_sub(now).max(0)).unwrap_or(u32::MAX)
}

/// 次の版を返す。
pub fn next_version(version: i64) -> i64 {
    version.saturating_add(1)
}

#[cfg(test)]
mod tests {
    //! 予約の版と送信の条件の例示テスト。
    //!
    //! 対応する要求: `REQ-TMR-009`、`REQ-TMR-010`。

    use super::{
        delay_seconds, end_at, next_version, should_send, ttl_seconds, ReservationState,
        DELIVERY_WINDOW_SECONDS,
    };

    fn reservation() -> ReservationState {
        ReservationState {
            active: true,
            version: 3,
            end_at: 1_700_000_100,
            sent_at: None,
        }
    }

    #[test]
    fn req_tmr_009_the_delivery_window_starts_at_the_end_and_lasts_thirty_seconds() {
        let value = reservation();
        assert!(!should_send(value, 3, value.end_at - 1));
        assert!(should_send(value, 3, value.end_at));
        assert!(should_send(
            value,
            3,
            value.end_at + DELIVERY_WINDOW_SECONDS
        ));
        assert!(!should_send(
            value,
            3,
            value.end_at + DELIVERY_WINDOW_SECONDS + 1
        ));
    }

    #[test]
    fn req_tmr_010_an_inactive_or_old_version_is_not_sent() {
        let mut value = reservation();
        assert!(!should_send(value, 2, value.end_at));
        assert!(!should_send(value, 4, value.end_at));
        value.active = false;
        assert!(!should_send(value, 3, value.end_at));
    }

    #[test]
    fn req_tmr_009_a_sent_reservation_is_not_sent_again() {
        let mut value = reservation();
        value.sent_at = Some(value.end_at);
        assert!(!should_send(value, 3, value.end_at));
    }

    #[test]
    fn req_tmr_009_the_ttl_decreases_from_thirty_seconds_to_zero() {
        let value = reservation();
        assert_eq!(ttl_seconds(value, value.end_at), 30);
        assert_eq!(ttl_seconds(value, value.end_at + 1), 29);
        assert_eq!(ttl_seconds(value, value.end_at + 30), 0);
        assert_eq!(ttl_seconds(value, value.end_at + 31), 0);
    }

    #[test]
    fn req_tmr_010_the_end_and_delay_clamp_the_remaining_seconds() {
        assert_eq!(end_at(1_000, 90), 1_090);
        assert_eq!(end_at(1_000, 0), 1_001);
        assert_eq!(end_at(1_000, 100_000), 4_600);
        assert_eq!(delay_seconds(1_090, 1_000), 90);
        assert_eq!(delay_seconds(1_000, 1_090), 0);
        assert_eq!(next_version(3), 4);
    }
}
