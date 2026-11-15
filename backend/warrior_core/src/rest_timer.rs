//! レスト時間の選択と、レストタイマーの終了予定時刻・残り時間・延長と短縮 (REQ-TMR-001、REQ-TMR-003、REQ-TMR-006)。
//!
//! レスト時間は、全体の既定値、種目ごとの値、セットごとの値の順に優先する。
//! タイマーは残り時間を減算する状態で持たず、終了予定時刻と現在時刻の差から残り時間を求める (ADR-007)。
//! 延長と短縮は終了予定時刻だけを変える。

use serde::{Deserialize, Serialize};

use crate::duration;

/// レスト時間の既定値の初期値 (秒、REQ-NFR-019)。
pub const DEFAULT_SECONDS: u32 = 90;

/// レスト時間の下限 (秒、REQ-NFR-019)。
pub const MIN_SECONDS: u32 = 1;

/// レスト時間の上限 (秒、REQ-NFR-019)。
pub const MAX_SECONDS: u32 = 3_600;

/// 延長と短縮の 1 回の幅 (秒)。
pub const STEP_SECONDS: u32 = 15;

/// レスト時間が受け付ける範囲にあるか。
pub fn is_valid_seconds(seconds: u32) -> bool {
    (MIN_SECONDS..=MAX_SECONDS).contains(&seconds)
}

/// `mm:ss` または秒の表記をレスト時間に変換する。範囲外と解釈できない入力は None。
pub fn parse(text: &str) -> Option<u32> {
    duration::parse(text).filter(|seconds| is_valid_seconds(*seconds))
}

/// レスト時間を `mm:ss` の表記にする。
pub fn format(seconds: u32) -> String {
    duration::format(seconds)
}

/// セット、種目、全体の既定値の順に優先して、使うレスト時間を決める。
///
/// 指定された値が範囲外のときは None を返す。
pub fn resolve_seconds(
    default_seconds: u32,
    exercise_seconds: Option<u32>,
    set_seconds: Option<u32>,
) -> Option<u32> {
    if !is_valid_seconds(default_seconds)
        || exercise_seconds.is_some_and(|seconds| !is_valid_seconds(seconds))
        || set_seconds.is_some_and(|seconds| !is_valid_seconds(seconds))
    {
        return None;
    }
    Some(set_seconds.or(exercise_seconds).unwrap_or(default_seconds))
}

/// 1 つのレストタイマー。残り時間は持たず、終了予定時刻だけを持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestTimer {
    /// 終了予定時刻 (Unix エポックからのミリ秒)。
    end_ms: i64,
}

impl RestTimer {
    /// 現在時刻とレスト時間からタイマーを開始する。範囲外のレスト時間は None。
    pub fn start(now_ms: i64, seconds: u32) -> Option<Self> {
        if !is_valid_seconds(seconds) {
            return None;
        }
        let millis = i64::from(seconds).checked_mul(1_000)?;
        Some(Self {
            end_ms: now_ms.checked_add(millis)?,
        })
    }

    /// 保存した終了予定時刻からタイマーを復元する。
    pub fn from_end_ms(end_ms: i64) -> Self {
        Self { end_ms }
    }

    /// 終了予定時刻 (Unix エポックからのミリ秒)。
    pub fn end_ms(self) -> i64 {
        self.end_ms
    }

    /// 残り時間 (秒)。終了予定時刻を過ぎていれば 0 になる。
    ///
    /// 残りが 1 ミリ秒以上ある間は 1 秒以上を返す。端末の時計を戻すと残り時間が増える。
    pub fn remaining_seconds(self, now_ms: i64) -> u64 {
        if now_ms >= self.end_ms {
            return 0;
        }
        let difference = i128::from(self.end_ms) - i128::from(now_ms);
        u64::try_from((difference + 999) / 1_000).unwrap_or(u64::MAX)
    }

    /// 終了予定時刻を過ぎているか。
    pub fn is_finished(self, now_ms: i64) -> bool {
        now_ms >= self.end_ms
    }

    /// 終了予定時刻を指定した秒数だけ後ろへずらす。
    pub fn extended(self, seconds: u32) -> Self {
        self.shifted(i128::from(seconds) * 1_000)
    }

    /// 終了予定時刻を指定した秒数だけ前へずらす。
    pub fn shortened(self, seconds: u32) -> Self {
        self.shifted(-i128::from(seconds) * 1_000)
    }

    fn shifted(self, millis: i128) -> Self {
        let shifted = i128::from(self.end_ms) + millis;
        let end_ms = shifted.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        Self {
            end_ms: i64::try_from(end_ms).unwrap_or(self.end_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-TMR-001`、`REQ-TMR-003`、`REQ-TMR-006`、`REQ-NFR-019`。

    use super::{resolve_seconds, RestTimer, DEFAULT_SECONDS, MAX_SECONDS};

    #[test]
    fn req_tmr_001_prefers_the_set_then_the_exercise_then_the_default() {
        assert_eq!(resolve_seconds(90, None, None), Some(90));
        assert_eq!(resolve_seconds(90, Some(120), None), Some(120));
        assert_eq!(resolve_seconds(90, Some(120), Some(60)), Some(60));
        assert_eq!(resolve_seconds(90, None, Some(60)), Some(60));
        assert_eq!(resolve_seconds(DEFAULT_SECONDS, None, None), Some(90));
    }

    #[test]
    fn req_nfr_019_rejects_out_of_range_rest_seconds() {
        assert_eq!(resolve_seconds(0, None, None), None);
        assert_eq!(resolve_seconds(90, Some(0), None), None);
        assert_eq!(resolve_seconds(90, None, Some(86_400)), None);
        assert_eq!(resolve_seconds(90, Some(3_601), None), None);
        assert_eq!(resolve_seconds(MAX_SECONDS, None, None), Some(3_600));
        assert_eq!(resolve_seconds(90, Some(MAX_SECONDS), None), Some(3_600));
    }

    #[test]
    fn req_tmr_006_remaining_is_zero_at_the_end_and_after_it() {
        let timer = RestTimer::start(1_000_000, 90).expect("a valid timer");
        let end = timer.end_ms();
        assert_eq!(timer.remaining_seconds(end), 0);
        assert_eq!(timer.remaining_seconds(end + 1), 0);
        assert_eq!(timer.remaining_seconds(end + 60_000), 0);
        assert!(timer.is_finished(end));
        assert!(timer.is_finished(end + 1));
    }

    #[test]
    fn req_tmr_006_remaining_is_one_second_before_the_end() {
        let timer = RestTimer::start(1_000_000, 90).expect("a valid timer");
        let end = timer.end_ms();
        assert_eq!(timer.remaining_seconds(end - 1_000), 1);
        assert_eq!(timer.remaining_seconds(end - 999), 1);
        assert_eq!(timer.remaining_seconds(end - 1), 1);
        assert!(!timer.is_finished(end - 1));
    }

    #[test]
    fn req_tmr_003_extension_and_shortening_change_only_the_end_time() {
        let timer = RestTimer::start(1_000_000, 90).expect("a valid timer");
        let extended = timer.extended(15);
        assert_eq!(extended.end_ms(), timer.end_ms() + 15_000);
        assert_eq!(extended.remaining_seconds(1_000_000), 105);
        let shortened = extended.shortened(15);
        assert_eq!(shortened, timer);
        assert_eq!(
            shortened.remaining_seconds(1_000_000),
            timer.remaining_seconds(1_000_000)
        );
    }

    #[test]
    fn req_tmr_003_shortening_past_the_current_time_finishes_the_timer() {
        let timer = RestTimer::start(1_000_000, 90).expect("a valid timer");
        let shortened = timer.shortened(3_600);
        assert!(shortened.is_finished(1_000_000 + 1));
        assert_eq!(shortened.remaining_seconds(1_000_000 + 1), 0);
    }
}
