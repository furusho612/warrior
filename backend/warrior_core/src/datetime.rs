//! ISO 8601 の UTC の文字列と、日本時間の日付の変換。
//!
//! 時刻は `YYYY-MM-DDTHH:MM:SSZ` の形で保存する (ADR-015)。
//! 日付の境界は日本時間 (UTC+9) で決める (ADR-005)。

use std::cmp::Ordering;

/// 日本時間と UTC の差 (秒)。
pub const JAPAN_OFFSET_SECONDS: i64 = 9 * 60 * 60;

pub(crate) const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// UTC の時刻を epoch 秒から `YYYY-MM-DDTHH:MM:SSZ` の文字列にする。
pub fn format_utc(epoch_seconds: i64) -> String {
    let parts = utc_parts(epoch_seconds);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second
    )
}

/// `YYYY-MM-DDTHH:MM:SSZ` の文字列を epoch 秒にする。
pub fn parse_utc(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() != 20 {
        return None;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }

    let year = parse_digits(&text[0..4])?;
    let month = parse_digits(&text[5..7])?;
    let day = parse_digits(&text[8..10])?;
    let hour = parse_digits(&text[11..13])?;
    let minute = parse_digits(&text[14..16])?;
    let second = parse_digits(&text[17..19])?;

    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let days = days_from_civil(i64::from(year), month, day)?;
    Some(
        days * SECONDS_PER_DAY
            + i64::from(hour) * 3600
            + i64::from(minute) * 60
            + i64::from(second),
    )
}

/// 2 つの UTC の文字列を時刻として比較する。
pub fn compare_utc(left: &str, right: &str) -> Option<Ordering> {
    Some(parse_utc(left)?.cmp(&parse_utc(right)?))
}

/// UTC の時刻を日本時間の `YYYY-MM-DD` にする。
pub fn japan_date(epoch_seconds: i64) -> String {
    let parts = utc_parts(epoch_seconds + JAPAN_OFFSET_SECONDS);
    format!("{:04}-{:02}-{:02}", parts.year, parts.month, parts.day)
}

/// UTC の文字列を日本時間の `YYYY-MM-DD` にする。
pub fn japan_date_from_utc(text: &str) -> Option<String> {
    Some(japan_date(parse_utc(text)?))
}

struct UtcParts {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

fn utc_parts(epoch_seconds: i64) -> UtcParts {
    let days = epoch_seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = epoch_seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);

    UtcParts {
        year,
        month,
        day,
        hour: (seconds_of_day / 3600) as u32,
        minute: ((seconds_of_day % 3600) / 60) as u32,
        second: (seconds_of_day % 60) as u32,
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

pub(crate) fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub(crate) fn days_in_month(year: i64, month: u32) -> Option<u32> {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    };
    Some(days)
}

pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    let last_day = days_in_month(year, month)?;
    if day == 0 || day > last_day {
        return None;
    }

    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = shifted_year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * i64::from(month_index) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{compare_utc, format_utc, japan_date, japan_date_from_utc, parse_utc};

    #[test]
    fn formats_unix_epoch() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn parses_unix_epoch() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn round_trips_fixed_timestamps() {
        for text in [
            "2026-11-12T00:00:00Z",
            "2024-02-29T23:59:59Z",
            "2026-12-31T14:59:59Z",
            "2027-01-01T00:00:00Z",
        ] {
            let seconds = parse_utc(text).unwrap();
            assert_eq!(format_utc(seconds), text);
        }
    }

    #[test]
    fn rejects_malformed_timestamps() {
        for text in [
            "2026-11-12T00:00:00",
            "2026-11-12 00:00:00Z",
            "2026-11-12T00:00:60Z",
            "2026-11-12T24:00:00Z",
            "2026-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-01-00T00:00:00Z",
            "2026-01-01T00:00:00Z ",
            "abcd-01-01T00:00:00Z",
        ] {
            assert_eq!(parse_utc(text), None, "{text}");
        }
    }

    #[test]
    fn japan_date_switches_at_utc_fifteen() {
        assert_eq!(
            japan_date_from_utc("2026-12-31T14:59:59Z").as_deref(),
            Some("2026-12-31")
        );
        assert_eq!(
            japan_date_from_utc("2026-12-31T15:00:00Z").as_deref(),
            Some("2027-01-01")
        );
    }

    #[test]
    fn japan_date_handles_leap_year_and_month_end() {
        assert_eq!(
            japan_date_from_utc("2024-02-29T15:00:00Z").as_deref(),
            Some("2024-03-01")
        );
        assert_eq!(
            japan_date_from_utc("2026-01-31T14:59:59Z").as_deref(),
            Some("2026-01-31")
        );
        assert_eq!(
            japan_date_from_utc("2026-01-31T15:00:00Z").as_deref(),
            Some("2026-02-01")
        );
    }

    #[test]
    fn japan_date_offsets_epoch() {
        assert_eq!(japan_date(0), "1970-01-01");
    }

    #[test]
    fn compares_timestamps_by_instant() {
        assert_eq!(
            compare_utc("2026-01-01T00:00:00Z", "2026-01-01T00:00:01Z"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_utc("2026-01-01T00:00:01Z", "2026-01-01T00:00:01Z"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_utc("2026-01-01T00:00:02Z", "2026-01-01T00:00:01Z"),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn rejects_comparison_with_malformed_timestamp() {
        assert_eq!(compare_utc("2026-01-01T00:00:00Z", "invalid"), None);
    }
}
