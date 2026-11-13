//! アプリの画面を起動する。

#[cfg(target_arch = "wasm32")]
fn main() {
    // wasm の既定のパニックの出力は開発者コンソールに残らない。E2E が `panic:` を含む
    // ログを検査できるよう、自前のフックで `console.error` へ書く (ADR-025)。
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!(
            "panic: {info:?}"
        )));
    }));
    dioxus::launch(warrior_frontend::app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
