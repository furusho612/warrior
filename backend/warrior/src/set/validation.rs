//! 完了したセットの検証。
//!
//! 完了したセットは、すべての段に値を持ち、SetRecord の検証を通る必要がある。
//! 空の段を含む完了済みのセットで、終了のサマリーが 500 になるのを防ぐ。

use warrior_core::exercise::{ExerciseSnapshot, GripSelection};
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::Result;

use super::{parse_rules, set_row, CountRow, ExerciseRules};
use crate::db::Db;
use crate::stored_sets::{segment_from_row, SetSegmentRow, SET_SEGMENT_COLUMNS};

/// 完了したセットのすべての段に値があるか返す。
pub(super) async fn completed_set_has_values(
    db: &Db,
    user_id: &str,
    training_id: &str,
    set_id: &str,
) -> Result<bool> {
    let Some(row) = set_row(db, user_id, training_id, set_id).await? else {
        return Ok(false);
    };
    let rules = parse_rules(&row.snapshot)?;
    let condition = completed_segment_condition(rules);
    let total = count_segments(db, user_id, set_id, "").await?;
    let with_values = count_segments(db, user_id, set_id, &format!("AND ({condition})")).await?;
    Ok(total > 0 && total == with_values)
}

/// 段が完了に必要な値を持つ条件。
fn completed_segment_condition(rules: ExerciseRules) -> &'static str {
    if rules.lateral() {
        return match rules.record_mode {
            RecordMode::Time => "left_duration_seconds > 0 AND right_duration_seconds > 0",
            RecordMode::WeightAndReps => {
                "left_weight_kg IS NOT NULL AND left_reps > 0 \
                 AND right_weight_kg IS NOT NULL AND right_reps > 0"
            }
            RecordMode::BodyweightAndReps => "left_reps > 0 AND right_reps > 0",
        };
    }
    match rules.record_mode {
        RecordMode::Time => "duration_seconds > 0",
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
            "left_weight_kg IS NULL AND left_reps IS NULL \
             AND left_duration_seconds IS NULL AND right_weight_kg IS NULL \
             AND right_reps IS NULL AND right_duration_seconds IS NULL AND reps > 0"
        }
    }
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
            format!(
                "SELECT {SET_SEGMENT_COLUMNS} FROM sets s \
                 JOIN training_exercises te ON te.id = s.training_exercise_id \
                 LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = s.user_id \
                 WHERE s.id = ? AND s.user_id = ? AND te.training_id = ? AND te.user_id = ? \
                 ORDER BY sg.position, sg.id"
            ),
            vec![
                SqlValue::Text(set_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?
        .results::<SetSegmentRow>()?;
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
    let (grip, attachment) = grip
        .map(|grip| grip.comparison_key())
        .unwrap_or((None, None));
    let key = BestKey {
        exercise_id: snapshot.exercise_id.clone(),
        grip,
        attachment,
    };
    let segments = rows
        .iter()
        .filter(|row| row.segment_id.is_some())
        .map(|row| segment_from_row(row, snapshot.laterality, snapshot.record_mode))
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Ok(false);
    }
    if kind != SetKind::Drop && segments.len() != 1 {
        return Ok(false);
    }
    Ok(SetRecord::new(
        key,
        kind,
        segments,
        first.set_rpe,
        None,
        snapshot.record_mode,
    )
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
