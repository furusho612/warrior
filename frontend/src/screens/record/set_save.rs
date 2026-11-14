//! セットの入力の保存と完了の処理。
//!
//! 入力の版が変わるたびに端末内へ保存し、保存中に新しい入力が届いたら繰り返す。
//! セットの完了は、いまの値の更新を並べてから完了の操作を送り、成功したら
//! レストタイマーのイベントを 1 回発行する。

use dioxus::prelude::*;
use warrior_core::exercise::RecordMode;
use warrior_core::superset::rest_timer_starts;

use super::exercise_view::{find_view_and_set, is_lateral, set_error, ExerciseView, SetSaver};
use super::set_events::{SetCompleted, SetCompletedEvents};
use super::sets::SetView;
use crate::storage;
use crate::sync;
use crate::text::Key;
use crate::training_set;

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
    let Some(mode) = mode_for_set(views, &set_id) else {
        saver.finish(&set_id);
        return;
    };
    while let Some(set) = find_set(views, &set_id) {
        if let Some(key) = set.validation_error(lateral) {
            set_error(error, key);
            break;
        }
        let revision = set.revision;
        let values = set.values(lateral, mode);
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
        match find_set(views, &set_id) {
            None => {
                saver.finish(&set_id);
                return;
            }
            Some(latest) if latest.revision == revision => {
                // 版を確かめてから保存の印を外すまでの間にも入力が届く。
                // 印を外した後にもう一度確かめ、変わっていれば自分で続ける。
                saver.finish(&set_id);
                match find_set(views, &set_id) {
                    None => return,
                    Some(latest) if latest.revision == revision => return,
                    _ => {
                        if !saver.start(&set_id) {
                            return;
                        }
                    }
                }
            }
            Some(_) => {}
        }
    }
    saver.finish(&set_id);
}

/// セットの完了の状態を変え、完了したらイベントを 1 回発行する。
pub(super) async fn complete_set(
    training_id: String,
    set_id: String,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
    completed_events: SetCompletedEvents,
) {
    let Some((view, set)) = find_view_and_set(views, &set_id) else {
        return;
    };
    let completed = !set.completed();
    let lateral = is_lateral(&view.snapshot);
    let mode = view.snapshot.record_mode;
    if completed && !set.can_complete(view.snapshot.record_mode, lateral) {
        set_error(
            error,
            if view.snapshot.record_mode == RecordMode::Time {
                Key::RecordSetTimeInvalid
            } else if lateral {
                Key::RecordSetSidesInvalid
            } else {
                Key::RecordSetInputInvalid
            },
        );
        return;
    }
    let (updated, operation) = match training_set::complete(&training_id, &set.record, completed) {
        Ok(updated) => updated,
        Err(_) => {
            set_error(error, Key::RecordSetSaveFailed);
            return;
        }
    };
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            set_error(error, Key::RecordSetSaveFailed);
            return;
        }
    };
    // 完了の操作より先に、いまの値の更新を並べる。完了したセットは値を持つ。
    if completed {
        let values = set.values(lateral, mode);
        let segments = set
            .segments
            .iter()
            .map(|segment| segment.record.clone())
            .collect::<Vec<_>>();
        let (rows, update_operation) =
            match training_set::update(&training_id, &set.record, &segments, &values) {
                Ok(rows) => rows,
                Err(_) => {
                    set_error(error, Key::RecordSetSaveFailed);
                    return;
                }
            };
        if storage
            .save_set_update(&rows.set, &rows.segments, &update_operation)
            .await
            .is_err()
        {
            set_error(error, Key::RecordSetSaveFailed);
            return;
        }
    }
    update_completed(views, &set_id, completed);
    if storage
        .save_set_complete(&updated, &operation)
        .await
        .is_err()
    {
        update_completed(views, &set_id, !completed);
        set_error(error, Key::RecordSetSaveFailed);
        return;
    }
    if completed {
        let starts_rest_timer = rest_timer_starts(
            view.superset
                .as_ref()
                .map(|group| group.member_ids.as_slice()),
            &view.record.id,
        );
        completed_events.publish(SetCompleted {
            training_id,
            training_exercise_id: view.record.id,
            set_id,
            starts_rest_timer,
        });
    }
    sync::flush_in_background();
}

fn update_completed(mut views: Signal<Vec<ExerciseView>>, set_id: &str, completed: bool) {
    let mut list = (views)();
    for set in list.iter_mut().flat_map(|view| view.sets.iter_mut()) {
        if set.record.id != set_id {
            continue;
        }
        set.record.completed = if completed { 1 } else { 0 };
        set.bump();
    }
    views.set(list);
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
        .is_some_and(|view| is_lateral(&view.snapshot))
}

fn mode_for_set(views: Signal<Vec<ExerciseView>>, set_id: &str) -> Option<RecordMode> {
    (views)()
        .iter()
        .find(|view| view.sets.iter().any(|set| set.record.id == set_id))
        .map(|view| view.snapshot.record_mode)
}
