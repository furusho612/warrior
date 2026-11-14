//! セットの入力の保存のループと、セットごとの保存の直列化。
//!
//! 入力の保存は版が変わるたびに繰り返す。セットの完了は、そのセットの実行中の保存が
//! 終わってから書き、完了したセットへは値を書かない。完了の後ろに値の更新を積むと、
//! サーバーが完了済みのセットの更新を 409 で拒否するためである。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

use dioxus::prelude::*;
use warrior_core::exercise::Laterality;

use super::exercise_view::{set_error, ExerciseView};
use super::sets::SetView;
use crate::storage;
use crate::sync;
use crate::text::Key;
use crate::training_set;

/// セットごとの保存の状態。
#[derive(Default)]
struct SaveStates {
    /// 保存のループが走っているセット。
    in_flight: HashSet<String>,
    /// 完了の書き込みのために、保存のループを止めたセット。
    stopped: HashSet<String>,
    /// 保存の終わりを待つ処理。
    waiters: HashMap<String, Vec<Waker>>,
}

/// セットの保存が同時に走らないようにする印。
#[derive(Clone, Default)]
pub(super) struct SetSaver {
    states: Rc<RefCell<SaveStates>>,
}

impl SetSaver {
    fn start(&self, set_id: &str) -> bool {
        let mut states = self.states.borrow_mut();
        if states.stopped.contains(set_id) {
            return false;
        }
        states.in_flight.insert(set_id.to_owned())
    }

    fn finish(&self, set_id: &str) {
        let waiters = {
            let mut states = self.states.borrow_mut();
            states.in_flight.remove(set_id);
            states.waiters.remove(set_id)
        };
        for waker in waiters.unwrap_or_default() {
            waker.wake();
        }
    }

    /// セットの保存を止める。実行中の保存は終わるまで走る。
    pub(super) fn stop(&self, set_id: &str) {
        self.states.borrow_mut().stopped.insert(set_id.to_owned());
    }

    /// 止めたセットの保存を再び許す。
    pub(super) fn resume(&self, set_id: &str) {
        self.states.borrow_mut().stopped.remove(set_id);
    }

    /// セットの保存を止めているか。
    fn is_stopped(&self, set_id: &str) -> bool {
        self.states.borrow().stopped.contains(set_id)
    }

    /// セットの実行中の保存が終わるまで待つ。
    pub(super) async fn wait_finished(&self, set_id: &str) {
        poll_fn(|context| {
            let mut states = self.states.borrow_mut();
            if !states.in_flight.contains(set_id) {
                return Poll::Ready(());
            }
            let waiters = states.waiters.entry(set_id.to_owned()).or_default();
            if !waiters.iter().any(|waker| waker.will_wake(context.waker())) {
                waiters.push(context.waker().clone());
            }
            Poll::Pending
        })
        .await;
    }
}

/// 入力の最新の値を端末内へ保存する。保存中に新しい入力があれば繰り返す。
pub(super) async fn save_set_loop(
    training_id: String,
    set_id: String,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
    saver: SetSaver,
) {
    if !saver.start(&set_id) {
        return;
    }
    let lateral = lateral_for_set(views, &set_id);
    while let Some(set) = find_set(views, &set_id) {
        if saver.is_stopped(&set_id) || set.completed() {
            break;
        }
        if let Some(key) = set.validation_error(lateral) {
            set_error(error, key);
            break;
        }
        let revision = set.revision;
        let values = set.values(lateral);
        let segment_records = set
            .segments
            .iter()
            .map(|segment| segment.record.clone())
            .collect::<Vec<_>>();
        let (rows, operation) =
            match training_set::update(&training_id, &set.record, &segment_records, &values) {
                Ok(rows) => rows,
                Err(_) => {
                    set_error(error, Key::RecordSetSaveFailed);
                    break;
                }
            };
        let result = match storage::current().await {
            Ok(storage) => {
                if saving_stopped(views, &saver, &set_id) {
                    saver.finish(&set_id);
                    return;
                }
                storage
                    .save_set_update(&rows.set, &rows.segments, &operation)
                    .await
            }
            Err(error) => Err(error),
        };
        if result.is_err() {
            set_error(error, Key::RecordSetSaveFailed);
            break;
        }
        sync::flush_in_background();
        if saving_stopped(views, &saver, &set_id) {
            saver.finish(&set_id);
            return;
        }
        match find_set(views, &set_id) {
            Some(latest) if latest.revision == revision => {
                // 版を確かめてから保存の印を外すまでの間にも入力が届く。
                // 印を外した後にもう一度確かめ、変わっていれば自分で続ける。
                saver.finish(&set_id);
                if saving_stopped(views, &saver, &set_id) {
                    return;
                }
                match find_set(views, &set_id) {
                    Some(latest) if latest.revision == revision => return,
                    _ => {
                        if !saver.start(&set_id) {
                            return;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    saver.finish(&set_id);
}

/// 保存を続けない状態か。止めたセット、完了したセット、画面から消えたセット。
fn saving_stopped(views: Signal<Vec<ExerciseView>>, saver: &SetSaver, set_id: &str) -> bool {
    saver.is_stopped(set_id) || find_set(views, set_id).is_none_or(|set| set.completed())
}

fn find_set(views: Signal<Vec<ExerciseView>>, set_id: &str) -> Option<SetView> {
    (views)()
        .iter()
        .flat_map(|view| view.sets.iter())
        .find(|set| set.record.id == set_id)
        .cloned()
}

fn lateral_for_set(views: Signal<Vec<ExerciseView>>, set_id: &str) -> bool {
    (views)()
        .iter()
        .find(|view| view.sets.iter().any(|set| set.record.id == set_id))
        .is_some_and(|view| view.snapshot.laterality == Laterality::Unilateral)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Waker};

    use wasm_bindgen_test::wasm_bindgen_test;
    use wasm_bindgen_test::wasm_bindgen_test_configure;

    use super::SetSaver;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn a_stopped_set_does_not_start_saving() {
        let saver = SetSaver::default();
        saver.stop("set");
        assert!(!saver.start("set"));
        assert!(saver.is_stopped("set"));
        saver.resume("set");
        assert!(!saver.is_stopped("set"));
        assert!(saver.start("set"));
    }

    #[wasm_bindgen_test]
    fn waiting_for_a_set_ends_when_its_saving_ends() {
        let saver = SetSaver::default();
        assert!(saver.start("set"));
        let mut waiting = pin!(saver.wait_finished("set"));
        let mut context = Context::from_waker(Waker::noop());
        assert!(waiting.as_mut().poll(&mut context).is_pending());
        saver.finish("set");
        assert!(waiting.as_mut().poll(&mut context).is_ready());
    }

    #[wasm_bindgen_test]
    fn waiting_for_an_idle_set_ends_at_once() {
        let saver = SetSaver::default();
        let mut waiting = pin!(saver.wait_finished("set"));
        let mut context = Context::from_waker(Waker::noop());
        assert!(waiting.as_mut().poll(&mut context).is_ready());
    }

    #[wasm_bindgen_test]
    fn a_saving_set_does_not_start_twice() {
        let saver = SetSaver::default();
        assert!(saver.start("set"));
        assert!(!saver.start("set"));
        assert!(saver.start("other"));
        saver.finish("set");
        assert!(saver.start("set"));
    }
}
