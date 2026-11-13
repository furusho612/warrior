//! アプリの画面を起動する。

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::launch(warrior_frontend::app::App);
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
