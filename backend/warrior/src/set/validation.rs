//! セットの記録方式の検証と、完了に必要な値の判定。
//!
//! 時間で記録する種目は、時間だけを持ち、重量と回数、反動と補助の回数を持たない。
//! 左右別の種目は左右それぞれの値を持ち、左右の両方が揃ったときだけ完了できる。
//! 判定は保存する種目の snapshot の記録方式と左右別の区分で切り替える。

use serde::Deserialize;
use warrior_core::exercise::{ExerciseSnapshot, Laterality, RecordMode};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{SetSegmentPayload, ValidatedOperation, ValidatedPayload};
use worker::{Response, Result};

use crate::db::Db;
use crate::error::{response, ErrorCode};

/// 種目の記録方式と左右別の区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ExerciseShape {
    /// 記録方式。
    pub(super) mode: RecordMode,
    /// 左右別の区分。
    pub(super) laterality: Laterality,
}

/// 段のある値の件数。
#[derive(Debug, Deserialize)]
struct SetValueRow {
    /// 段の件数。
    segments: i64,
    /// 完了に必要な値を持つ段の件数。
    ready: i64,
}

/// 種目の記録方式を読むための snapshot。
#[derive(Debug, Deserialize)]
struct ExerciseShapeRow {
    /// 記録時点の種目の内容の JSON。
    snapshot: String,
}

/// 種目の記録方式と左右別の区分を返す。種目がトレーニングに無いときは None。
pub(super) async fn exercise_shape(
    db: &Db,
    user_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Result<Option<ExerciseShape>> {
    let row = db
        .first::<ExerciseShapeRow>(&SqlStatement::new(
            "SELECT snapshot FROM training_exercises \
             WHERE id = ? AND user_id = ? AND training_id = ?",
            vec![
                SqlValue::Text(training_exercise_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot).map_err(|error| {
        worker::Error::RustError(format!("the exercise snapshot is invalid: {error}"))
    })?;
    Ok(Some(ExerciseShape {
        mode: snapshot.record_mode,
        laterality: snapshot.laterality,
    }))
}

/// 操作の対象の種目の記録方式と左右別の区分を返す。段の値を作らない操作は None。
pub(super) async fn payload_exercise_shape(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
) -> Result<Option<ExerciseShape>> {
    let training_id = &validated.operation.training_id;
    match &validated.payload {
        ValidatedPayload::SetCreate(payload) => {
            exercise_shape(db, user_id, training_id, &payload.training_exercise_id).await
        }
        ValidatedPayload::SetUpdate(payload) => {
            let Some(row) = super::set_row(db, user_id, training_id, &payload.set_id).await? else {
                return Ok(None);
            };
            exercise_shape(db, user_id, training_id, &row.training_exercise_id).await
        }
        ValidatedPayload::SetComplete(payload) => {
            let Some(row) = super::set_row(db, user_id, training_id, &payload.set_id).await? else {
                return Ok(None);
            };
            exercise_shape(db, user_id, training_id, &row.training_exercise_id).await
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_) => Ok(None),
    }
}

/// 時間種目に重量、回数、反動、補助の回数が入っているか。
pub(super) fn has_non_time_values(segments: &[SetSegmentPayload]) -> bool {
    segments.iter().any(|segment| {
        segment.weight_kg != 0.0
            || segment.reps != 0
            || segment.momentum_reps != 0
            || segment.assisted_reps != 0
            || [segment.left.as_ref(), segment.right.as_ref()]
                .into_iter()
                .flatten()
                .any(|side| side.weight_kg != 0.0 || side.reps != 0)
    })
}

/// 時間種目に重量、回数、反動、補助の回数を保存しようとしたときの応答。
pub(super) fn time_value_rejection() -> Response {
    response(
        ErrorCode::BadRequest,
        "a time set cannot store weight, reps, momentum, or assisted reps",
    )
}

/// 完了に必要な値を持つ段を数える CASE 式。
pub(super) fn readiness_case(shape: ExerciseShape) -> &'static str {
    match (shape.mode, shape.laterality) {
        (RecordMode::Time, Laterality::Bilateral) => {
            "CASE WHEN \
              (left_weight_kg IS NULL AND left_reps IS NULL \
               AND left_duration_seconds IS NULL AND right_weight_kg IS NULL \
               AND right_reps IS NULL AND right_duration_seconds IS NULL) \
              AND weight_kg IS NULL AND reps IS NULL AND momentum_reps = 0 \
              AND assisted_reps = 0 AND duration_seconds > 0 THEN 1 ELSE 0 END"
        }
        (RecordMode::Time, Laterality::Unilateral) => {
            "CASE WHEN \
              left_duration_seconds > 0 AND right_duration_seconds > 0 \
              AND left_weight_kg IS NULL AND left_reps IS NULL \
              AND right_weight_kg IS NULL AND right_reps IS NULL \
              AND weight_kg IS NULL AND reps IS NULL AND duration_seconds IS NULL \
              AND momentum_reps = 0 AND assisted_reps = 0 THEN 1 ELSE 0 END"
        }
        (RecordMode::WeightAndReps | RecordMode::BodyweightAndReps, Laterality::Bilateral) => {
            "CASE WHEN \
              (left_weight_kg IS NULL AND left_reps IS NULL \
               AND left_duration_seconds IS NULL AND right_weight_kg IS NULL \
               AND right_reps IS NULL AND right_duration_seconds IS NULL) \
              AND reps > 0 THEN 1 ELSE 0 END"
        }
        (RecordMode::WeightAndReps | RecordMode::BodyweightAndReps, Laterality::Unilateral) => {
            "CASE WHEN left_reps > 0 AND right_reps > 0 THEN 1 ELSE 0 END"
        }
    }
}

/// 保存済みのセットが完了に必要な値を持つか。
pub(super) async fn completed_set_has_values(
    db: &Db,
    user_id: &str,
    set_id: &str,
    shape: ExerciseShape,
) -> Result<bool> {
    let sql = format!(
        "SELECT COUNT(*) AS segments, \
         COALESCE(SUM({}), 0) AS ready \
         FROM set_segments WHERE set_id = ? AND user_id = ?",
        readiness_case(shape)
    );
    let row = db
        .first::<SetValueRow>(&SqlStatement::new(
            sql,
            vec![
                SqlValue::Text(set_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?;
    Ok(row.is_some_and(|row| row.segments > 0 && row.ready == row.segments))
}

/// 保存する段がすべて完了に必要な値を持つか。
pub(super) fn segments_are_complete(segments: &[SetSegmentPayload], shape: ExerciseShape) -> bool {
    !segments.is_empty()
        && segments
            .iter()
            .all(|segment| match (shape.mode, shape.laterality) {
                (RecordMode::Time, Laterality::Bilateral) => {
                    segment.left.is_none()
                        && segment.right.is_none()
                        && segment.is_complete(RecordMode::Time)
                }
                (RecordMode::Time, Laterality::Unilateral) => {
                    segment.duration_seconds.is_none()
                        && segment.left.is_some()
                        && segment.right.is_some()
                        && segment.is_complete(RecordMode::Time)
                }
                (
                    RecordMode::WeightAndReps | RecordMode::BodyweightAndReps,
                    Laterality::Bilateral,
                ) => {
                    segment.left.is_none()
                        && segment.right.is_none()
                        && segment.is_complete(shape.mode)
                }
                (
                    RecordMode::WeightAndReps | RecordMode::BodyweightAndReps,
                    Laterality::Unilateral,
                ) => {
                    segment.left.is_some()
                        && segment.right.is_some()
                        && segment.is_complete(shape.mode)
                }
            })
}
