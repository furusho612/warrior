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
use warrior_core::superset::SupersetMembers;
use warrior_core::training_record::TrainingExerciseRecord;

use super::set_events::SetCompletedEvents;
use super::set_save::{complete_set, save_set_loop};
use super::sets::{
    kind_allowed, move_allowed, previous_values, Field, SegmentView, SetAction, SetView,
};
use crate::ids::{new_uuid, now_iso};
use crate::storage;
use crate::sync;
use crate::text::{text, Key};
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

pub(super) fn is_lateral(snapshot: &ExerciseSnapshot) -> bool {
    snapshot.laterality == Laterality::Unilateral
}

/// セットの保存が同時に走らないようにする印。
#[derive(Clone, Default)]
pub(super) struct SetSaver {
    in_flight: Rc<RefCell<HashSet<String>>>,
}

impl SetSaver {
    pub(super) fn start(&self, set_id: &str) -> bool {
        self.in_flight.borrow_mut().insert(set_id.to_owned())
    }

    pub(super) fn finish(&self, set_id: &str) {
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
    let mode = view.snapshot.record_mode;
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
        Field::LeftDuration => segment.left_duration = value,
        Field::RightWeight => segment.right_weight = value,
        Field::RightReps => segment.right_reps = value,
        Field::RightDuration => segment.right_duration = value,
    }
    if set.completed() && !set.values(lateral, mode).is_complete() {
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
    error: Signal<Option<String>>,
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
        set_error(error, Key::RecordSetDropInvalid);
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
    error: Signal<Option<String>>,
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
        set_error(error, Key::RecordSetSaveFailed);
        return false;
    };
    let now = now_iso();
    let side = lateral.then_some(SegmentSideValues {
        weight_kg: 0.0,
        reps: 0,
        duration_seconds: None,
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
        left_duration: String::new(),
        right_weight: String::new(),
        right_reps: String::new(),
        right_duration: String::new(),
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
    error: Signal<Option<String>>,
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
    error: Signal<Option<String>>,
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
        duration_seconds: None,
    });
    let values = SetValues {
        kind: SetKind::Normal,
        mode: view.snapshot.record_mode,
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
            set_error(error, Key::RecordSetSaveFailed);
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
        set_error(error, Key::RecordSetSaveFailed);
        return;
    }
    sync::flush_in_background();
}

/// 前回の値をセットの行としてコピーする。
pub(super) async fn copy_sets(
    training_id: String,
    view_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
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
            set_error(error, Key::RecordSetSaveFailed);
            return;
        }
    };
    for set in &view.sets {
        let operation = match training_set::delete(&training_id, &set.record.id) {
            Ok(operation) => operation,
            Err(_) => {
                set_error(error, Key::RecordSetSaveFailed);
                return;
            }
        };
        if storage
            .save_set_delete(&training_id, &set.record.id, &operation)
            .await
            .is_err()
        {
            set_error(error, Key::RecordSetSaveFailed);
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
                set_error(error, Key::RecordSetSaveFailed);
                restore_sets(views, &view_id, error).await;
                return;
            }
        };
        if storage
            .save_set_create(&rows.set, &rows.segments, &operation)
            .await
            .is_err()
        {
            set_error(error, Key::RecordSetSaveFailed);
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

/// セットを画面から消し、端末内の行と送信待ちの列から削除する。
pub(super) async fn delete_set(
    training_id: String,
    set_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
) {
    let Some((view, _set)) = find_view_and_set(views, &set_id) else {
        return;
    };
    let operation = match training_set::delete(&training_id, &set_id) {
        Ok(operation) => operation,
        Err(_) => {
            set_error(error, Key::RecordSetSaveFailed);
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
        set_error(error, Key::RecordSetSaveFailed);
        restore_sets(views, &view.record.id, error).await;
        return;
    }
    sync::flush_in_background();
}

/// セットの順序を変え、端末内の位置と送信待ちの列へ保存する。
pub(super) async fn move_set(
    training_id: String,
    set_id: String,
    delta: i32,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
) {
    let Some((view_id, ordered)) = move_set_order(views, &set_id, delta, error) else {
        return;
    };
    let operation = match training_set::reorder(&training_id, &view_id, &ordered) {
        Ok(operation) => operation,
        Err(_) => {
            set_error(error, Key::RecordSetSaveFailed);
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
        set_error(error, Key::RecordSetSaveFailed);
        restore_sets(views, &view_id, error).await;
        return;
    }
    sync::flush_in_background();
}

fn move_set_order(
    mut views: Signal<Vec<ExerciseView>>,
    set_id: &str,
    delta: i32,
    error: Signal<Option<String>>,
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
            set_error(error, Key::RecordSetDropInvalid);
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
    mut error: Signal<Option<String>>,
) {
    let Ok(storage) = storage::current().await else {
        set_error(error, Key::RecordSetSaveFailed);
        return;
    };
    let Ok(stored) = storage.training_sets(view_id).await else {
        set_error(error, Key::RecordSetSaveFailed);
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
    error.set(None);
}

/// 選んだ種目と候補の種目をスーパーセットの組にする。
///
/// 既に組に属する種目からは新しい組を作らない。
pub(super) async fn save_superset_group(
    training_id: String,
    view_id: String,
    candidate_ids: Vec<String>,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
) -> bool {
    let Some(view) = (views)()
        .iter()
        .find(|view| view.record.id == view_id)
        .cloned()
    else {
        return false;
    };
    if view.superset.is_some() {
        set_error(error, Key::RecordSupersetSaveFailed);
        return false;
    }
    let mut members = vec![view.record.id.clone()];
    for candidate in candidate_ids {
        if !members.contains(&candidate) {
            members.push(candidate);
        }
    }
    let Ok(members) = SupersetMembers::new(members) else {
        set_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let Ok(storage) = storage::current().await else {
        set_error(error, Key::RecordSupersetSaveFailed);
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
            set_error(error, Key::RecordSupersetSaveFailed);
            return false;
        }
    };
    if storage
        .save_superset_create(&record, &operation)
        .await
        .is_err()
    {
        set_error(error, Key::RecordSupersetSaveFailed);
        return false;
    }
    sync::flush_in_background();
    true
}

/// スーパーセットの組を解散する。
pub(super) async fn dissolve_superset(
    training_id: String,
    superset_id: String,
    error: Signal<Option<String>>,
) -> bool {
    let Ok(storage) = storage::current().await else {
        set_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let operation = match crate::superset::delete(&training_id, &superset_id) {
        Ok(operation) => operation,
        Err(_) => {
            set_error(error, Key::RecordSupersetSaveFailed);
            return false;
        }
    };
    if storage
        .save_superset_delete(&training_id, &superset_id, &operation)
        .await
        .is_err()
    {
        set_error(error, Key::RecordSupersetSaveFailed);
        return false;
    }
    sync::flush_in_background();
    true
}

pub(super) fn find_view_and_set(
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

pub(super) fn set_error(mut error: Signal<Option<String>>, key: Key) {
    error.set(Some(text(key).to_owned()));
}

/// セットの操作を、種類ごとの非同期の処理へ振り分ける。
pub(super) fn dispatch(
    action: SetAction,
    training_id: String,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<Option<String>>,
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
