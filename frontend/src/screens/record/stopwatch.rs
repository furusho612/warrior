//! 時間で記録するセットのストップウォッチ。開始時刻を端末内に保存し、経過時間を表示する。
//!
//! 対応する要求: `REQ-SET-010`。
//! 開始ボタンで開始時刻を保存し、停止ボタンで保存した開始時刻と現在時刻の差を
//! 時間の入力欄に入れる。再読み込みの後は、保存した開始時刻から経過時間を復元する。

use dioxus::prelude::*;
use warrior_core::duration;

use super::sets::{Field, SetAction};
use super::sleep_ms;
use crate::storage::{self, StopwatchSide};
use crate::text::{text, Key};

/// 経過時間を画面へ反映する間隔 (ミリ秒)。
const TICK_INTERVAL_MS: i32 = 500;

/// ストップウォッチの画面の状態。
#[derive(Clone, Copy)]
struct StopwatchSignals {
    started_at: Signal<Option<i64>>,
    last_elapsed: Signal<Option<u64>>,
    failed: Signal<bool>,
}

/// ストップウォッチが書き込む入力欄。
struct StopwatchTarget {
    set_id: String,
    segment_id: String,
    side: StopwatchSide,
    field: Field,
}

/// セットのストップウォッチ。
#[component]
pub(super) fn StopwatchControl(
    set_id: String,
    segment_id: String,
    side: StopwatchSide,
    field: Field,
    disabled: bool,
    on_action: EventHandler<SetAction>,
) -> Element {
    let started_at = use_signal(|| Option::<i64>::None);
    let last_elapsed = use_signal(|| Option::<u64>::None);
    let failed = use_signal(|| false);
    let signals = StopwatchSignals {
        started_at,
        last_elapsed,
        failed,
    };
    let tick = use_signal(|| 0u64);
    let load_set_id = set_id.clone();
    use_effect(move || {
        load_started_at(load_set_id.clone(), side, signals);
    });
    // 経過時間は保存した開始時刻との差で求める。tick は表示の再描画だけを促す。
    use_future(move || {
        let mut tick = tick;
        async move {
            loop {
                sleep_ms(TICK_INTERVAL_MS).await;
                if started_at.peek().is_some() {
                    let next = tick.peek().wrapping_add(1);
                    tick.set(next);
                }
            }
        }
    });
    let _ = (tick)();
    let started = (started_at)();
    let elapsed = match started {
        Some(started) => duration::elapsed_seconds(started, now_ms()),
        None => (last_elapsed)().unwrap_or(0),
    };
    let elapsed_text = format_elapsed(elapsed);
    let running = started.is_some();
    let side_name = side_name(side);

    let start_set_id = set_id.clone();
    let start = move |_| {
        if disabled {
            return;
        }
        start_stopwatch(start_set_id.clone(), side, signals);
    };
    let stop_set_id = set_id.clone();
    let stop_segment_id = segment_id.clone();
    let stop = move |_| {
        stop_stopwatch(
            StopwatchTarget {
                set_id: stop_set_id.clone(),
                segment_id: stop_segment_id.clone(),
                side,
                field,
            },
            signals,
            on_action,
        );
    };

    rsx! {
        div {
            class: "flex shrink-0 items-center gap-1",
            "data-stopwatch": "true",
            "data-stopwatch-side": "{side_name}",
            span {
                class: "w-12 text-[10px] font-bold tabular-nums",
                "data-stopwatch-elapsed": "{elapsed_text}",
                "data-stopwatch-elapsed-seconds": "{elapsed.min(u64::from(u32::MAX))}",
                "data-stopwatch-running": if running { "true" } else { "false" },
                "{elapsed_text}"
            }
            button {
                class: "min-h-8 shrink-0 border border-line px-1 text-[10px]",
                "data-stopwatch-start": "true",
                disabled: disabled || running,
                onclick: start,
                "{text(Key::RecordSetStopwatchStart)}"
            }
            button {
                class: "min-h-8 shrink-0 border border-line px-1 text-[10px]",
                "data-stopwatch-stop": "true",
                disabled: disabled || !running,
                onclick: stop,
                "{text(Key::RecordSetStopwatchStop)}"
            }
            if failed() {
                p { class: "text-[10px] text-primary", "{text(Key::RecordSetStopwatchFailed)}" }
            }
        }
    }
}

fn load_started_at(set_id: String, side: StopwatchSide, signals: StopwatchSignals) {
    let mut started_at = signals.started_at;
    let mut failed = signals.failed;
    spawn(async move {
        let Ok(storage) = storage::current().await else {
            failed.set(true);
            return;
        };
        match storage.stopwatch_started_at(&set_id, side).await {
            Ok(Some(started)) => started_at.set(Some(started)),
            Ok(None) => {}
            Err(_) => failed.set(true),
        }
    });
}

fn start_stopwatch(set_id: String, side: StopwatchSide, signals: StopwatchSignals) {
    let now = now_ms();
    let mut started_at = signals.started_at;
    let mut last_elapsed = signals.last_elapsed;
    let mut failed = signals.failed;
    spawn(async move {
        let Ok(storage) = storage::current().await else {
            failed.set(true);
            return;
        };
        if storage
            .save_stopwatch_started_at(&set_id, side, now)
            .await
            .is_err()
        {
            failed.set(true);
            return;
        }
        failed.set(false);
        started_at.set(Some(now));
        last_elapsed.set(None);
    });
}

fn stop_stopwatch(
    target: StopwatchTarget,
    signals: StopwatchSignals,
    on_action: EventHandler<SetAction>,
) {
    let Some(started) = (signals.started_at)() else {
        return;
    };
    let now = now_ms();
    let mut started_at = signals.started_at;
    let mut last_elapsed = signals.last_elapsed;
    let mut failed = signals.failed;
    spawn(async move {
        let Ok(storage) = storage::current().await else {
            failed.set(true);
            return;
        };
        if storage
            .clear_stopwatch_started_at(&target.set_id, target.side)
            .await
            .is_err()
        {
            failed.set(true);
            return;
        }
        let elapsed = duration::elapsed_seconds(started, now);
        failed.set(false);
        started_at.set(None);
        last_elapsed.set(Some(elapsed));
        if let Ok(seconds) = u32::try_from(elapsed) {
            if duration::is_valid(seconds) {
                on_action.call(SetAction::Input {
                    set_id: target.set_id,
                    segment_id: target.segment_id,
                    field: target.field,
                    value: duration::format(seconds),
                });
            }
        }
    });
}

fn format_elapsed(elapsed: u64) -> String {
    duration::format(u32::try_from(elapsed).unwrap_or(u32::MAX))
}

fn side_name(side: StopwatchSide) -> &'static str {
    match side {
        StopwatchSide::Whole => "whole",
        StopwatchSide::Left => "left",
        StopwatchSide::Right => "right",
    }
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}
