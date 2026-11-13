//! 固定の時間窓の性質テスト。
//!
//! 対応する要求: `REQ-NFR-017`。

use proptest::prelude::*;
use warrior_core::rate_limit::{retry_after, window_start};

proptest! {
    #[test]
    fn the_window_start_is_a_multiple_of_the_window(now in -1_000_000_000_000i64..=1_000_000_000_000, window in 1i64..=86_400) {
        let start = window_start(now, window);
        prop_assert_eq!(start.rem_euclid(window), 0);
        prop_assert!(start <= now);
        prop_assert!(now < start + window);
    }

    #[test]
    fn times_in_the_same_window_share_the_start(now in -1_000_000_000_000i64..=1_000_000_000_000, window in 1i64..=86_400) {
        let start = window_start(now, window);
        let later = start + window / 2;
        prop_assert_eq!(window_start(later, window), start);
    }

    #[test]
    fn retry_after_is_within_one_window(now in -1_000_000_000_000i64..=1_000_000_000_000, window in 1i64..=86_400) {
        let retry = retry_after(now, window);
        prop_assert!(retry >= 1);
        prop_assert!(retry <= window);
    }
}
