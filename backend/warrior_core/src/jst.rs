//! 日本時間の日付と、週と月の境界。
//!
//! 日本時間は UTC に 9 時間を足した時刻とし、夏時間を扱わない (ADR-005)。
//! 週は月曜 0 時に始まり、月は 1 日 0 時に始まる。

use std::fmt;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::datetime::{self, SECONDS_PER_DAY};

pub use crate::datetime::JAPAN_OFFSET_SECONDS;

/// UTC の時刻。秒とナノ秒を持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcInstant {
    seconds: i64,
    nanos: u32,
}

impl UtcInstant {
    /// epoch 秒から作る。ナノ秒は 0 にする。
    pub fn from_epoch_seconds(seconds: i64) -> Self {
        Self { seconds, nanos: 0 }
    }

    /// 秒とナノ秒から作る。ナノ秒は 0 以上 10 億未満に限る。
    pub fn new(seconds: i64, nanos: u32) -> Option<Self> {
        if nanos >= 1_000_000_000 {
            return None;
        }
        Some(Self { seconds, nanos })
    }

    /// ISO 8601 の UTC の文字列から作る。
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(seconds) = datetime::parse_utc(text) {
            return Some(Self::from_epoch_seconds(seconds));
        }
        parse_fractional(text)
    }

    /// ISO 8601 の UTC の文字列にする。
    pub fn to_iso8601(self) -> String {
        let base = datetime::format_utc(self.seconds);
        if self.nanos == 0 {
            base
        } else {
            format!("{}.{:09}Z", &base[..19], self.nanos)
        }
    }

    /// epoch 秒。
    pub fn seconds(self) -> i64 {
        self.seconds
    }

    /// ナノ秒。
    pub fn nanos(self) -> u32 {
        self.nanos
    }

    /// 日本時間の日付にする。
    pub fn jst_date(self) -> Option<JstDate> {
        let shifted = self.seconds.checked_add(JAPAN_OFFSET_SECONDS)?;
        let days = shifted.div_euclid(SECONDS_PER_DAY);
        let (year, month, day) = datetime::civil_from_days(days);
        JstDate::new(year, month, day)
    }
}

fn parse_fractional(text: &str) -> Option<UtcInstant> {
    let bytes = text.as_bytes();
    if bytes.len() < 22 || bytes[19] != b'.' || bytes[bytes.len() - 1] != b'Z' {
        return None;
    }
    let base = format!("{}Z", &text[..19]);
    let seconds = datetime::parse_utc(&base)?;
    let fraction = &text[20..bytes.len() - 1];
    if fraction.is_empty()
        || fraction.len() > 9
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let mut nanos = 0u32;
    for character in fraction.bytes() {
        nanos = nanos * 10 + u32::from(character - b'0');
    }
    for _ in fraction.len()..9 {
        nanos *= 10;
    }
    UtcInstant::new(seconds, nanos)
}

/// 日本時間の日付の曜日。月曜を週の始まりとする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weekday {
    /// 月曜。
    Monday,
    /// 火曜。
    Tuesday,
    /// 水曜。
    Wednesday,
    /// 木曜。
    Thursday,
    /// 金曜。
    Friday,
    /// 土曜。
    Saturday,
    /// 日曜。
    Sunday,
}

impl Weekday {
    /// 月曜を 0 とする曜日の番号。
    pub fn index(self) -> u8 {
        match self {
            Weekday::Monday => 0,
            Weekday::Tuesday => 1,
            Weekday::Wednesday => 2,
            Weekday::Thursday => 3,
            Weekday::Friday => 4,
            Weekday::Saturday => 5,
            Weekday::Sunday => 6,
        }
    }

    fn from_epoch_days(days: i64) -> Self {
        match (days + 3).rem_euclid(7) {
            0 => Weekday::Monday,
            1 => Weekday::Tuesday,
            2 => Weekday::Wednesday,
            3 => Weekday::Thursday,
            4 => Weekday::Friday,
            5 => Weekday::Saturday,
            _ => Weekday::Sunday,
        }
    }
}

/// 日本時間の日付。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JstDate {
    year: i32,
    month: u8,
    day: u8,
}

impl JstDate {
    /// 年、月、日から作る。存在しない日付は受け付けない。
    pub fn new(year: i64, month: u32, day: u32) -> Option<Self> {
        if !(0..=9999).contains(&year) {
            return None;
        }
        datetime::days_from_civil(year, month, day)?;
        Some(Self {
            year: year as i32,
            month: month as u8,
            day: day as u8,
        })
    }

    /// `YYYY-MM-DD` から作る。
    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let year = parse_digits(&text[0..4])?;
        let month = parse_digits(&text[5..7])?;
        let day = parse_digits(&text[8..10])?;
        Self::new(i64::from(year), month, day)
    }

    /// 年。
    pub fn year(self) -> i32 {
        self.year
    }

    /// 月。
    pub fn month(self) -> u8 {
        self.month
    }

    /// 日。
    pub fn day(self) -> u8 {
        self.day
    }

    /// 曜日。
    pub fn weekday(self) -> Weekday {
        let days = datetime::days_from_civil(
            i64::from(self.year),
            u32::from(self.month),
            u32::from(self.day),
        )
        .expect("a validated date has epoch days");
        Weekday::from_epoch_days(days)
    }

    /// 日付を進める。存在しない日付になる場合は None。
    pub fn add_days(self, days: i64) -> Option<Self> {
        let current = datetime::days_from_civil(
            i64::from(self.year),
            u32::from(self.month),
            u32::from(self.day),
        )?;
        let (year, month, day) = datetime::civil_from_days(current.checked_add(days)?);
        Self::new(year, month, day)
    }

    /// 月を引いた日。引いた先の月に同じ日が無い場合は、その月の末日にする。
    ///
    /// 表現できる範囲の外に出るときは None。
    pub fn subtract_months(self, months: u32) -> Option<Self> {
        let total = i64::from(self.month) - 1 - i64::from(months);
        let year = i64::from(self.year) + total.div_euclid(12);
        let month = u32::try_from(total.rem_euclid(12) + 1).ok()?;
        let last_day = datetime::days_in_month(year, month)?;
        Self::new(year, month, u32::from(self.day).min(last_day))
    }

    /// その日を含む週。月曜 0 時に始まり、日曜 23:59:59 で終わる。
    pub fn week(self) -> JstWeek {
        let days = datetime::days_from_civil(
            i64::from(self.year),
            u32::from(self.month),
            u32::from(self.day),
        )
        .expect("a validated date has epoch days");
        // 1970-01-01 は木曜である。月曜を 0 とする。
        let weekday = (days + 3).rem_euclid(7);
        let start = self
            .add_days(-weekday)
            .expect("a week starts in the representable range");
        let end = start
            .add_days(6)
            .expect("a week ends in the representable range");
        JstWeek { start, end }
    }

    /// その日を含む月。
    pub fn month_range(self) -> JstMonth {
        let start = Self::new(i64::from(self.year), u32::from(self.month), 1)
            .expect("the first day of a validated month");
        let last_day = datetime::days_in_month(i64::from(self.year), u32::from(self.month))
            .expect("a validated month");
        let end = Self::new(i64::from(self.year), u32::from(self.month), last_day)
            .expect("the last day of a validated month");
        JstMonth { start, end }
    }

    /// この日の日本時間 0 時の UTC の時刻。
    pub fn start_utc(self) -> UtcInstant {
        let days = datetime::days_from_civil(
            i64::from(self.year),
            u32::from(self.month),
            u32::from(self.day),
        )
        .expect("a validated date has epoch days");
        UtcInstant::from_epoch_seconds(days * SECONDS_PER_DAY - JAPAN_OFFSET_SECONDS)
    }
}

impl Serialize for JstDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for JstDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        JstDate::parse(&text)
            .ok_or_else(|| de::Error::custom(format!("invalid Japan date: {text}")))
    }
}

impl fmt::Display for JstDate {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

impl FromStr for JstDate {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        JstDate::parse(text).ok_or(())
    }
}

/// 月曜 0 時に始まる週。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JstWeek {
    start: JstDate,
    end: JstDate,
}

impl JstWeek {
    /// 週の開始日 (月曜)。
    pub fn start(self) -> JstDate {
        self.start
    }

    /// 週の終了日 (日曜)。
    pub fn end(self) -> JstDate {
        self.end
    }

    /// 週の開始の UTC の時刻。
    pub fn start_utc(self) -> UtcInstant {
        self.start.start_utc()
    }

    /// 週の終了の次の瞬間 (次の月曜 0 時) の UTC の時刻。
    pub fn end_utc_exclusive(self) -> UtcInstant {
        self.end
            .add_days(1)
            .expect("the next day of a representable date")
            .start_utc()
    }

    /// 日付が週に含まれるか。
    pub fn contains(self, date: JstDate) -> bool {
        self.start <= date && date <= self.end
    }
}

/// 1 日 0 時に始まる月。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JstMonth {
    start: JstDate,
    end: JstDate,
}

impl JstMonth {
    /// 月の開始日。
    pub fn start(self) -> JstDate {
        self.start
    }

    /// 月の終了日。
    pub fn end(self) -> JstDate {
        self.end
    }

    /// 月の開始の UTC の時刻。
    pub fn start_utc(self) -> UtcInstant {
        self.start.start_utc()
    }

    /// 月の終了の次の瞬間 (翌月 1 日 0 時) の UTC の時刻。
    pub fn end_utc_exclusive(self) -> UtcInstant {
        self.end
            .add_days(1)
            .expect("the next day of a representable date")
            .start_utc()
    }

    /// 日付が月に含まれるか。
    pub fn contains(self, date: JstDate) -> bool {
        self.start <= date && date <= self.end
    }
}

fn parse_digits(text: &str) -> Option<u32> {
    let mut value = 0u32;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u32::from(byte - b'0');
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::{JstDate, UtcInstant};

    #[test]
    fn converts_utc_to_japan_date() {
        let instant = UtcInstant::parse("2026-11-11T15:00:00Z").unwrap();
        assert_eq!(instant.jst_date(), JstDate::new(2026, 11, 12));
    }

    #[test]
    fn japan_date_switches_at_utc_fifteen() {
        assert_eq!(
            UtcInstant::parse("2026-12-31T14:59:59Z")
                .unwrap()
                .jst_date(),
            JstDate::new(2026, 12, 31)
        );
        assert_eq!(
            UtcInstant::parse("2026-12-31T15:00:00Z")
                .unwrap()
                .jst_date(),
            JstDate::new(2027, 1, 1)
        );
    }

    #[test]
    fn formats_and_parses_japan_dates() {
        let date = JstDate::new(2026, 11, 12).unwrap();
        assert_eq!(date.to_string(), "2026-11-12");
        assert_eq!(JstDate::parse("2026-11-12"), Some(date));
    }

    #[test]
    fn rejects_invalid_japan_dates() {
        for text in [
            "2026-02-29",
            "2023-02-29",
            "2026-13-01",
            "2026-00-01",
            "2026-01-00",
            "2026-01-32",
            "2026-1-01",
            "2026-01-1",
            "2026/01/01",
            "2026-01-01 ",
            "abcd-01-01",
        ] {
            assert_eq!(JstDate::parse(text), None, "{text}");
        }
    }

    #[test]
    fn leap_day_is_valid_in_a_leap_year() {
        assert_eq!(JstDate::parse("2024-02-29"), JstDate::new(2024, 2, 29));
    }

    #[test]
    fn subtract_months_keeps_the_day_or_moves_to_the_month_end() {
        let march_first = JstDate::new(2026, 3, 1).unwrap();
        assert_eq!(march_first.subtract_months(1), JstDate::new(2026, 2, 1));
        let march_thirty_first = JstDate::new(2026, 3, 31).unwrap();
        assert_eq!(
            march_thirty_first.subtract_months(1),
            JstDate::new(2026, 2, 28)
        );
        let leap_march_thirty_first = JstDate::new(2024, 3, 31).unwrap();
        assert_eq!(
            leap_march_thirty_first.subtract_months(1),
            JstDate::new(2024, 2, 29)
        );
        assert_eq!(
            JstDate::new(2026, 11, 12).unwrap().subtract_months(12),
            JstDate::new(2025, 11, 12)
        );
        assert_eq!(
            JstDate::new(2026, 1, 31).unwrap().subtract_months(2),
            JstDate::new(2025, 11, 30)
        );
        assert_eq!(
            JstDate::new(0, 1, 1).unwrap().subtract_months(1),
            None,
            "a date before year zero is not representable"
        );
    }

    #[test]
    fn week_starts_on_monday() {
        let sunday = JstDate::new(2026, 11, 8).unwrap();
        let week = sunday.week();
        assert_eq!(week.start(), JstDate::new(2026, 11, 2).unwrap());
        assert_eq!(week.end(), sunday);
        assert!(week.contains(sunday));

        let monday = JstDate::new(2026, 11, 9).unwrap();
        let next_week = monday.week();
        assert_eq!(next_week.start(), monday);
        assert_eq!(next_week.end(), JstDate::new(2026, 11, 15).unwrap());
    }

    #[test]
    fn week_switches_at_monday_midnight_in_japan() {
        let sunday_last = UtcInstant::parse("2026-11-08T14:59:59Z").unwrap();
        let monday_first = UtcInstant::parse("2026-11-08T15:00:00Z").unwrap();
        let sunday_week = sunday_last.jst_date().unwrap().week();
        let monday_week = monday_first.jst_date().unwrap().week();

        assert_eq!(
            sunday_week.start_utc(),
            UtcInstant::parse("2026-11-01T15:00:00Z").unwrap()
        );
        assert_eq!(
            sunday_week.end_utc_exclusive(),
            UtcInstant::parse("2026-11-08T15:00:00Z").unwrap()
        );
        assert_eq!(monday_week.start_utc(), sunday_week.end_utc_exclusive());
    }

    #[test]
    fn month_handles_month_end_and_leap_year() {
        let january = JstDate::new(2026, 1, 31).unwrap().month_range();
        assert_eq!(january.start(), JstDate::new(2026, 1, 1).unwrap());
        assert_eq!(january.end(), JstDate::new(2026, 1, 31).unwrap());
        assert_eq!(
            january.end_utc_exclusive(),
            UtcInstant::parse("2026-01-31T15:00:00Z").unwrap()
        );

        let february = JstDate::new(2024, 2, 10).unwrap().month_range();
        assert_eq!(february.end(), JstDate::new(2024, 2, 29).unwrap());
        assert_eq!(
            february.end_utc_exclusive(),
            UtcInstant::parse("2024-02-29T15:00:00Z").unwrap()
        );

        let december = JstDate::new(2026, 12, 1).unwrap().month_range();
        assert_eq!(december.end(), JstDate::new(2026, 12, 31).unwrap());
        assert_eq!(
            december.end_utc_exclusive(),
            UtcInstant::parse("2026-12-31T15:00:00Z").unwrap()
        );
    }

    #[test]
    fn month_boundary_switches_at_first_day_midnight_in_japan() {
        let last_january = UtcInstant::parse("2026-01-31T14:59:59Z").unwrap();
        let first_february = UtcInstant::parse("2026-01-31T15:00:00Z").unwrap();
        assert_eq!(
            last_january.jst_date().unwrap().month_range().end(),
            JstDate::new(2026, 1, 31).unwrap()
        );
        assert_eq!(
            first_february.jst_date().unwrap().month_range().start(),
            JstDate::new(2026, 2, 1).unwrap()
        );
    }

    #[test]
    fn fractional_seconds_round_trip() {
        let instant = UtcInstant::parse("2026-11-12T00:00:00.123456789Z").unwrap();
        assert_eq!(instant.nanos(), 123_456_789);
        assert_eq!(instant.to_iso8601(), "2026-11-12T00:00:00.123456789Z");
    }
}
