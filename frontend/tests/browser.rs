//! ブラウザの DOM の入口を確かめるテスト。
//!
//! IndexedDB のテストは `tests/storage.rs` が持つ。

use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn browser_has_a_document() {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("a browser must have a document");
    let root = document.document_element().expect("the root element");
    assert_eq!(root.tag_name(), "HTML");
}
