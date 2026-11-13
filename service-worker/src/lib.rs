//! Service Worker の wasm の入口。
//!
//! ブラウザのイベントへの接続は、ローダーの JavaScript がこのモジュールの関数を呼ぶ。

use wasm_bindgen::prelude::*;

/// Service Worker の起動時に呼ばれる入口。
#[wasm_bindgen]
pub fn service_worker_start() {}
