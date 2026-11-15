//! レストタイマーの残り時間と、延長と短縮の性質テスト。
//!
//! 対応する要求: `REQ-TMR-003`、`REQ-TMR-006`。
//! 残り時間が 0 以上で、終了予定時刻を過ぎた場合は 0 になること、
//! 延長と短縮が終了予定時刻だけを変え、残り時間の計算が同じ規則であることを確かめる。

use proptest::prelude::*;
use warrior_core::rest_timer::RestTimer;

/// 延長と短縮で i64 の範囲を超えない終了予定時刻。
fn bounded_end_ms() -> impl Strategy<Value = i64> {
    -4_000_000_000_000_000_000i64..=4_000_000_000_000_000_000i64
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn req_tmr_006_remaining_seconds_are_non_negative_and_zero_after_the_end(
        end_ms in any::<i64>(),
        now_ms in any::<i64>(),
    ) {
        let timer = RestTimer::from_end_ms(end_ms);
        let remaining = timer.remaining_seconds(now_ms);
        if now_ms >= end_ms {
            prop_assert_eq!(remaining, 0);
        } else {
            prop_assert!(remaining >= 1);
            let difference = i128::from(end_ms) - i128::from(now_ms);
            prop_assert!(i128::from(remaining) * 1_000 >= difference);
            prop_assert!(i128::from(remaining) * 1_000 - 999 <= difference);
        }
    }

    #[test]
    fn req_tmr_003_extension_moves_the_end_time_later(
        end_ms in any::<i64>(),
        seconds in 0u32..=3_600,
        now_ms in any::<i64>(),
    ) {
        let timer = RestTimer::from_end_ms(end_ms);
        let extended = timer.extended(seconds);
        let shift = i128::from(seconds) * 1_000;
        let expected = (i128::from(end_ms) + shift)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        prop_assert_eq!(i128::from(extended.end_ms()), expected);
        prop_assert_eq!(extended.remaining_seconds(now_ms), RestTimer::from_end_ms(extended.end_ms()).remaining_seconds(now_ms));
    }

    #[test]
    fn req_tmr_003_shortening_moves_the_end_time_earlier(
        end_ms in any::<i64>(),
        seconds in 0u32..=3_600,
        now_ms in any::<i64>(),
    ) {
        let timer = RestTimer::from_end_ms(end_ms);
        let shortened = timer.shortened(seconds);
        let shift = i128::from(seconds) * 1_000;
        let expected = (i128::from(end_ms) - shift)
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        prop_assert_eq!(i128::from(shortened.end_ms()), expected);
        prop_assert_eq!(shortened.remaining_seconds(now_ms), RestTimer::from_end_ms(shortened.end_ms()).remaining_seconds(now_ms));
    }

    #[test]
    fn req_tmr_003_extension_and_shortening_keep_the_same_remaining_rule(
        end_ms in bounded_end_ms(),
        seconds in 0u32..=3_600,
        now_ms in any::<i64>(),
    ) {
        let timer = RestTimer::from_end_ms(end_ms);
        let shifted = timer.extended(seconds).shortened(seconds);
        prop_assert_eq!(shifted.end_ms(), timer.end_ms());
        prop_assert_eq!(shifted.remaining_seconds(now_ms), timer.remaining_seconds(now_ms));
    }
}
