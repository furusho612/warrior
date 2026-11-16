//! レスト終了のプッシュ通知の予約の性質テスト。
//!
//! 対応する要求: `REQ-TMR-009`、`REQ-TMR-010`。

use proptest::prelude::*;
use warrior_core::rest_push::{
    delay_seconds, end_at, next_version, should_send, ttl_seconds, ReservationState,
    DELIVERY_WINDOW_SECONDS,
};

proptest! {
    #[test]
    fn req_tmr_009_the_send_condition_requires_every_condition(
        active in any::<bool>(),
        version in 1i64..10,
        message_version in 1i64..10,
        end_at in 1_700_000_000i64..1_700_001_000,
        sent_at in prop::option::of(1_700_000_000i64..1_700_001_000),
        now in 1_700_000_000i64..1_700_002_000,
    ) {
        let reservation = ReservationState {
            active,
            version,
            end_at,
            sent_at,
        };
        let expected = active
            && version == message_version
            && sent_at.is_none()
            && now >= end_at
            && now <= end_at + DELIVERY_WINDOW_SECONDS;
        prop_assert_eq!(should_send(reservation, message_version, now), expected);
    }

    #[test]
    fn req_tmr_009_the_ttl_is_between_zero_and_thirty(
        end_at in 1_700_000_000i64..1_700_001_000,
        now in 1_700_000_000i64..1_700_002_000,
    ) {
        let reservation = ReservationState {
            active: true,
            version: 1,
            end_at,
            sent_at: None,
        };
        let ttl = ttl_seconds(reservation, now);
        prop_assert!(ttl <= 30);
        if now < end_at {
            prop_assert_eq!(ttl, 30);
        }
        if now > end_at + DELIVERY_WINDOW_SECONDS {
            prop_assert_eq!(ttl, 0);
        }
    }

    #[test]
    fn req_tmr_010_the_delay_and_end_are_monotonic(
        now in 1_700_000_000i64..1_700_001_000,
        remaining in 1u32..3_600,
    ) {
        let end = end_at(now, remaining);
        prop_assert_eq!(end, now + i64::from(remaining));
        prop_assert_eq!(delay_seconds(end, now), remaining);
        prop_assert_eq!(delay_seconds(now, end), 0);
        prop_assert_eq!(next_version(remaining as i64), remaining as i64 + 1);
    }
}
