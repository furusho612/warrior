//! 記録中のレストタイマーの状態 (REQ-TMR-002、REQ-TMR-003、REQ-TMR-006)。
//!
//! 記録中のトレーニングが持つタイマーは 1 つだけである。種目を切り替えても状態は
//! 画面の外に置いたままにする。残り時間は状態に持たず、表示のたびに終了予定時刻と
//! 現在時刻の差から求める (ADR-007)。

use dioxus::prelude::*;
use warrior_core::rest_timer::RestTimer;

use crate::rest_push::RestReservation;

/// 記録中のトレーニングと、そのレストタイマー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRestTimer {
    /// 対象のトレーニングの ID。
    pub training_id: String,
    /// 終了予定時刻を持つタイマー。
    pub timer: RestTimer,
    /// サーバーのプッシュ通知の予約。予約できない場合は None。
    pub reservation: Option<RestReservation>,
}

/// 記録画面と下部のタイマーが共有する状態。
#[derive(Clone, Copy, PartialEq)]
pub struct RestTimerState {
    /// 記録中のトレーニングのタイマー。
    pub timer: Signal<Option<ActiveRestTimer>>,
    /// 画面の消灯防止を有効にするか。
    pub wake_lock_enabled: Signal<bool>,
    /// レストの終了をブラウザ通知で知らせる設定。
    pub notification_enabled: Signal<bool>,
}

impl RestTimerState {
    /// タイマーを開始する。ほかのトレーニングのタイマーは置き換える。
    pub fn start(self, training_id: &str, timer: RestTimer) {
        self.start_with_reservation(training_id, timer, None);
    }

    /// プッシュ通知の予約と一緒にタイマーを開始する。
    pub fn start_with_reservation(
        self,
        training_id: &str,
        timer: RestTimer,
        reservation: Option<RestReservation>,
    ) {
        let mut signal = self.timer;
        signal.set(Some(ActiveRestTimer {
            training_id: training_id.to_owned(),
            timer,
            reservation,
        }));
    }

    /// タイマーを終了する。
    pub fn clear(self) {
        let mut signal = self.timer;
        signal.set(None);
    }

    /// 表示中のタイマーの終了予定時刻を変える。
    pub fn replace(self, training_id: &str, timer: RestTimer) {
        let mut signal = self.timer;
        let current = signal.peek().clone();
        if let Some(active) = current.filter(|active| active.training_id == training_id) {
            signal.set(Some(ActiveRestTimer {
                training_id: training_id.to_owned(),
                timer,
                reservation: active.reservation,
            }));
        }
    }

    /// 表示中のタイマーの予約を置き換える。
    pub fn set_reservation(self, training_id: &str, reservation: Option<RestReservation>) {
        let mut signal = self.timer;
        let current = signal.peek().clone();
        if let Some(active) = current.filter(|active| active.training_id == training_id) {
            signal.set(Some(ActiveRestTimer {
                training_id: training_id.to_owned(),
                timer: active.timer,
                reservation,
            }));
        }
    }
}

/// 現在時刻 (Unix エポックからのミリ秒)。
pub(crate) fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}
