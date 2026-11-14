//! D1 に保存した種目、セット、段を、検証済みのセットの記録に組み立てる。
//!
//! トレーニングのサマリーとカレンダーの日の詳細は、このモジュールが返す記録だけを使う。
//! 総重量などの計算は `set_calc` に任せ、ここでは行から記録への変換だけを行う。
//! スナップショットは記録した時点の種目の内容を持つ (ADR-006)。

use serde::Deserialize;
use warrior_core::exercise::{ExerciseSnapshot, GripSelection, Laterality};
use warrior_core::set_calc::{
    BestKey, SetKind, SetRecord, SetSegment, SetValidationError, SideValue,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::SessionSet;
use worker::{Error, Result};

use crate::db::Db;

/// 種目、セット、段を結合した 1 行。
#[derive(Debug, Deserialize)]
struct SetSegmentRow {
    training_id: String,
    training_exercise_id: String,
    snapshot: String,
    set_id: String,
    kind: String,
    set_rpe: Option<f64>,
    grip: Option<String>,
    segment_id: Option<String>,
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

/// トレーニング 1 つ分の種目と、その種目のセットの記録。
#[derive(Debug, Clone, PartialEq)]
pub struct StoredExercise {
    /// トレーニングの ID。
    pub training_id: String,
    /// 記録した時点の種目の内容。
    pub snapshot: ExerciseSnapshot,
    /// 完了したセットの記録。
    pub sets: Vec<SetRecord>,
}

const SET_SEGMENT_COLUMNS: &str = "te.training_id AS training_id, \
     te.id AS training_exercise_id, te.snapshot, \
     s.id AS set_id, s.kind AS kind, s.rpe AS set_rpe, s.grip AS grip, \
     sg.id AS segment_id, \
     sg.weight_kg, sg.reps, sg.momentum_reps, sg.assisted_reps, sg.duration_seconds, \
     sg.left_weight_kg, sg.left_reps, sg.left_duration_seconds, \
     sg.right_weight_kg, sg.right_reps, sg.right_duration_seconds";

/// トレーニング 1 つの種目とセットを読む。
pub async fn load_training(
    db: &Db,
    user_id: &str,
    training_id: &str,
) -> Result<Vec<StoredExercise>> {
    let statement = SqlStatement::new(
        format!(
            "SELECT {SET_SEGMENT_COLUMNS} FROM training_exercises te \
             JOIN sets s ON s.training_exercise_id = te.id AND s.completed = 1 \
             LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = ? \
             WHERE te.user_id = ? AND te.training_id = ? AND s.user_id = ? \
             ORDER BY te.position, s.position, sg.position"
        ),
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    );
    let rows = db.all(&statement).await?.results::<SetSegmentRow>()?;
    build_exercises(rows).map_err(invalid_stored_data)
}

/// 日 1 日のトレーニングの種目とセットを読む。
pub async fn load_day(db: &Db, user_id: &str, date: &str) -> Result<Vec<StoredExercise>> {
    let statement = SqlStatement::new(
        format!(
            "SELECT {SET_SEGMENT_COLUMNS} FROM training_sessions ts \
             JOIN training_exercises te ON te.training_id = ts.id AND te.user_id = ts.user_id \
             JOIN sets s ON s.training_exercise_id = te.id AND s.completed = 1 \
              AND s.user_id = ts.user_id \
             LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = ts.user_id \
             WHERE ts.user_id = ? AND ts.date = ? \
             ORDER BY te.position, s.position, sg.position"
        ),
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(date.to_owned()),
        ],
    );
    let rows = db.all(&statement).await?.results::<SetSegmentRow>()?;
    build_exercises(rows).map_err(invalid_stored_data)
}

/// サマリーの計算に渡すセットの列にする。
pub fn session_sets(exercises: &[StoredExercise]) -> Vec<SessionSet> {
    let mut sets = Vec::new();
    for exercise in exercises {
        for record in &exercise.sets {
            sets.push(SessionSet {
                record: record.clone(),
                exercise_name: exercise.snapshot.name.clone(),
                laterality: exercise.snapshot.laterality,
            });
        }
    }
    sets
}

fn invalid_stored_data(error: BuildSetsError) -> Error {
    Error::RustError(format!("the stored sets are invalid: {error:?}"))
}

/// 保存した行を、セットの記録に変換できない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildSetsError {
    /// 種目のスナップショットを復元できない。
    UnreadableSnapshot,
    /// セット種別の文字列が既知の値でない。
    UnknownSetKind,
    /// グリップの JSON を復元できない。
    InvalidGrip,
    /// 完了したセットに段が無い。
    MissingSegments,
    /// セットが計算規則の検証を通らない。
    InvalidSetRecord(SetValidationError),
}

struct ExerciseGroup {
    training_exercise_id: String,
    snapshot: ExerciseSnapshot,
    sets: Vec<StoredSet>,
}

struct StoredSet {
    id: String,
    kind: String,
    rpe: Option<f64>,
    grip: Option<String>,
    segments: Vec<SetSegment>,
}

fn build_exercises(
    rows: Vec<SetSegmentRow>,
) -> std::result::Result<Vec<StoredExercise>, BuildSetsError> {
    let mut groups = Vec::<(String, Vec<SetSegmentRow>)>::new();
    for row in rows {
        let index = match groups
            .iter()
            .position(|(training_id, _)| *training_id == row.training_id)
        {
            Some(index) => index,
            None => {
                groups.push((row.training_id.clone(), Vec::new()));
                groups.len() - 1
            }
        };
        groups[index].1.push(row);
    }
    let mut exercises = Vec::new();
    for (training_id, rows) in groups {
        exercises.extend(build_training(&training_id, rows)?);
    }
    Ok(exercises)
}

fn build_training(
    training_id: &str,
    rows: Vec<SetSegmentRow>,
) -> std::result::Result<Vec<StoredExercise>, BuildSetsError> {
    let mut groups = Vec::<ExerciseGroup>::new();
    for row in rows {
        let index = match groups
            .iter()
            .position(|group| group.training_exercise_id == row.training_exercise_id)
        {
            Some(index) => index,
            None => {
                let snapshot = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot)
                    .map_err(|_| BuildSetsError::UnreadableSnapshot)?;
                groups.push(ExerciseGroup {
                    training_exercise_id: row.training_exercise_id.clone(),
                    snapshot,
                    sets: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        let segment = row
            .segment_id
            .as_ref()
            .map(|_| segment_from_row(&row, group.snapshot.laterality));
        match group.sets.iter_mut().find(|set| set.id == row.set_id) {
            Some(set) => {
                if let Some(segment) = segment {
                    set.segments.push(segment);
                }
            }
            None => {
                let mut segments = Vec::new();
                if let Some(segment) = segment {
                    segments.push(segment);
                }
                group.sets.push(StoredSet {
                    id: row.set_id.clone(),
                    kind: row.kind.clone(),
                    rpe: row.set_rpe,
                    grip: row.grip.clone(),
                    segments,
                });
            }
        }
    }

    let mut exercises = Vec::new();
    for group in groups {
        let mut sets = Vec::new();
        for set in group.sets {
            if set.segments.is_empty() {
                return Err(BuildSetsError::MissingSegments);
            }
            let grip = parse_grip(set.grip.as_deref())?;
            let (grip, attachment) = grip.comparison_key();
            let key = BestKey {
                exercise_id: group.snapshot.exercise_id.clone(),
                grip,
                attachment,
            };
            let kind = parse_set_kind(&set.kind).ok_or(BuildSetsError::UnknownSetKind)?;
            let record = SetRecord::new(
                key,
                kind,
                set.segments,
                set.rpe,
                None,
                group.snapshot.record_mode,
            )
            .map_err(BuildSetsError::InvalidSetRecord)?;
            sets.push(record);
        }
        exercises.push(StoredExercise {
            training_id: training_id.to_owned(),
            snapshot: group.snapshot,
            sets,
        });
    }
    Ok(exercises)
}

fn parse_grip(text: Option<&str>) -> std::result::Result<GripSelection, BuildSetsError> {
    match text {
        None => Ok(GripSelection::None),
        Some(text) => serde_json::from_str(text).map_err(|_| BuildSetsError::InvalidGrip),
    }
}

fn parse_set_kind(text: &str) -> Option<SetKind> {
    SetKind::parse(text)
}

fn segment_from_row(row: &SetSegmentRow, laterality: Laterality) -> SetSegment {
    match laterality {
        Laterality::Unilateral => SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: to_u32(row.momentum_reps),
            assisted_reps: to_u32(row.assisted_reps),
            left: Some(side_value(
                row.left_weight_kg,
                row.left_reps,
                row.left_duration_seconds,
            )),
            right: Some(side_value(
                row.right_weight_kg,
                row.right_reps,
                row.right_duration_seconds,
            )),
            duration_seconds: None,
        },
        Laterality::Bilateral => SetSegment {
            weight_kg: row.weight_kg.unwrap_or(0.0),
            reps: to_u32(row.reps),
            momentum_reps: to_u32(row.momentum_reps),
            assisted_reps: to_u32(row.assisted_reps),
            left: None,
            right: None,
            duration_seconds: row
                .duration_seconds
                .and_then(|value| u32::try_from(value).ok()),
        },
    }
}

fn side_value(
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> SideValue {
    SideValue {
        weight_kg: weight_kg.unwrap_or(0.0),
        reps: to_u32(reps),
        duration_seconds: duration_seconds.and_then(|value| u32::try_from(value).ok()),
    }
}

fn to_u32(value: Option<i64>) -> u32 {
    value
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! 保存した行からセットの記録への変換の例示テスト。
    //!
    //! 要求の受入基準は結合テストと E2E が担い、ここは変換の失敗と並びを確かめる。

    use serde_json::json;
    use warrior_core::exercise::{BodyPart, Laterality};
    use warrior_core::set_calc::{RecordMode, SetKind};

    use super::{build_exercises, session_sets, BuildSetsError, SetSegmentRow};

    #[test]
    fn the_stored_rows_become_the_weight_and_time_sets() {
        let rows = vec![
            segment_row(
                "training-a",
                "exercise-weight",
                "set-warmup",
                "warmup",
                Some(60.0),
                Some(10),
                None,
            ),
            segment_row(
                "training-a",
                "exercise-weight",
                "set-normal",
                "normal",
                Some(100.0),
                Some(5),
                None,
            ),
            segment_row(
                "training-a",
                "exercise-weight",
                "set-drop",
                "drop",
                Some(100.0),
                Some(8),
                None,
            ),
            segment_row(
                "training-a",
                "exercise-weight",
                "set-drop",
                "drop",
                Some(80.0),
                Some(6),
                None,
            ),
            segment_row(
                "training-a",
                "exercise-time",
                "set-time",
                "normal",
                None,
                None,
                Some(90),
            ),
        ];
        let exercises = build_exercises(rows).expect("the stored sets are valid");
        assert_eq!(exercises.len(), 2);
        assert_eq!(exercises[0].training_id, "training-a");
        assert_eq!(exercises[0].snapshot.body_part, BodyPart::Chest);
        let sets = session_sets(&exercises);
        assert_eq!(sets.len(), 4);
        assert_eq!(sets[0].record.kind(), SetKind::Warmup);
        assert_eq!(sets[0].record.total_weight_kg(), 0.0);
        assert_eq!(sets[1].record.mode(), RecordMode::WeightAndReps);
        assert_eq!(sets[1].record.total_weight_kg(), 500.0);
        assert_eq!(sets[2].record.kind(), SetKind::Drop);
        assert_eq!(sets[2].record.segments().len(), 2);
        assert_eq!(sets[2].record.total_weight_kg(), 1_280.0);
        assert_eq!(sets[3].record.mode(), RecordMode::Time);
        assert_eq!(sets[3].record.longest_duration_seconds(), Some(90));
        assert_eq!(sets[1].exercise_name, "exercise-weight");
        assert_eq!(sets[1].laterality, Laterality::Bilateral);
    }

    #[test]
    fn the_rows_of_a_day_keep_each_training_apart() {
        let rows = vec![
            segment_row(
                "training-b",
                "exercise-b",
                "set-b",
                "normal",
                Some(40.0),
                Some(10),
                None,
            ),
            segment_row(
                "training-a",
                "exercise-a",
                "set-a",
                "normal",
                Some(80.0),
                Some(8),
                None,
            ),
        ];
        let exercises = build_exercises(rows).expect("the stored sets are valid");
        assert_eq!(
            exercises
                .iter()
                .map(|exercise| exercise.training_id.as_str())
                .collect::<Vec<_>>(),
            vec!["training-b", "training-a"]
        );
        assert_eq!(exercises[0].sets[0].total_weight_kg(), 400.0);
        assert_eq!(exercises[1].sets[0].total_weight_kg(), 640.0);
    }

    #[test]
    fn an_unknown_set_kind_is_rejected() {
        let rows = vec![segment_row(
            "training-a",
            "exercise-weight",
            "set-normal",
            "unknown",
            Some(100.0),
            Some(5),
            None,
        )];
        assert_eq!(build_exercises(rows), Err(BuildSetsError::UnknownSetKind));
    }

    #[test]
    fn a_completed_set_without_segments_is_rejected() {
        let mut row = segment_row(
            "training-a",
            "exercise-weight",
            "set-normal",
            "normal",
            Some(100.0),
            Some(5),
            None,
        );
        row.segment_id = None;
        assert_eq!(
            build_exercises(vec![row]),
            Err(BuildSetsError::MissingSegments)
        );
    }

    #[test]
    fn an_invalid_grip_json_is_rejected() {
        let mut row = segment_row(
            "training-a",
            "exercise-weight",
            "set-normal",
            "normal",
            Some(100.0),
            Some(5),
            None,
        );
        row.grip = Some("not-json".to_owned());
        assert_eq!(build_exercises(vec![row]), Err(BuildSetsError::InvalidGrip));
    }

    #[test]
    fn an_unreadable_snapshot_is_rejected() {
        let mut row = segment_row(
            "training-a",
            "exercise-weight",
            "set-normal",
            "normal",
            Some(100.0),
            Some(5),
            None,
        );
        row.snapshot = "not-json".to_owned();
        assert_eq!(
            build_exercises(vec![row]),
            Err(BuildSetsError::UnreadableSnapshot)
        );
    }

    fn segment_row(
        training_id: &str,
        exercise_id: &str,
        set_id: &str,
        kind: &str,
        weight_kg: Option<f64>,
        reps: Option<i64>,
        duration_seconds: Option<i64>,
    ) -> SetSegmentRow {
        let record_mode = if duration_seconds.is_some() {
            "time"
        } else {
            "weight_and_reps"
        };
        let snapshot = json!({
            "exercise_id": exercise_id,
            "name": exercise_id,
            "body_part": "chest",
            "muscles": [],
            "record_mode": record_mode,
            "laterality": "bilateral",
            "grip_mode": "none",
            "grip_options": []
        });
        SetSegmentRow {
            training_id: training_id.to_owned(),
            training_exercise_id: exercise_id.to_owned(),
            snapshot: snapshot.to_string(),
            set_id: set_id.to_owned(),
            kind: kind.to_owned(),
            set_rpe: None,
            grip: None,
            segment_id: Some(format!("{set_id}-segment")),
            weight_kg,
            reps,
            momentum_reps: Some(0),
            assisted_reps: Some(0),
            duration_seconds,
            left_weight_kg: None,
            left_reps: None,
            left_duration_seconds: None,
            right_weight_kg: None,
            right_reps: None,
            right_duration_seconds: None,
        }
    }
}
