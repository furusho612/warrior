//! セットのグリップの変更と、その種目の前回の記録の取り直し。

use dioxus::prelude::*;
use warrior_core::exercise::GripSelection;

use super::super::view::ExerciseView;
use super::set_io::save_set_loop;
use super::SetSaver;
use crate::previous as previous_api;
use crate::screens::record::errors::RecordErrors;
use crate::screens::record::sets::set_grip_is;

/// セットのグリップを画面へ反映し、その種目の前回を取り直して保存する。
pub(super) async fn change_grip(
    training_id: String,
    set_id: String,
    grip: GripSelection,
    views: Signal<Vec<ExerciseView>>,
    error: Signal<RecordErrors>,
    saver: SetSaver,
) {
    let Some((view_id, exercise_id)) = update_set_grip(views, &set_id, grip) else {
        return;
    };
    let save_finished = saver.clone();
    let saved = save_set_loop(training_id.clone(), set_id.clone(), views, error, saver).await;
    if !saved {
        save_finished.wait_finished(&set_id).await;
        if (error)().sets.contains_key(&set_id) {
            return;
        }
    }
    refresh_previous(&training_id, &view_id, &set_id, &exercise_id, grip, views).await;
}

/// セットのグリップを画面へ反映する。
fn update_set_grip(
    mut views: Signal<Vec<ExerciseView>>,
    set_id: &str,
    grip: GripSelection,
) -> Option<(String, String)> {
    let mut list = (views)();
    let (view_id, exercise_id) = {
        let view = list
            .iter_mut()
            .find(|view| view.sets.iter().any(|set| set.record.id == set_id))?;
        let set = view.sets.iter_mut().find(|set| set.record.id == set_id)?;
        set.set_grip(grip);
        (view.record.id.clone(), view.snapshot.exercise_id.clone())
    };
    views.set(list);
    Some((view_id, exercise_id))
}

/// 追加したセットの既定のグリップで前回を取り直す。
pub(super) async fn refresh_after_add(
    training_id: &str,
    view_id: &str,
    set_id: &str,
    view: &ExerciseView,
    grip: GripSelection,
    views: Signal<Vec<ExerciseView>>,
) {
    refresh_previous(
        training_id,
        view_id,
        set_id,
        &view.snapshot.exercise_id,
        grip,
        views,
    )
    .await;
}

/// 種目の前回の記録を、選んだグリップで取り直す。
pub(super) async fn refresh_previous(
    training_id: &str,
    view_id: &str,
    set_id: &str,
    exercise_id: &str,
    grip: GripSelection,
    mut views: Signal<Vec<ExerciseView>>,
) {
    let expected = grip;
    let (grip, attachment) = previous_api::grip_query(Some(grip));
    let (previous, previous_error) = match previous_api::fetch(
        training_id,
        exercise_id,
        grip.as_deref(),
        attachment.as_deref(),
    )
    .await
    {
        Ok(response) => (response.exercise, false),
        Err(_) => (None, true),
    };
    let mut list = (views)();
    let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) else {
        return;
    };
    if !set_grip_is(&view.sets, set_id, expected) {
        return;
    }
    view.previous = previous;
    view.previous_error = previous_error;
    views.set(list);
}
