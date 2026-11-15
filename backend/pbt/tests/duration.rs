//! 時間の解析、整形、経過時間の性質テスト。
//!
//! 対応する要求: `REQ-SET-010`、`REQ-NFR-019`。
//! `mm:ss` と秒の解析と整形が往復し、経過時間が差分の範囲、単調性、
//! 秒単位の差分から一意に決まることを確かめる。

use proptest::prelude::*;
use warrior_core::duration::{elapsed_seconds, format, parse};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn req_set_010_format_and_parse_round_trip(seconds in 1u32..=86_400) {
        prop_assert_eq!(parse(&format(seconds)), Some(seconds));
        prop_assert_eq!(parse(&seconds.to_string()), Some(seconds));
    }

    #[test]
    fn req_set_010_minutes_and_seconds_parse_to_the_sum(
        minutes in 0u32..=1_440,
        seconds in 0u32..=59,
    ) {
        let total = minutes * 60 + seconds;
        let expected = (1..=86_400).contains(&total).then_some(total);
        prop_assert_eq!(parse(&format!("{minutes:02}:{seconds:02}")), expected);
    }

    #[test]
    fn req_set_010_elapsed_seconds_are_the_truncated_difference(
        started_at_ms in any::<i64>(),
        now_ms in any::<i64>(),
    ) {
        let elapsed = elapsed_seconds(started_at_ms, now_ms);
        if now_ms <= started_at_ms {
            prop_assert_eq!(elapsed, 0);
        } else {
            let difference = i128::from(now_ms) - i128::from(started_at_ms);
            let elapsed = i128::from(elapsed);
            prop_assert!(elapsed * 1_000 <= difference);
            prop_assert!(difference < (elapsed + 1) * 1_000);
        }
    }

    #[test]
    fn req_set_010_elapsed_seconds_are_zero_before_the_start(
        started_at_ms in any::<i64>(),
        delta_ms in 0u64..=1_000_000,
    ) {
        let now_ms = started_at_ms.saturating_sub(i64::try_from(delta_ms).unwrap());
        prop_assert_eq!(elapsed_seconds(started_at_ms, now_ms), 0);
    }

    #[test]
    fn req_set_010_elapsed_seconds_match_whole_second_offsets(
        started_at_ms in any::<i64>(),
        seconds in 0u64..=86_400,
    ) {
        let offset_ms = i64::try_from(seconds).unwrap() * 1_000;
        let Some(now_ms) = started_at_ms.checked_add(offset_ms) else {
            return Ok(());
        };
        prop_assert_eq!(elapsed_seconds(started_at_ms, now_ms), seconds);
    }

    #[test]
    fn req_set_010_elapsed_seconds_are_monotonic(
        started_at_ms in any::<i64>(),
        first_seconds in 0u64..=86_400,
        second_seconds in 0u64..=86_400,
    ) {
        let first_offset = i64::try_from(first_seconds).unwrap() * 1_000;
        let second_offset = i64::try_from(second_seconds).unwrap() * 1_000;
        let (Some(first_now), Some(second_now)) = (
            started_at_ms.checked_add(first_offset),
            started_at_ms.checked_add(second_offset),
        ) else {
            return Ok(());
        };
        let (first_now, second_now) = if first_now <= second_now {
            (first_now, second_now)
        } else {
            (second_now, first_now)
        };
        prop_assert!(
            elapsed_seconds(started_at_ms, first_now)
                <= elapsed_seconds(started_at_ms, second_now)
        );
    }
}
