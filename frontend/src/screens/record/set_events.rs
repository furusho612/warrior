//! セットの完了のイベント。
//!
//! セットを完了したときに 1 回だけ発行する。レストタイマーはこのイベントを受け取る。
//! このモジュールは型と発火だけを置く。

use dioxus::prelude::*;

/// セットを完了したイベント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCompleted {
    /// 対象のトレーニングの ID。
    pub training_id: String,
    /// 完了したセットの種目の行の ID。
    pub training_exercise_id: String,
    /// 完了したセットの ID。
    pub set_id: String,
    /// この完了でレストタイマーを起動するか。
    pub starts_rest_timer: bool,
}

/// セットの完了のイベントを溜める signal。
#[derive(Clone, Copy, PartialEq)]
pub struct SetCompletedEvents(pub Signal<Vec<SetCompleted>>);

impl SetCompletedEvents {
    /// イベントを 1 回発行する。
    pub fn publish(mut self, event: SetCompleted) {
        self.0.write().push(event);
    }

    /// 発行したイベントの件数。
    pub fn count(self) -> usize {
        self.0.read().len()
    }

    /// レストタイマーを起動するイベントの件数。
    pub fn rest_timer_count(self) -> usize {
        self.0
            .read()
            .iter()
            .filter(|event| event.starts_rest_timer)
            .count()
    }
}
