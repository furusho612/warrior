//! 記録画面の種目の状態と、セットの保存の処理。
//!
//! セットの入力は画面へすぐ反映し、端末内の保存は入力の版が変わるたびに行う。
//! セットの完了は `SetCompleted` を 1 回発行する。表示は
//! `frontend/src/screens/record/exercises.rs` が担う。
//!
//! セットの操作は `operations.rs`、組の操作は `superset.rs`、集計は `totals.rs`、
//! 画面が読む状態は `view.rs` が持つ。

mod operations;
mod superset;
mod totals;
mod view;

pub(super) use operations::{dispatch, SetSaver};
pub(super) use superset::{dissolve_superset, save_superset_group};
pub(super) use totals::{exercise_diff, training_difference};
pub(super) use view::{ExerciseView, SupersetGroupView};

use dioxus::prelude::*;

use crate::text::{text, Key};

fn set_error(mut error: Signal<Option<String>>, key: Key) {
    error.set(Some(text(key).to_owned()));
}
