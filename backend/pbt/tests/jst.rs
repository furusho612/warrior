//! 日本時間の日付と週、月の性質テスト。
//!
//! 対応する要求はない。ADR-005 が定める日本時間の境界の内部条件を確かめる。

use proptest::prelude::*;
use warrior_core::datetime;
use warrior_core::jst::{JstDate, UtcInstant, Weekday};

fn epoch_seconds() -> impl Strategy<Value = i64> {
    // 1900 年から 2100 年までの範囲に限る。遠い未来の日付は保存の対象ではない。
    -2_208_988_800i64..=4_102_444_800i64
}

fn jst_date() -> impl Strategy<Value = JstDate> {
    epoch_seconds().prop_map(|seconds| {
        UtcInstant::from_epoch_seconds(seconds)
            .jst_date()
            .expect("the test range is representable")
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_utc_instant_has_one_japan_date(seconds in epoch_seconds()) {
        let instant = UtcInstant::from_epoch_seconds(seconds);
        let first = instant.jst_date().expect("a representable date");
        let second = instant.jst_date().expect("a representable date");
        prop_assert_eq!(first, second);
        prop_assert_eq!(
            datetime::japan_date_from_utc(&instant.to_iso8601()),
            Some(first.to_string())
        );
    }

    #[test]
    fn japan_dates_are_monotonic_in_utc_time(first in epoch_seconds(), second in epoch_seconds()) {
        let (earlier, later) = if first <= second { (first, second) } else { (second, first) };
        let earlier_date = UtcInstant::from_epoch_seconds(earlier).jst_date().unwrap();
        let later_date = UtcInstant::from_epoch_seconds(later).jst_date().unwrap();
        prop_assert!(earlier_date <= later_date);
    }

    #[test]
    fn japan_dates_round_trip(date in jst_date()) {
        prop_assert_eq!(JstDate::parse(&date.to_string()), Some(date));
        prop_assert_eq!(UtcInstant::parse(&date.start_utc().to_iso8601()).unwrap().jst_date(), Some(date));
    }

    #[test]
    fn weeks_start_on_monday_and_contain_the_date(date in jst_date()) {
        let week = date.week();
        prop_assert_eq!(week.start().weekday(), Weekday::Monday);
        prop_assert_eq!(week.end().weekday(), Weekday::Sunday);
        prop_assert!(week.contains(date));
        prop_assert_eq!(week.start().add_days(6), Some(week.end()));
        prop_assert_eq!(week.start().week().start(), week.start());
        prop_assert_eq!(week.end().week().start(), week.start());
    }

    #[test]
    fn week_boundaries_cover_the_date_range(date in jst_date()) {
        let week = date.week();
        let start = week.start_utc();
        let end = week.end_utc_exclusive();
        let instant = date.start_utc();
        prop_assert!(start <= instant);
        prop_assert!(instant < end);
    }

    #[test]
    fn months_start_on_the_first_day_and_end_before_the_next_month(date in jst_date()) {
        let month = date.month_range();
        prop_assert_eq!(month.start().day(), 1);
        prop_assert!(month.contains(date));
        let next = month.end().add_days(1).expect("the next month is representable");
        prop_assert_ne!(next.month(), month.start().month());
        prop_assert_eq!(next.day(), 1);
        prop_assert!(month.start_utc() <= date.start_utc());
        prop_assert!(date.start_utc() < month.end_utc_exclusive());
    }

    #[test]
    fn month_end_handles_leap_years(year in 1900i64..=2100i64) {
        let february = JstDate::new(year, 2, 1).unwrap();
        let expected = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            29
        } else {
            28
        };
        let end = february.month_range().end();
        prop_assert_eq!(end.day(), expected);
    }
}
