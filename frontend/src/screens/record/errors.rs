//! 記録画面のエラー。画面全体のエラーと、セットごとのエラーを分けて持つ。
//!
//! セットの保存はセットごとに走る。あるセットのエラーを別のセットの保存結果で
//! 消さないため、セットのエラーはセット ID ごとに保持する。

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::text::{text, Key};

/// 記録画面のエラー。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct RecordErrors {
    /// 画面全体のエラー。読み込み、種目、スーパーセットの操作で使う。
    pub(super) load: Option<String>,
    /// セット ID ごとのエラー。
    pub(super) sets: BTreeMap<String, String>,
}

/// 画面全体のエラーを設定する。
pub(super) fn set_load_error(mut errors: Signal<RecordErrors>, key: Key) {
    let mut current = errors();
    current.load = Some(text(key).to_owned());
    errors.set(current);
}

/// セットのエラーを設定する。
pub(super) fn set_set_error(mut errors: Signal<RecordErrors>, set_id: &str, key: Key) {
    let mut current = errors();
    current.sets.insert(set_id.to_owned(), text(key).to_owned());
    errors.set(current);
}

/// セットのエラーだけを消す。
pub(super) fn clear_set_error(mut errors: Signal<RecordErrors>, set_id: &str) {
    let mut current = errors();
    if current.sets.remove(set_id).is_some() {
        errors.set(current);
    }
}

/// 画面全体のエラーだけを消す。
pub(super) fn clear_load_error(mut errors: Signal<RecordErrors>) {
    let mut current = errors();
    if current.load.take().is_some() {
        errors.set(current);
    }
}

/// 残っているセットのエラーだけを残す。
pub(super) fn retain_set_errors(mut errors: Signal<RecordErrors>, set_ids: &[String]) {
    let mut current = errors();
    let before = current.sets.len();
    current
        .sets
        .retain(|set_id, _| set_ids.iter().any(|kept| kept == set_id));
    if current.sets.len() != before {
        errors.set(current);
    }
}
