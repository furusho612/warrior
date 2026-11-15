//! レスト終了の音と振動 (REQ-TMR-004)。
//!
//! 音は Web Audio の短い音で知らせる。ブラウザが自動再生を止める場合は、セットの
//! 完了の操作の中で音の再生を許可しておき、終了の時点で鳴らす。音を鳴らせなかった
//! ときは false を返し、画面が終了の表示と合わせて知らせる。
//! 振動に対応する端末では、音と同じときに振動する。

use std::cell::RefCell;

use wasm_bindgen::JsCast;
use web_sys::{AudioContext, AudioContextState, AudioNode};

/// 終了の音の周波数 (Hz)。
const TONE_HZ: f32 = 880.0;

/// 終了の音の長さ (秒)。
const TONE_SECONDS: f64 = 0.25;

/// 終了の音の大きさ。
const TONE_GAIN: f32 = 0.15;

/// 振動の長さ (ミリ秒)。
const VIBRATION_MS: u32 = 200;

thread_local! {
    static AUDIO: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
}

/// 利用者の操作の中で音の再生を許可する。
///
/// ブラウザが音の再生を止める場合は false を返す。呼び出し側は音を補助として扱い、
/// 失敗してもセットの完了を止めない。
pub fn unlock() -> bool {
    let Some(context) = context() else {
        return false;
    };
    if context.state() == AudioContextState::Suspended {
        let _ = context.resume();
    }
    true
}

/// レスト終了の音を鳴らす。鳴らせなかったときは false を返す。
pub fn play_finished_sound() -> bool {
    let Some(context) = context() else {
        return false;
    };
    if context.state() != AudioContextState::Running {
        return false;
    }
    let now = context.current_time();
    let Ok(oscillator) = context.create_oscillator() else {
        return false;
    };
    let Ok(gain) = context.create_gain() else {
        return false;
    };
    if oscillator
        .frequency()
        .set_value_at_time(TONE_HZ, now)
        .is_err()
        || gain.gain().set_value_at_time(TONE_GAIN, now).is_err()
    {
        return false;
    }
    let oscillator_node: &AudioNode = oscillator.unchecked_ref();
    let gain_node: &AudioNode = gain.unchecked_ref();
    let destination = context.destination();
    let destination: &AudioNode = destination.unchecked_ref();
    if oscillator_node.connect_with_audio_node(gain_node).is_err()
        || gain_node.connect_with_audio_node(destination).is_err()
    {
        return false;
    }
    if oscillator.start().is_err() {
        return false;
    }
    oscillator.stop_with_when(now + TONE_SECONDS).is_ok()
}

/// レスト終了の振動を行う。振動に対応しない端末では false を返す。
pub fn vibrate_finished() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    window.navigator().vibrate_with_duration(VIBRATION_MS)
}

/// レスト終了を音と振動で知らせる。音を鳴らせたかを返す。
pub fn notify_finished() -> bool {
    let sound = play_finished_sound();
    // 振動に対応しない端末では false を返す。音と画面の終了表示で知らせる。
    let _ = vibrate_finished();
    sound
}

fn context() -> Option<AudioContext> {
    AUDIO.with(|audio| {
        let mut audio = audio.borrow_mut();
        if audio
            .as_ref()
            .is_some_and(|context| context.state() == AudioContextState::Closed)
        {
            *audio = None;
        }
        if audio.is_none() {
            *audio = AudioContext::new().ok();
        }
        audio.clone()
    })
}
