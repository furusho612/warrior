//! スーパーセットの組の作成と解散。

use dioxus::prelude::*;
use warrior_core::superset::SupersetMembers;

use super::super::errors::{set_load_error, RecordErrors};
use super::view::{ExerciseView, SupersetGroupView};
use crate::storage;
use crate::sync;
use crate::text::Key;

/// 選んだ種目と候補の種目をスーパーセットの組にする。
///
/// 既に組に属する種目からは新しい組を作らない。
pub(in crate::screens::record) async fn save_superset_group(
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
    // 組の並びは選択の順ではなく、種目の並びに従う。
    let ordered = (views)()
        .iter()
        .map(|view| view.record.id.clone())
        .filter(|id| members.contains(id))
        .collect::<Vec<_>>();
    let Ok(members) = SupersetMembers::new(ordered) else {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let Ok(storage) = storage::current().await else {
        set_load_error(error, Key::RecordSupersetSaveFailed);
        return false;
    };
    let position = match storage.training_supersets(&training_id).await {
        Ok(groups) => groups
            .iter()
            .map(|group| group.position)
            .max()
            .unwrap_or(0)
            .saturating_add(1),
        Err(_) => {
            set_load_error(error, Key::RecordSupersetSaveFailed);
            return false;
        }
    };
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
    apply_superset_group(
        views,
        SupersetGroupView {
            id: record.id,
            member_ids: members.as_slice().to_vec(),
        },
    );
    sync::flush_in_background();
    true
}

/// スーパーセットの組を解散する。
pub(in crate::screens::record) async fn dissolve_superset(
    training_id: String,
    superset_id: String,
    views: Signal<Vec<ExerciseView>>,
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
    clear_superset_group(views, &superset_id);
    sync::flush_in_background();
    true
}

/// 組の作成を画面へ反映する。
fn apply_superset_group(mut views: Signal<Vec<ExerciseView>>, group: SupersetGroupView) {
    let mut list = (views)();
    for view in &mut list {
        if group.member_ids.contains(&view.record.id) {
            view.superset = Some(group.clone());
        }
    }
    views.set(list);
}

/// 組の解散を画面へ反映する。
fn clear_superset_group(mut views: Signal<Vec<ExerciseView>>, superset_id: &str) {
    let mut list = (views)();
    for view in &mut list {
        if view
            .superset
            .as_ref()
            .is_some_and(|group| group.id == superset_id)
        {
            view.superset = None;
        }
    }
    views.set(list);
}
