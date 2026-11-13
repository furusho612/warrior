// Service Worker のローダー。
// ブラウザは Service Worker の登録に JavaScript を要求するため、
// wasm の初期化と install の待機だけを行う最小のローダーを置く (ADR-020)。

importScripts("/sw/warrior_sw.js");

const ready = wasm_bindgen("/sw/warrior_sw_bg.wasm");

self.addEventListener("install", function (event) {
  event.waitUntil(
    ready.then(function () {
      return wasm_bindgen.service_worker_install();
    }),
  );
});

ready.then(function () {
  wasm_bindgen.service_worker_start();
});
