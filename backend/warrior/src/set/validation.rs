//! 完了したセットの検証。
//!
//! 完了したセットは、すべての段に値を持ち、SetRecord の検証を通る必要がある。
//! 空の段を含む完了済みのセットで、終了のサマリーが 500 になるのを防ぐ。

use serde::Deserialize;
use warrior_core::exercise::{ExerciseSnapshot, GripSelection, Laterality};
use warrior_core::set_calc::{SetKind, SideValue};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{SetSegmentPayload, SetUpdatePayload};
use worker::Result;

use super::CountRow;
use crate::db::Db;

/// 完了したセットのすべての段に値があるか返す。
pub(super) async fn completed_set_has_values(db: &Db, user_id: &str, set_id: &str) -> Result<bool> {
    let total = count_segments(db, user_id, set_id, "").await?;
    let with_values = count_segments(
        db,
        user_id,
        set_id,
        "AND (reps > 0 OR duration_seconds > 0 \
         OR left_reps > 0 OR right_reps > 0 \
         OR left_duration_seconds > 0 OR right_duration_seconds > 0)",
    )
    .await?;
    Ok(total > 0 && total == with_values)
}

/// 保存済みのセットが SetRecord の検証を通るか返す。
pub(super) async fn completed_set_is_valid(
    db: &Db,
    user_id: &str,
    training_id: &str,
    set_id: &str,
) -> Result<bool> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT s.position, s.kind, s.rpe, s.grip, s.body_weight_kg, te.snapshot, \
             sg.id AS segment_id, sg.position AS segment_position, sg.weight_kg, sg.reps, \
             sg.momentum_reps, sg.assisted_reps, sg.duration_seconds, \
             sg.left_weight_kg, sg.left_reps, sg.left_duration_seconds, \
             sg.right_weight_kg, sg.right_reps, sg.right_duration_seconds \
             FROM sets s \
             JOIN training_exercises te ON te.id = s.training_exercise_id \
             LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = s.user_id \
             WHERE s.id = ? AND s.user_id = ? AND te.training_id = ? AND te.user_id = ? \
             ORDER BY sg.position, sg.id",
            vec![
                SqlValue::Text(set_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?
        .results::<StoredSegmentRow>()?;
    let Some(first) = rows.first() else {
        return Ok(false);
    };
    let Some(kind) = SetKind::parse(&first.kind) else {
        return Ok(false);
    };
    let Ok(snapshot) = serde_json::from_str::<ExerciseSnapshot>(&first.snapshot) else {
        return Ok(false);
    };
    let Ok(grip) = first
        .grip
        .as_deref()
        .map(serde_json::from_str::<GripSelection>)
        .transpose()
    else {
        return Ok(false);
    };
    let segments = rows
        .iter()
        .filter_map(|row| stored_segment_payload(row, snapshot.laterality))
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Ok(false);
    }
    if kind != SetKind::Drop && segments.len() != 1 {
        return Ok(false);
    }
    let payload = SetUpdatePayload {
        training_id: training_id.to_owned(),
        set_id: set_id.to_owned(),
        position: first.position,
        kind,
        completed: true,
        rpe: first.rpe,
        grip,
        body_weight_kg: first.body_weight_kg,
        segments,
    };
    Ok(payload
        .to_record(&snapshot.exercise_id, snapshot.record_mode)
        .is_ok())
}

async fn count_segments(db: &Db, user_id: &str, set_id: &str, condition: &str) -> Result<i64> {
    let count = db
        .first::<CountRow>(&SqlStatement::new(
            format!(
                "SELECT COUNT(*) AS count FROM set_segments \
                 WHERE set_id = ? AND user_id = ? {condition}"
            ),
            vec![
                SqlValue::Text(set_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?
        .map_or(0, |row| row.count);
    Ok(count)
}

#[derive(Debug, Deserialize)]
struct StoredSegmentRow {
    position: i64,
    kind: String,
    rpe: Option<f64>,
    grip: Option<String>,
    body_weight_kg: Option<f64>,
    snapshot: String,
    segment_id: Option<String>,
    segment_position: Option<i64>,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    momentum_reps: Option<i64>,
    assisted_reps: Option<i64>,
    duration_seconds: Option<i64>,
    left_weight_kg: Option<f64>,
    left_reps: Option<i64>,
    left_duration_seconds: Option<i64>,
    right_weight_kg: Option<f64>,
    right_reps: Option<i64>,
    right_duration_seconds: Option<i64>,
}

fn stored_segment_payload(
    row: &StoredSegmentRow,
    laterality: Laterality,
) -> Option<SetSegmentPayload> {
    let (left, right) = match laterality {
        Laterality::Unilateral => (
            stored_side(row.left_weight_kg, row.left_reps, row.left_duration_seconds),
            stored_side(
                row.right_weight_kg,
                row.right_reps,
                row.right_duration_seconds,
            ),
        ),
        Laterality::Bilateral => (None, None),
    };
    Some(SetSegmentPayload {
        segment_id: row.segment_id.clone()?,
        position: row.segment_position.unwrap_or(0),
        weight_kg: row.weight_kg.unwrap_or(0.0),
        reps: stored_count(row.reps),
        momentum_reps: stored_count(row.momentum_reps),
        assisted_reps: stored_count(row.assisted_reps),
        duration_seconds: stored_duration(row.duration_seconds),
        left,
        right,
    })
}

fn stored_side(
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> Option<SideValue> {
    if weight_kg.is_none() && reps.is_none() && duration_seconds.is_none() {
        return None;
    }
    Some(SideValue {
        weight_kg: weight_kg.unwrap_or(0.0),
        reps: stored_count(reps),
        duration_seconds: stored_duration(duration_seconds),
    })
}

fn stored_count(value: Option<i64>) -> u32 {
    value
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

fn stored_duration(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}
