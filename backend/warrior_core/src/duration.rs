//! 時間で記録するセットの時間の解析、整形、経過時間 (REQ-SET-010、REQ-NFR-019)。
//!
//! `mm:ss` と秒の表記を秒の整数に変換し、画面の表示は `mm:ss` にそろえる。
//! 受け付けるのは 1 秒から 86400 秒までである。ストップウォッチの経過時間は、
//! 保存した開始時刻と現在時刻の差から求める。

/// 受け付ける時間の下限 (秒)。
pub const MIN_SECONDS: u32 = 1;

/// 受け付ける時間の上限 (秒)。1 日を表す。
pub const MAX_SECONDS: u32 = 86_400;

/// 時間 (秒) が受け付ける範囲にあるか。
pub fn is_valid(seconds: u32) -> bool {
    (MIN_SECONDS..=MAX_SECONDS).contains(&seconds)
}

/// `mm:ss` または秒の表記を秒の整数に変換する。範囲外と解釈できない入力は None。
pub fn parse(text: &str) -> Option<u32> {
    let text = text.trim();
    let seconds = match text.split_once(':') {
        Some((minutes, seconds)) => {
            if seconds.contains(':') {
                return None;
            }
            let minutes = minutes.parse::<u32>().ok()?;
            let seconds = seconds.parse::<u32>().ok()?;
            if seconds >= 60 {
                return None;
            }
            minutes.checked_mul(60)?.checked_add(seconds)?
        }
        None => text.parse::<u32>().ok()?,
    };
    is_valid(seconds).then_some(seconds)
}

/// 時間 (秒) を `mm:ss` の表記にする。
///
/// 分は 2 桁以上で表し、時間が 1 時間を超えても分を繰り上げる。
pub fn format(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

/// 開始時刻と現在時刻 (ミリ秒) から経過した秒数を返す。
///
/// 現在時刻が開始時刻より前のときは 0 を返す。端末の時計を変更すると結果が変わる。
pub fn elapsed_seconds(started_at_ms: i64, now_ms: i64) -> u64 {
    if now_ms <= started_at_ms {
        return 0;
    }
    let elapsed_ms = i128::from(now_ms) - i128::from(started_at_ms);
    u64::try_from(elapsed_ms / 1_000).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-SET-010`、`REQ-NFR-019`。

    use super::{elapsed_seconds, format, is_valid, parse};

    #[test]
    fn req_set_010_parses_the_example_seconds() {
        assert_eq!(parse("1"), Some(1));
        assert_eq!(parse("59"), Some(59));
        assert_eq!(parse("60"), Some(60));
        assert_eq!(parse("3599"), Some(3_599));
        assert_eq!(parse("3600"), Some(3_600));
        assert_eq!(parse("86400"), Some(86_400));
    }

    #[test]
    fn req_set_010_parses_the_example_minutes_and_seconds() {
        assert_eq!(parse("00:01"), Some(1));
        assert_eq!(parse("00:59"), Some(59));
        assert_eq!(parse("01:00"), Some(60));
        assert_eq!(parse("59:59"), Some(3_599));
        assert_eq!(parse("60:00"), Some(3_600));
        assert_eq!(parse("1440:00"), Some(86_400));
    }

    #[test]
    fn req_set_010_formats_the_example_seconds() {
        assert_eq!(format(0), "00:00");
        assert_eq!(format(1), "00:01");
        assert_eq!(format(59), "00:59");
        assert_eq!(format(60), "01:00");
        assert_eq!(format(3_599), "59:59");
        assert_eq!(format(3_600), "60:00");
        assert_eq!(format(86_400), "1440:00");
    }

    #[test]
    fn req_nfr_019_rejects_out_of_range_and_unreadable_values() {
        for text in [
            "", " ", "0", "00:00", "86401", "1440:01", "00:60", "01:60", "-1", "1.5", "1:2:3",
            "abc", ":30", "30:",
        ] {
            assert_eq!(parse(text), None, "{text:?} must be rejected");
        }
        assert!(!is_valid(0));
        assert!(is_valid(1));
        assert!(is_valid(86_400));
        assert!(!is_valid(86_401));
    }

    #[test]
    fn elapsed_seconds_truncates_to_whole_seconds() {
        assert_eq!(elapsed_seconds(1_000, 1_000), 0);
        assert_eq!(elapsed_seconds(1_000, 1_999), 0);
        assert_eq!(elapsed_seconds(1_000, 2_000), 1);
        assert_eq!(elapsed_seconds(1_000, 3_500), 2);
        assert_eq!(elapsed_seconds(2_000, 1_000), 0);
    }

    #[test]
    fn elapsed_seconds_handles_the_i64_limits() {
        assert_eq!(elapsed_seconds(i64::MIN, i64::MAX), 18_446_744_073_709_551);
        assert_eq!(elapsed_seconds(i64::MAX, i64::MIN), 0);
        assert_eq!(elapsed_seconds(0, 999), 0);
        assert_eq!(elapsed_seconds(0, 1_000), 1);
    }
}
