//! 種目の木の端末内の保存、並べ替え、削除と、失敗した操作の破棄のブラウザのテスト。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-NFR-005`、`REQ-SET-005`。
//! 種目の作成、削除、並べ替えが送信待ちの列を通り、削除でセットと段と参照が消えること、
//! 失敗した作成の破棄で対象の行だけが消え、失敗した削除の破棄でサーバーの記録から
//! 対象の種目だけを戻せることを確かめる。
//!
//! 復元は `restore.rs`、破棄は `discard.rs` が持つ。

#[path = "storage_exercise/discard.rs"]
mod discard;
#[path = "storage_exercise/restore.rs"]
mod restore;
mod support;

use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);
