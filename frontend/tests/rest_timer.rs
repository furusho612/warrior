//! レストタイマーの端末内の保存、Wake Lock の要求と取り直し、終了の音と振動のブラウザのテスト。
//!
//! 対応する要求: `REQ-TMR-001`、`REQ-TMR-004`、`REQ-TMR-005`、`REQ-TMR-006`。
//! Wake Lock と音の API は、実機の画面消灯と音をテストから観測できないため、
//! ブラウザの API をテストの中だけ差し替え、要求と再要求、音と振動の呼び出しを数える。

use js_sys::Promise;
use warrior_core::rest_timer::{resolve_seconds, RestTimer, DEFAULT_SECONDS};
use warrior_frontend::sound;
use warrior_frontend::storage::Storage;
use warrior_frontend::wake_lock::ScreenWakeLock;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{database_name, open};

const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5801";
const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5802";
const SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5803";

#[wasm_bindgen_test]
async fn req_tmr_001_rest_seconds_survive_a_reload() {
    let name = database_name("rest-seconds");
    let storage = open(&name).await;
    assert_eq!(
        storage
            .rest_default_seconds()
            .await
            .expect("the default must be readable"),
        DEFAULT_SECONDS
    );
    storage
        .save_rest_default_seconds(Some(120))
        .await
        .expect("the default must be saved");
    storage
        .save_rest_exercise_seconds(EXERCISE_ROW_ID, Some(60))
        .await
        .expect("the exercise seconds must be saved");
    storage
        .save_rest_set_seconds(SET_ID, Some(45))
        .await
        .expect("the set seconds must be saved");
    assert!(
        storage
            .wake_lock_enabled()
            .await
            .expect("the wake lock setting must be readable"),
        "the wake lock must be enabled by default"
    );
    storage
        .save_wake_lock_enabled(false)
        .await
        .expect("the wake lock setting must be saved");
    drop(storage);

    let reopened: Storage = open(&name).await;
    assert_eq!(
        reopened
            .rest_default_seconds()
            .await
            .expect("the default must be readable"),
        120
    );
    assert_eq!(
        reopened
            .rest_exercise_seconds(EXERCISE_ROW_ID)
            .await
            .expect("the exercise seconds must be readable"),
        Some(60)
    );
    assert_eq!(
        reopened
            .rest_set_seconds(SET_ID)
            .await
            .expect("the set seconds must be readable"),
        Some(45)
    );
    assert!(
        !reopened
            .wake_lock_enabled()
            .await
            .expect("the wake lock setting must be readable"),
        "disabling the wake lock must survive a reload"
    );
    assert_eq!(
        resolve_seconds(120, Some(60), Some(45),),
        Some(45),
        "the set seconds must take precedence over the exercise seconds"
    );

    reopened
        .save_rest_set_seconds(SET_ID, None)
        .await
        .expect("the set seconds must be cleared");
    assert_eq!(
        resolve_seconds(
            120,
            reopened
                .rest_exercise_seconds(EXERCISE_ROW_ID)
                .await
                .expect("the exercise seconds must be readable"),
            reopened
                .rest_set_seconds(SET_ID)
                .await
                .expect("the set seconds must be readable"),
        ),
        Some(60),
        "clearing the set seconds must fall back to the exercise seconds"
    );

    reopened
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn req_tmr_006_rest_timer_end_time_survives_a_reload() {
    let name = database_name("rest-timer-end");
    let storage = open(&name).await;
    let timer = RestTimer::start(1_000_000, 90).expect("a valid timer");
    storage
        .save_rest_timer(TRAINING_ID, timer.end_ms())
        .await
        .expect("the end time must be saved");
    drop(storage);

    let reopened: Storage = open(&name).await;
    let end_ms = reopened
        .rest_timer_end(TRAINING_ID)
        .await
        .expect("the end time must be readable")
        .expect("the end time must remain");
    let restored = RestTimer::from_end_ms(end_ms);
    assert_eq!(restored.end_ms(), timer.end_ms());
    assert_eq!(restored.remaining_seconds(1_030_000), 60);
    assert_eq!(restored.remaining_seconds(timer.end_ms()), 0);

    reopened
        .clear_rest_timer(TRAINING_ID)
        .await
        .expect("the end time must be cleared");
    assert_eq!(
        reopened
            .rest_timer_end(TRAINING_ID)
            .await
            .expect("the end time must be readable"),
        None
    );

    reopened
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
async fn req_tmr_005_wake_lock_requests_and_reacquires_after_visibility_returns() {
    install_wake_lock_stub();
    let lock = ScreenWakeLock::default();
    lock.start();
    wait_until("the first wake lock request", || {
        counter("__wakeLockRequests") >= 1.0
    })
    .await;
    assert_eq!(counter("__wakeLockRequests"), 1.0);

    set_visibility("hidden");
    dispatch_visibility_change();
    wait_for_tick().await;
    assert_eq!(
        counter("__wakeLockRequests"),
        1.0,
        "the wake lock must not be requested while the page is hidden"
    );

    set_released(true);
    set_visibility("visible");
    dispatch_visibility_change();
    wait_until("the wake lock reacquisition", || {
        counter("__wakeLockRequests") >= 2.0
    })
    .await;
    assert_eq!(counter("__wakeLockRequests"), 2.0);

    lock.stop();
    wait_until("the wake lock release", || {
        counter("__wakeLockReleases") >= 1.0
    })
    .await;
}

#[wasm_bindgen_test]
async fn req_tmr_004_finished_rest_plays_sound_and_vibrates() {
    install_sound_stub();
    assert!(
        sound::unlock(),
        "the sound must be prepared in a user action"
    );
    assert!(
        sound::notify_finished(),
        "the sound must play when the rest finishes"
    );
    assert_eq!(counter("__restAudioStarts"), 1.0);
    assert_eq!(counter("__restVibrations"), 1.0);
}

fn install_wake_lock_stub() {
    js_sys::eval(
        r#"
        window.__wakeLockRequests = 0;
        window.__wakeLockReleases = 0;
        window.__wakeLockReleased = false;
        window.__visibility = 'visible';
        Object.defineProperty(document, 'visibilityState', {
            configurable: true,
            get: function () { return window.__visibility; }
        });
        const lock = {
            request: function () {
                window.__wakeLockRequests++;
                window.__wakeLockReleased = false;
                return Promise.resolve({
                    get released() { return window.__wakeLockReleased; },
                    release: function () {
                        window.__wakeLockReleased = true;
                        window.__wakeLockReleases++;
                        return Promise.resolve();
                    }
                });
            }
        };
        try {
            Object.defineProperty(navigator, 'wakeLock', { configurable: true, value: lock });
        } catch (error) {
            Object.defineProperty(Navigator.prototype, 'wakeLock', { configurable: true, value: lock });
        }
        "#,
    )
    .expect("the wake lock stub must be installed");
}

fn install_sound_stub() {
    js_sys::eval(
        r#"
        window.__restAudioStarts = 0;
        window.__restVibrations = 0;
        class FakeAudioParam {
            constructor() { this.value = 0; }
            setValueAtTime(value) { this.value = value; return this; }
        }
        class FakeOscillator {
            constructor() { this.frequency = new FakeAudioParam(); }
            connect() {}
            start() { window.__restAudioStarts++; }
            stop() {}
        }
        class FakeGain {
            constructor() { this.gain = new FakeAudioParam(); }
            connect() {}
        }
        class FakeAudioContext {
            constructor() {
                this.currentTime = 0;
                this.state = 'running';
                this.destination = {};
            }
            createOscillator() { return new FakeOscillator(); }
            createGain() { return new FakeGain(); }
            resume() { this.state = 'running'; return Promise.resolve(); }
        }
        Object.defineProperty(window, 'AudioContext', {
            configurable: true,
            value: FakeAudioContext
        });
        Object.defineProperty(navigator, 'vibrate', {
            configurable: true,
            value: function () { window.__restVibrations++; return true; }
        });
        "#,
    )
    .expect("the sound stub must be installed");
}

fn set_visibility(value: &str) {
    js_sys::eval(&format!("window.__visibility = '{value}';")).expect("the visibility must be set");
}

fn set_released(value: bool) {
    js_sys::eval(&format!("window.__wakeLockReleased = {value};"))
        .expect("the released flag must be set");
}

fn dispatch_visibility_change() {
    js_sys::eval("document.dispatchEvent(new Event('visibilitychange'));")
        .expect("the visibility change must be dispatched");
}

fn counter(name: &str) -> f64 {
    js_sys::eval(&format!("window.{name} || 0"))
        .expect("the counter must be readable")
        .as_f64()
        .unwrap_or(0.0)
}

async fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    for _ in 0..600 {
        if condition() {
            return;
        }
        wait_for_tick().await;
    }
    panic!("{what} did not happen");
}

async fn wait_for_tick() {
    let promise = Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10);
        }
    });
    JsFuture::from(promise)
        .await
        .expect("the timer must resolve");
}
