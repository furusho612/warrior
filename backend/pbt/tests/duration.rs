//! 時間の解析、整形、経過時間の性質テスト。
//!
//! 対応する要求: `REQ-SET-010`、`REQ-NFR-019`。
//! `mm:ss` と秒の解析と整形が往復し、任意の開始時刻と現在時刻から経過時間が
//! 一意に決まることを確かめる。

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
        let expected = if now_ms >= started_at_ms {
            u64::try_from((i128::from(now_ms) - i128::from(started_at_ms)) / 1_000)
                .unwrap_or(u64::MAX)
        } else {
            0
        };
        prop_assert_eq!(elapsed_seconds(started_at_ms, now_ms), expected);
    }
}
