// Service Worker のローダー。
// ブラウザは Service Worker の登録に JavaScript を要求するため、
// wasm の初期化だけを行う最小のローダーを置く (ADR-020)。

importScripts("/sw/warrior_sw.js");

wasm_bindgen("/sw/warrior_sw_bg.wasm").then(function () {
  wasm_bindgen.service_worker_start();
});
