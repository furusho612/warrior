//! 記録画面の種目の状態と、セットの保存の処理。
//!
//! セットの入力は画面へすぐ反映し、端末内の保存は入力の版が変わるたびに行う。
//! セットの完了は `SetCompleted` を 1 回発行する。表示は
//! `frontend/src/screens/record/exercises.rs` が担う。

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use dioxus::prelude::*;
use warrior_core::exercise::{ExerciseSnapshot, Laterality, RecordMode};
use warrior_core::previous::{
    exercise_difference, total_weight_difference, ExerciseDifference, PreviousExercise,
    PreviousTraining, TotalWeightDifference,
};
use warrior_core::rpe;
use warrior_core::set_calc::SetKind;
use warrior_core::superset::{rest_timer_starts, SupersetMembers};
use warrior_core::training_record::TrainingExerciseRecord;

use super::errors::{clear_set_error, set_load_error, set_set_error, RecordErrors};
use super::set_events::{SetCompleted, SetCompletedEvents};
use super::sets::{
    kind_allowed, move_allowed, previous_values, Field, SegmentView, SetAction, SetView,
};
use crate::ids::{new_uuid, now_iso};
use crate::storage;
use crate::sync;
use crate::text::Key;
use crate::training_set::{self, SegmentSideValues, SetSegmentValues, SetValues};

/// 画面が読む 1 つの種目の組。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SupersetGroupView {
    /// 組の ID。
    pub(super) id: String,
    /// 組の種目の行の ID。並び順である。
    pub(super) member_ids: Vec<String>,
}

/// 画面が読む 1 つの種目。
#[derive(Clone, PartialEq)]
pub(super) struct ExerciseView {
    /// 種目の行。
    pub(super) record: TrainingExerciseRecord,
    /// 記録した時点の種目。
    pub(super) snapshot: ExerciseSnapshot,
    /// 前回の記録。
    pub(super) previous: Option<PreviousExercise>,
    /// 前回の記録を読めなかったか。
    pub(super) previous_error: bool,
    /// セットの行。
    pub(super) sets: Vec<SetView>,
    /// 記録した時点の体重 (kg)。身体情報が無いときは None。
    pub(super) body_weight_kg: Option<f64>,
    /// この種目を含むスーパーセットの組。
    pub(super) superset: Option<SupersetGroupView>,
}

fn is_lateral(snapshot: &ExerciseSnapshot) -> bool {
    snapshot.laterality == Laterality::Unilateral && snapshot.record_mode != RecordMode::Time
}

/// セットの保存が同時に走らないようにする印。
#[derive(Clone, Default)]
pub(super) struct SetSaver {
    in_flight: Rc<RefCell<HashSet<String>>>,
}

impl SetSaver {
    fn start(&self, set_id: &str) -> bool {
        self.in_flight.borrow_mut().insert(set_id.to_owned())
    }

    fn finish(&self, set_id: &str) {
        self.in_flight.borrow_mut().remove(set_id);
    }
}

/// 今回の入力から作った種目の合計。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CurrentTotals {
    total_weight_kg: f64,
    total_duration_seconds: u32,
    longest_duration_seconds: u32,
}

fn current_totals(view: &ExerciseView) -> CurrentTotals {
    let mut totals = CurrentTotals::default();
    for set in &view.sets {
        let Some(record) = set.to_record(
            &view.snapshot.exercise_id,
            view.snapshot.record_mode,
            is_lateral(&view.snapshot),
        ) else {
            continue;
        };
        totals.total_weight_kg += record.total_weight_kg();
        totals.total_duration_seconds += record.total_duration_seconds();
        totals.longest_duration_seconds = totals
            .longest_duration_seconds
            .max(record.longest_duration_seconds().unwrap_or(0));
    }
    totals
}

/// 種目ごとの今回と前回の差。
pub(super) fn exercise_diff(view: &ExerciseView) -> Option<ExerciseDifference> {
    let totals = current_totals(view);
    exercise_difference(
        view.snapshot.record_mode,
        totals.total_weight_kg,
        totals.total_duration_seconds,
        totals.longest_duration_seconds,
        view.previous.as_ref(),
    )
}

/// トレーニング全体の今回と前回の差。
pub(super) fn training_difference(
    views: &[ExerciseView],
    previous: Option<&PreviousTraining>,
) -> Option<TotalWeightDifference> {
    let current = views
        .iter()
        .filter(|view| view.snapshot.record_mode != RecordMode::Time)
        .map(|view| current_totals(view).total_weight_kg)
        .sum::<f64>();
    total_weight_difference(current, previous.map(|training| training.total_weight_kg))
}

/// セットの入力の値を画面へ反映する。
pub(super) fn update_set_input(
    mut views: Signal<Vec<ExerciseView>>,
    set_id: &str,
    segment_id: &str,
    field: Field,
    value: String,
) {
    let mut list = (views)();
    let Some(view) = list
        .iter_mut()
        .find(|view| view.sets.iter().any(|set| set.record.id == set_id))
    else {
        return;
    };
    let lateral = is_lateral(&view.snapshot);
    let Some(set) = view.sets.iter_mut().find(|set| set.record.id == set_id) else {
        return;
    };
    let Some(segment) = set
        .segments
        .iter_mut()
        .find(|segment| segment.record.id == segment_id)
    else {
        return;
    };
    match field {
        Field::Weight => segment.weight = value,
        Field::Reps => segment.reps = value,
        Field::Momentum => segment.momentum = value,
        Field::Assisted => segment.assisted = value,
        Field::Duration => segment.duration = value,
        Field::LeftWeight => segment.left_weight = value,
        Field::LeftReps => segment.left_reps = value,
        Field::RightWeight => segment.right_weight = value,
        Field::RightReps => segment.right_reps = value,
    }
    if set.completed() && !set.values(lateral).is_complete() {
        set.record.completed = 0;
    }
    set.bump();
    views.set(list);
}

/// セットの RPE を画面へ反映する。
pub(super) fn update_set_rpe(mut views: Signal<Vec<ExerciseView>>, set_id: &str, value: String) {
    let mut list = (views)();
    let Some(set) = list
        .iter_mut()
        .flat_map(|view| view.sets.iter_mut())
        .find(|set| set.record.id == set_id)
    else {
        return;
    };
    if let Ok(value) = rpe::parse(&value) {
        set.record.rpe = value;
    }
    set.rpe = value;
    set.bump();
    views.set(list);
}

/// セット種別を画面へ反映する。ドロップセットの位置の規則を満たさない変更は拒む。
pub(super) fn update_set_kind(
    mut views: Signal<Vec<ExerciseView>>,
    set_id: &str,
    kind: SetKind,
    error: Signal<RecordErrors>,
) -> bool {
    let mut list = (views)();
    let Some(view_index) = list
        .iter()
        .position(|view| view.sets.iter().any(|set| set.record.id == set_id))
    else {
        return false;
    };
    let Some(set_index) = list[view_index]
        .sets
        .iter()
        .position(|set| set.record.id == set_id)
    else {
        return false;
    };
    if !kind_allowed(&list[view_index].sets, set_index, kind) {
        set_set_error(error, set_id, Key::RecordSetDropInvalid);
        return false;
    }
    let set = &mut list[view_index].sets[set_index];
    set.record.kind = kind.as_str().to_owned();
    if kind != SetKind::Drop {
        set.segments.truncate(1);
    }
    set.bump();
    views.set(list);
    true
}

/// セットに段を追加して画面へ反映する。
pub(super) fn append_segment(
    mut views: Signal<Vec<ExerciseView>>,
    set_id: &str,
    error: Signal<RecordErrors>,
) -> bool {
    let mut list = (views)();
    let Some(view) = list
        .iter_mut()
        .find(|view| view.sets.iter().any(|set| set.record.id == set_id))
    else {
        return false;
    };
    let lateral = is_lateral(&view.snapshot);
    let Some(set) = view.sets.iter_mut().find(|set| set.record.id == set_id) else {
        return false;
    };
    if set.kind() != SetKind::Drop {
        return false;
    }
    let Ok(segment_id) = new_uuid() else {
        set_set_error(error, set_id, Key::RecordSetSaveFailed);
        return false;
    };
    let now = now_iso();
    let side = lateral.then_some(SegmentSideValues {
        weight_kg: 0.0,
        reps: 0,
    });
    set.record.completed = 0;
    set.segments.push(SegmentView {
        record: warrior_core::training_record::SetSegmentRecord {
            id: segment_id,
            set_id: set_id.to_owned(),
            position: i64::try_from(set.segments.len())
                .unwrap_or(i64::MAX)
                .saturating_add(1),
            weight_kg: Some(0.0),
            reps: Some(0),
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left_weight_kg: side.map(|side| side.weight_kg),
            left_reps: side.map(|side| i64::from(side.reps)),
            left_duration_seconds: None,
            right_weight_kg: side.map(|side| side.weight_kg),
            right_reps: side.map(|side| i64::from(side.reps)),
            right_duration_seconds: None,
            created_at: now.clone(),
            updated_at: now,
        },
        weight: String::new(),
        reps: String::new(),
        momentum: String::new(),
        assisted: String::new(),
        duration: String::new(),
        left_weight: String::new(),
        left_reps: String::new(),
        right_weight: String::new(),
        right_reps: String::new(),
    });
    set.bump();
    views.set(list);
    true
}

/// 段を削除して画面へ反映する。残りの段が無いときはセットを削除する。
pub(super) async fn delete_segment(
    training_id: String,
    set_id: String,
    segment_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
    saver: SetSaver,
) {
    let mut list = (views)();
    let Some(set) = list
        .iter_mut()
        .flat_map(|view| view.sets.iter_mut())
        .find(|set| set.record.id == set_id)
    else {
        return;
    };
    set.segments
        .retain(|segment| segment.record.id != segment_id);
    if set.segments.is_empty() {
        views.set(list);
        delete_set(training_id, set_id, views, error).await;
        return;
    }
    for (index, segment) in set.segments.iter_mut().enumerate() {
        segment.record.position = i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1);
    }
    set.bump();
    views.set(list);
    save_set_loop(training_id, set_id, views, error, saver).await;
}

/// セットの追加を画面へ反映し、端末内へ保存する。
pub(super) async fn add_set(
    training_id: String,
    view_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
) {
    let Some(view) = (views)()
        .iter()
        .find(|view| view.record.id == view_id)
        .cloned()
    else {
        return;
    };
    let position = view
        .sets
        .iter()
        .map(|set| set.record.position)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let side = is_lateral(&view.snapshot).then_some(SegmentSideValues {
        weight_kg: 0.0,
        reps: 0,
    });
    let values = SetValues {
        kind: SetKind::Normal,
        segments: vec![SetSegmentValues {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left: side,
            right: side,
        }],
        completed: false,
        body_weight_kg: view.body_weight_kg,
        rpe: None,
        grip: None,
    };
    let (rows, operation) = match training_set::create(&training_id, &view_id, position, &values) {
        Ok(rows) => rows,
        Err(_) => {
            set_load_error(error, Key::RecordSetSaveFailed);
            return;
        }
    };
    let set_view = SetView::from_rows(rows.set.clone(), rows.segments.clone());
    {
        let mut list = (views)();
        if let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) {
            view.sets.push(set_view);
        }
        views.set(list);
    }
    let result = match storage::current().await {
        Ok(storage) => {
            storage
                .save_set_create(&rows.set, &rows.segments, &operation)
                .await
        }
        Err(error) => Err(error),
    };
    if result.is_err() {
        let mut list = (views)();
        if let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) {
            view.sets.retain(|set| set.record.id != rows.set.id);
        }
        views.set(list);
        set_load_error(error, Key::RecordSetSaveFailed);
        return;
    }
    sync::flush_in_background();
}

/// 入力の最新の値を端末内へ保存する。保存中に新しい入力があれば繰り返す。
pub(super) async fn save_set_loop(
    training_id: String,
    set_id: String,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
    saver: SetSaver,
) {
    if !saver.start(&set_id) {
        return;
    }
    let lateral = lateral_for_set(views, &set_id);
    while let Some(set) = find_set(views, &set_id) {
        if let Some(key) = set.validation_error(lateral) {
            set_set_error(error, &set_id, key);
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
                    set_set_error(error, &set_id, Key::RecordSetSaveFailed);
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
            set_set_error(error, &set_id, Key::RecordSetSaveFailed);
            break;
        }
        clear_set_error(error, &set_id);
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

/// 前回の値をセットの行としてコピーする。
pub(super) async fn copy_sets(
    training_id: String,
    view_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
) {
    let Some(view) = (views)()
        .iter()
        .find(|view| view.record.id == view_id)
        .cloned()
    else {
        return;
    };
    let Some(previous) = view.previous.as_ref() else {
        return;
    };
    let values = previous
        .sets
        .iter()
        .filter_map(|set| {
            previous_values(
                set,
                view.snapshot.record_mode,
                view.body_weight_kg,
                view.snapshot.laterality,
            )
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return;
    }
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            set_load_error(error, Key::RecordSetSaveFailed);
            return;
        }
    };
    for set in &view.sets {
        let operation = match training_set::delete(&training_id, &set.record.id) {
            Ok(operation) => operation,
            Err(_) => {
                set_load_error(error, Key::RecordSetSaveFailed);
                return;
            }
        };
        if storage
            .save_set_delete(&training_id, &set.record.id, &operation)
            .await
            .is_err()
        {
            set_load_error(error, Key::RecordSetSaveFailed);
            restore_sets(views, &view_id, error).await;
            return;
        }
    }
    let mut copied = Vec::new();
    for (index, values) in values.iter().enumerate() {
        let position = i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1);
        let (rows, operation) = match training_set::create(&training_id, &view_id, position, values)
        {
            Ok(rows) => rows,
            Err(_) => {
                set_load_error(error, Key::RecordSetSaveFailed);
                restore_sets(views, &view_id, error).await;
                return;
            }
        };
        if storage
            .save_set_create(&rows.set, &rows.segments, &operation)
            .await
            .is_err()
        {
            set_load_error(error, Key::RecordSetSaveFailed);
            restore_sets(views, &view_id, error).await;
            return;
        }
        copied.push(SetView::from_rows(rows.set, rows.segments));
    }
    {
        let mut list = (views)();
        if let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) {
            view.sets = copied;
        }
        views.set(list);
    }
    sync::flush_in_background();
}

/// セットの完了の状態を変え、完了したらイベントを 1 回発行する。
pub(super) async fn complete_set(
    training_id: String,
    set_id: String,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
    completed_events: SetCompletedEvents,
) {
    let Some((view, set)) = find_view_and_set(views, &set_id) else {
        return;
    };
    let completed = !set.completed();
    let lateral = is_lateral(&view.snapshot);
    if completed && !set.can_complete(view.snapshot.record_mode, lateral) {
        set_set_error(
            error,
            &set_id,
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
            set_set_error(error, &set_id, Key::RecordSetSaveFailed);
            return;
        }
    };
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            set_set_error(error, &set_id, Key::RecordSetSaveFailed);
            return;
        }
    };
    // 完了の操作より先に、いまの値の更新を並べる。完了したセットは値を持つ。
    if completed {
        let values = set.values(lateral);
        let segments = set
            .segments
            .iter()
            .map(|segment| segment.record.clone())
            .collect::<Vec<_>>();
        let (rows, update_operation) =
            match training_set::update(&training_id, &set.record, &segments, &values) {
                Ok(rows) => rows,
                Err(_) => {
                    set_set_error(error, &set_id, Key::RecordSetSaveFailed);
                    return;
                }
            };
        if storage
            .save_set_update(&rows.set, &rows.segments, &update_operation)
            .await
            .is_err()
        {
            set_set_error(error, &set_id, Key::RecordSetSaveFailed);
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
        set_set_error(error, &set_id, Key::RecordSetSaveFailed);
        return;
    }
    clear_set_error(error, &set_id);
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

/// セットを画面から消し、端末内の行と送信待ちの列から削除する。
pub(super) async fn delete_set(
    training_id: String,
    set_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
) {
    let Some((view, _set)) = find_view_and_set(views, &set_id) else {
        return;
    };
    let operation = match training_set::delete(&training_id, &set_id) {
        Ok(operation) => operation,
        Err(_) => {
            set_set_error(error, &set_id, Key::RecordSetSaveFailed);
            return;
        }
    };
    {
        let mut list = (views)();
        for view in list.iter_mut() {
            view.sets.retain(|set| set.record.id != set_id);
        }
        views.set(list);
    }
    let result = match storage::current().await {
        Ok(storage) => {
            storage
                .save_set_delete(&training_id, &set_id, &operation)
                .await
        }
        Err(error) => Err(error),
    };
    if result.is_err() {
        set_set_error(error, &set_id, Key::RecordSetSaveFailed);
        restore_sets(views, &view.record.id, error).await;
        return;
    }
    clear_set_error(error, &set_id);
    sync::flush_in_background();
}

/// セットの順序を変え、端末内の位置と送信待ちの列へ保存する。
pub(super) async fn move_set(
    training_id: String,
    set_id: String,
    delta: i32,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
) {
    let Some((view_id, ordered)) = move_set_order(views, &set_id, delta, error) else {
        return;
    };
    let operation = match training_set::reorder(&training_id, &view_id, &ordered) {
        Ok(operation) => operation,
        Err(_) => {
            set_set_error(error, &set_id, Key::RecordSetSaveFailed);
            return;
        }
    };
    let result = match storage::current().await {
        Ok(storage) => {
            storage
                .save_set_reorder(&training_id, &view_id, &ordered, &operation)
                .await
        }
        Err(error) => Err(error),
    };
    if result.is_err() {
        set_set_error(error, &set_id, Key::RecordSetSaveFailed);
        restore_sets(views, &view_id, error).await;
        return;
    }
    clear_set_error(error, &set_id);
    sync::flush_in_background();
}

fn move_set_order(
    mut views: Signal<Vec<ExerciseView>>,
    set_id: &str,
    delta: i32,
    error: Signal<RecordErrors>,
) -> Option<(String, Vec<String>)> {
    let mut list = (views)();
    let (view_id, ordered) = {
        let view = list
            .iter_mut()
            .find(|view| view.sets.iter().any(|set| set.record.id == set_id))?;
        let index = view.sets.iter().position(|set| set.record.id == set_id)?;
        let target = index as i32 + delta;
        if target < 0 || target as usize >= view.sets.len() {
            return None;
        }
        if !move_allowed(&view.sets, index, delta) {
            set_set_error(error, set_id, Key::RecordSetDropInvalid);
            return None;
        }
        view.sets.swap(index, target as usize);
        for (index, set) in view.sets.iter_mut().enumerate() {
            set.record.position = i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1);
            set.bump();
        }
        (
            view.record.id.clone(),
            view.sets.iter().map(|set| set.record.id.clone()).collect(),
        )
    };
    views.set(list);
    Some((view_id, ordered))
}

async fn restore_sets(
    mut views: Signal<Vec<ExerciseView>>,
    view_id: &str,
    error: Signal<RecordErrors>,
) {
    let Ok(storage) = storage::current().await else {
        set_load_error(error, Key::RecordSetSaveFailed);
        return;
    };
    let Ok(stored) = storage.training_sets(view_id).await else {
        set_load_error(error, Key::RecordSetSaveFailed);
        return;
    };
    let sets = stored
        .into_iter()
        .filter_map(SetView::from_stored)
        .collect::<Vec<_>>();
    let mut list = (views)();
    if let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) {
        view.sets = sets;
    }
    views.set(list);
}

/// 選んだ種目と候補の種目をスーパーセットの組にする。
///
/// 既に組に属する種目からは新しい組を作らない。
pub(super) async fn save_superset_group(
    training_id: String,
    view_id: String,
    candidate_ids: Vec<String>,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
) -> bool {
    let Some(view) = (views)()
        .iter()
        .find(|view| view.record.id == view_id)
        .cloned()
    else {
        return false;
    };
    if view.superset.is_some() {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    }
    let mut members = vec![view.record.id.clone()];
    for candidate in candidate_ids {
        if !members.contains(&candidate) {
            members.push(candidate);
        }
    }
    let Ok(members) = SupersetMembers::new(members) else {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let Ok(storage) = storage::current().await else {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let position = storage
        .training_supersets(&training_id)
        .await
        .map(|groups| groups.iter().map(|group| group.position).max().unwrap_or(0))
        .unwrap_or(0)
        .saturating_add(1);
    let (record, operation) = match crate::superset::create(&training_id, position, &members) {
        Ok(rows) => rows,
        Err(_) => {
            set_load_error(error, Key::RecordSupersetSaveFailed);
            return false;
        }
    };
    if storage
        .save_superset_create(&record, &operation)
        .await
        .is_err()
    {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    }
    sync::flush_in_background();
    true
}

/// スーパーセットの組を解散する。
pub(super) async fn dissolve_superset(
    training_id: String,
    superset_id: String,
    error: Signal<RecordErrors>,
) -> bool {
    let Ok(storage) = storage::current().await else {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let operation = match crate::superset::delete(&training_id, &superset_id) {
        Ok(operation) => operation,
        Err(_) => {
            set_load_error(error, Key::RecordSupersetSaveFailed);
            return false;
        }
    };
    if storage
        .save_superset_delete(&training_id, &superset_id, &operation)
        .await
        .is_err()
    {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    }
    sync::flush_in_background();
    true
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

fn find_view_and_set(
    views: Signal<Vec<ExerciseView>>,
    set_id: &str,
) -> Option<(ExerciseView, SetView)> {
    for view in (views)() {
        if let Some(set) = view.sets.iter().find(|set| set.record.id == set_id) {
            return Some((view.clone(), set.clone()));
        }
    }
    None
}

/// セットの操作を、種類ごとの非同期の処理へ振り分ける。
pub(super) fn dispatch(
    action: SetAction,
    training_id: String,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
    saver: SetSaver,
    completed_events: SetCompletedEvents,
) {
    match action {
        SetAction::Add { view_id } => {
            spawn(add_set(training_id, view_id, views, error));
        }
        SetAction::Copy { view_id } => {
            spawn(copy_sets(training_id, view_id, views, error));
        }
        SetAction::Input {
            set_id,
            segment_id,
            field,
            value,
        } => {
            update_set_input(views, &set_id, &segment_id, field, value);
            spawn(save_set_loop(training_id, set_id, views, error, saver));
        }
        SetAction::Rpe { set_id, value } => {
            update_set_rpe(views, &set_id, value);
            spawn(save_set_loop(training_id, set_id, views, error, saver));
        }
        SetAction::Kind { set_id, kind } => {
            if update_set_kind(views, &set_id, kind, error) {
                spawn(save_set_loop(training_id, set_id, views, error, saver));
            }
        }
        SetAction::AddSegment { set_id } => {
            if append_segment(views, &set_id, error) {
                spawn(save_set_loop(training_id, set_id, views, error, saver));
            }
        }
        SetAction::DeleteSegment { set_id, segment_id } => {
            spawn(delete_segment(
                training_id,
                set_id,
                segment_id,
                views,
                error,
                saver,
            ));
        }
        SetAction::Complete { set_id } => {
            spawn(complete_set(
                training_id,
                set_id,
                views,
                error,
                completed_events,
            ));
        }
        SetAction::Delete { set_id } => {
            spawn(delete_set(training_id, set_id, views, error));
        }
        SetAction::Move { set_id, delta } => {
            spawn(move_set(training_id, set_id, delta, views, error));
        }
    }
}
