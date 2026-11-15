//! 部位別のセット数と総重量の分析の取得。
//!
//! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
//! 期間の境界と集計は `warrior_core::body_part_analysis` に任せ、このモジュールは
//! 終了済みのトレーニングの読み込み、記録時のスナップショットの部位区分の取り出し、
//! 応答の組み立てを担う (ADR-006、ADR-016)。期間の終わりには、要求を処理する時点の
//! 日本時間の日付を使う (ADR-005)。

use std::collections::HashMap;

use serde::Deserialize;
use warrior_core::body_part_analysis::{
    body_part_analysis, AnalysisMetric, AnalysisPeriod, BodyPartAnalysis, BodyPartSet,
};
use warrior_core::jst::JstDate;
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::history::{current_date, parse_date};
use crate::stored_sets::{self, SetSegmentRow, StoredExercise};

/// 分析の 1 行。トレーニングの日付を、セットの列に重ねる。
#[derive(Debug, Deserialize)]
struct AnalysisRow {
    analysis_training_id: String,
    analysis_date: String,
    #[serde(flatten)]
    segment: SetSegmentRow,
}

/// 1 トレーニング分の行。
#[derive(Debug)]
struct TrainingGroup {
    training_id: String,
    date: String,
    rows: Vec<SetSegmentRow>,
}

/// 1 トレーニング分の、変換済みの種目とセット。
#[derive(Debug)]
struct DatedExercises {
    date: String,
    exercises: Vec<StoredExercise>,
}

/// `GET /api/analysis/body-parts`
///
/// `period` は week か month、`metric` は sets か weight である。表示中の期間と、
/// その直前にある同じ単位の期間の、部位区分ごとのセット数と総重量を返す。期間の終わりは、
/// この要求を処理する時点の日本時間の日付を含む週または月の末日とする。
pub async fn body_parts(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let query = match request.url() {
        Ok(url) => url.query_pairs().into_owned().collect::<HashMap<_, _>>(),
        Err(error) => {
            console_error!("failed to read the analysis query: {error}");
            return Ok(response(ErrorCode::BadRequest, "the query is invalid"));
        }
    };
    let Some(period) = query
        .get("period")
        .and_then(|text| AnalysisPeriod::parse(text))
    else {
        return Ok(response(ErrorCode::BadRequest, "the period is unknown"));
    };
    let Some(metric) = query
        .get("metric")
        .and_then(|text| AnalysisMetric::parse(text))
    else {
        return Ok(response(ErrorCode::BadRequest, "the metric is unknown"));
    };
    let today = current_date()?;
    let range = period.range(today);
    let previous = period.previous_range(today);
    let db = Db::new(env.d1("DB")?);
    let trainings = match load_trainings(&db, &authenticated.id, previous.start, range.end).await {
        Ok(trainings) => trainings,
        Err(error) => {
            console_error!("failed to load the body part analysis: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    match analysis_response(period, metric, today, &trainings) {
        Ok(body) => Response::from_json(&body),
        Err(error) => {
            console_error!("failed to build the body part analysis: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}

fn analysis_statement(user_id: &str, from: JstDate, to: JstDate) -> SqlStatement {
    SqlStatement::new(
        format!(
            "SELECT ts.id AS analysis_training_id, ts.date AS analysis_date, {} \
             FROM training_sessions ts \
             JOIN training_exercises te ON te.training_id = ts.id AND te.user_id = ts.user_id \
             JOIN sets s ON s.training_exercise_id = te.id AND s.user_id = ts.user_id \
              AND s.completed = 1 \
             LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = ts.user_id \
             WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL AND ts.date >= ? \
              AND ts.date <= ? \
             ORDER BY ts.date, ts.finished_at, ts.id, te.position, s.position, sg.position",
            stored_sets::SET_SEGMENT_COLUMNS
        ),
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(from.to_string()),
            SqlValue::Text(to.to_string()),
        ],
    )
}

async fn load_trainings(
    db: &Db,
    user_id: &str,
    from: JstDate,
    to: JstDate,
) -> Result<Vec<DatedExercises>> {
    let rows = db
        .all(&analysis_statement(user_id, from, to))
        .await?
        .results::<AnalysisRow>()?;
    group_trainings(rows)
}

fn group_trainings(rows: Vec<AnalysisRow>) -> Result<Vec<DatedExercises>> {
    let mut groups = Vec::<TrainingGroup>::new();
    for row in rows {
        let index = match groups
            .iter()
            .position(|group| group.training_id == row.analysis_training_id)
        {
            Some(index) => index,
            None => {
                groups.push(TrainingGroup {
                    training_id: row.analysis_training_id,
                    date: row.analysis_date,
                    rows: Vec::new(),
                });
                groups.len() - 1
            }
        };
        groups[index].rows.push(row.segment);
    }
    groups
        .into_iter()
        .map(|group| {
            let exercises =
                stored_sets::build_training(&group.training_id, group.rows).map_err(|error| {
                    console_error!("the stored analysis sets are invalid: {error:?}");
                    worker::Error::RustError("the stored sets are invalid".to_owned())
                })?;
            Ok(DatedExercises {
                date: group.date,
                exercises,
            })
        })
        .collect()
}

fn analysis_response(
    period: AnalysisPeriod,
    metric: AnalysisMetric,
    today: JstDate,
    trainings: &[DatedExercises],
) -> Result<BodyPartAnalysis> {
    let mut sets = Vec::new();
    for training in trainings {
        let date = parse_date(&training.date)?;
        for exercise in &training.exercises {
            let body_part = exercise.snapshot.body_part;
            for record in &exercise.sets {
                sets.push(BodyPartSet {
                    date,
                    body_part,
                    record,
                });
            }
        }
    }
    Ok(body_part_analysis(period, metric, today, &sets))
}

#[cfg(test)]
mod tests {
    //! 部位別の分析の応答の組み立ての例示テスト。
    //!
    //! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
    //! 要求の受入基準は E2E が担い、ここは D1 の行から応答への変換、期間の範囲、
    //! スナップショットの部位区分の使用を確かめる。集計の規則は
    //! `warrior_core::body_part_analysis` のテストが担う。

    use warrior_core::body_part_analysis::{AnalysisMetric, AnalysisPeriod};
    use warrior_core::exercise::{
        BodyPart, ExerciseSnapshot, GripMode, Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
    };
    use warrior_core::jst::JstDate;
    use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};
    use warrior_core::sql::SqlValue;

    use super::{
        analysis_response, analysis_statement, group_trainings, AnalysisRow, DatedExercises,
    };
    use crate::stored_sets::{SetSegmentRow, StoredExercise};

    const USER_ID: &str = "user-a";
    const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4803";

    #[test]
    fn the_statement_scopes_the_user_the_dates_and_the_finished_sessions() {
        let statement = analysis_statement(USER_ID, date(2026, 11, 2), date(2026, 11, 15));
        assert!(statement.sql().contains(
            "WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL AND ts.date >= ? \
              AND ts.date <= ?"
        ));
        assert!(statement
            .sql()
            .contains("ORDER BY ts.date, ts.finished_at, ts.id"));
        assert!(statement
            .sql()
            .contains("ON sg.set_id = s.id AND sg.user_id = ts.user_id"));
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text(USER_ID.to_owned()),
                SqlValue::Text("2026-11-02".to_owned()),
                SqlValue::Text("2026-11-15".to_owned()),
            ]
        );
    }

    #[test]
    fn the_rows_are_grouped_by_the_training() {
        let rows = vec![
            analysis_row(
                "training-b",
                "2026-11-12",
                "set-b",
                BodyPart::Back,
                "normal",
            ),
            analysis_row(
                "training-a",
                "2026-11-11",
                "set-a",
                BodyPart::Chest,
                "normal",
            ),
        ];
        let trainings = group_trainings(rows).expect("the grouped trainings");
        assert_eq!(trainings.len(), 2);
        assert_eq!(trainings[0].date, "2026-11-12");
        assert_eq!(trainings[0].exercises.len(), 1);
        assert_eq!(trainings[0].exercises[0].sets.len(), 1);
        assert_eq!(trainings[0].exercises[0].snapshot.body_part, BodyPart::Back);
        assert_eq!(trainings[1].date, "2026-11-11");
    }

    #[test]
    fn the_response_uses_the_body_part_of_the_recorded_snapshot() {
        let trainings = vec![dated_exercises(
            "2026-11-12",
            BodyPart::Other,
            vec![weight_record(SetKind::Normal, 20.0, 10)],
        )];
        let analysis = analysis_response(
            AnalysisPeriod::Week,
            AnalysisMetric::Weight,
            date(2026, 11, 12),
            &trainings,
        )
        .expect("an analysis response");
        assert_eq!(analysis.period, AnalysisPeriod::Week);
        assert_eq!(analysis.metric, AnalysisMetric::Weight);
        assert_eq!(analysis.start, date(2026, 11, 9));
        assert_eq!(analysis.end, date(2026, 11, 15));
        assert_eq!(analysis.previous_start, date(2026, 11, 2));
        assert_eq!(analysis.previous_end, date(2026, 11, 8));
        assert_eq!(analysis.parts.len(), BodyPart::ALL.len());
        let other = analysis
            .parts
            .iter()
            .find(|part| part.body_part == BodyPart::Other)
            .expect("the other part");
        assert_eq!(other.sets, 1);
        assert_eq!(other.weight_kg, 200.0);
    }

    #[test]
    fn the_response_does_not_count_the_secondary_muscle_body_part() {
        let mut snapshot = exercise(BodyPart::Chest);
        snapshot.muscles = vec![
            MuscleAssignment {
                muscle: MuscleUnit::PectoralisMajorMiddle,
                role: MuscleRole::Primary,
            },
            MuscleAssignment {
                muscle: MuscleUnit::LatissimusDorsi,
                role: MuscleRole::Secondary,
            },
        ];
        let trainings = vec![DatedExercises {
            date: "2026-11-12".to_owned(),
            exercises: vec![StoredExercise {
                training_id: "training-a".to_owned(),
                snapshot,
                sets: vec![weight_record(SetKind::Normal, 100.0, 5)],
                memo: String::new(),
                set_grips: Vec::new(),
                set_body_weights: Vec::new(),
            }],
        }];
        let analysis = analysis_response(
            AnalysisPeriod::Week,
            AnalysisMetric::Sets,
            date(2026, 11, 12),
            &trainings,
        )
        .expect("an analysis response");
        let chest = analysis
            .parts
            .iter()
            .find(|part| part.body_part == BodyPart::Chest)
            .expect("the chest part");
        assert_eq!(chest.sets, 1);
        assert_eq!(chest.weight_kg, 500.0);
        let back = analysis
            .parts
            .iter()
            .find(|part| part.body_part == BodyPart::Back)
            .expect("the back part");
        assert_eq!(back.sets, 0);
        assert_eq!(back.weight_kg, 0.0);
    }

    #[test]
    fn the_response_rejects_an_invalid_stored_date() {
        let trainings = vec![dated_exercises(
            "2026-02-30",
            BodyPart::Chest,
            vec![weight_record(SetKind::Normal, 100.0, 5)],
        )];
        assert!(analysis_response(
            AnalysisPeriod::Month,
            AnalysisMetric::Sets,
            date(2026, 11, 12),
            &trainings,
        )
        .is_err());
    }

    fn exercise(body_part: BodyPart) -> ExerciseSnapshot {
        ExerciseSnapshot {
            exercise_id: EXERCISE_ID.to_owned(),
            name: "種目".to_owned(),
            body_part,
            muscles: muscle_assignments(body_part),
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode: GripMode::None,
            grip_options: Vec::new(),
        }
    }

    fn muscle_assignments(body_part: BodyPart) -> Vec<MuscleAssignment> {
        let muscle = match body_part {
            BodyPart::Chest => MuscleUnit::PectoralisMajorMiddle,
            BodyPart::Back => MuscleUnit::LatissimusDorsi,
            _ => return Vec::new(),
        };
        vec![MuscleAssignment {
            muscle,
            role: MuscleRole::Primary,
        }]
    }

    fn dated_exercises(date: &str, body_part: BodyPart, sets: Vec<SetRecord>) -> DatedExercises {
        let training_id = format!("training-{date}");
        DatedExercises {
            date: date.to_owned(),
            exercises: vec![StoredExercise {
                training_id: training_id.clone(),
                snapshot: exercise(body_part),
                sets,
                memo: String::new(),
                set_grips: Vec::new(),
                set_body_weights: Vec::new(),
            }],
        }
    }

    fn weight_record(kind: SetKind, weight_kg: f64, reps: u32) -> SetRecord {
        SetRecord::new(
            BestKey {
                exercise_id: EXERCISE_ID.to_owned(),
                grip: None,
                attachment: None,
            },
            kind,
            vec![SetSegment {
                weight_kg,
                reps,
                momentum_reps: 0,
                assisted_reps: 0,
                left: None,
                right: None,
                duration_seconds: None,
            }],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid weight set")
    }

    fn analysis_row(
        training_id: &str,
        date: &str,
        set_id: &str,
        body_part: BodyPart,
        kind: &str,
    ) -> AnalysisRow {
        AnalysisRow {
            analysis_training_id: training_id.to_owned(),
            analysis_date: date.to_owned(),
            segment: SetSegmentRow {
                training_id: training_id.to_owned(),
                training_date: None,
                training_memo: None,
                training_exercise_id: format!("{training_id}-exercise"),
                snapshot: serde_json::to_string(&exercise(body_part))
                    .expect("a serializable snapshot"),
                set_id: set_id.to_owned(),
                training_exercise_memo: String::new(),
                kind: kind.to_owned(),
                set_rpe: None,
                grip: None,
                body_weight_kg: None,
                segment_id: Some(format!("{set_id}-segment")),
                weight_kg: Some(20.0),
                reps: Some(10),
                momentum_reps: Some(0),
                assisted_reps: Some(0),
                duration_seconds: None,
                left_weight_kg: None,
                left_reps: None,
                left_duration_seconds: None,
                right_weight_kg: None,
                right_reps: None,
                right_duration_seconds: None,
            },
        }
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }
}
