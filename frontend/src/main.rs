//! アプリの画面を起動する。

#[cfg(target_arch = "wasm32")]
fn main() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!(
            "panic: {info:?}"
        )));
    }));
    dioxus::launch(warrior_frontend::app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
