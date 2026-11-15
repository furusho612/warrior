//! メモの入力の保存ループ。
//!
//! 入力の版が変わるたびに保存し、保存中に新しい入力があったときは、その入力を
//! 保存し終えてから次の保存へ進む。保存の往復は入力の完了条件にしない。

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;

use dioxus::prelude::*;

/// メモの入力と保存が共有する状態。
#[derive(Clone)]
pub(super) struct MemoSaveState {
    /// 保存中の表示。
    pub(super) saving: Signal<bool>,
    /// 保存のループが走っているか。
    pub(super) in_flight: Rc<Cell<bool>>,
    /// 保存中に新しい入力があったか。
    pub(super) dirty: Rc<Cell<bool>>,
    /// 入力中のメモ。保存の開始時の値を読む。
    pub(super) draft: Rc<RefCell<String>>,
}

impl MemoSaveState {
    /// 入力の保存を始める。既に保存中なら false を返す。
    pub(super) fn request(&self) -> bool {
        self.dirty.set(true);
        !self.in_flight.replace(true)
    }
}

/// 入力の版が変わるたびに保存する。保存が失敗したら止める。
pub(super) async fn run_save_loop<F, Fut>(mut state: MemoSaveState, mut save: F)
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = bool>,
{
    state.saving.set(true);
    loop {
        state.dirty.set(false);
        let draft = state.draft.borrow().clone();
        if !save(draft).await {
            break;
        }
        if !state.dirty.get() {
            break;
        }
    }
    state.saving.set(false);
    state.in_flight.set(false);
}
