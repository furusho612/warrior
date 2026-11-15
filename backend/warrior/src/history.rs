//! 種目別の履歴、自己ベスト、推移のグラフの取得。
//!
//! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`、
//! `REQ-SET-006`、`REQ-SET-011`、`REQ-GOAL-002`。
//! 記録の読み込みは `stored_sets` の変換を使い、期間の選択とグラフの点のまとめは
//! `warrior_core::history` に任せる。このモジュールはグリップとアタッチメントの
//! 絞り込み、D1 の行から応答への組み立て、期間の終わりに使う日本時間の日付の決定を担う
//! (ADR-016)。自己ベストは `personal_bests` の保存済みの値を読み (REQ-SET-009)、
//! 目標重量と現在の記録との差は `goal` の組み立てを共有する (REQ-GOAL-002)。

use std::collections::HashMap;

use serde::Deserialize;
use warrior_core::calendar;
use warrior_core::exercise::{Exercise, GripMode};
use warrior_core::history::{
    best_kinds, graph_points, matches_grip, sort_trainings, ExerciseBest, ExerciseGraph,
    ExerciseHistory, GraphMetric, GraphPeriod, GraphSet, GripFilter, HistoryTraining,
};
use warrior_core::jst::{JstDate, UtcInstant};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::BestKind;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;
use crate::goal::{self, GoalRow};
use crate::stored_sets::{self, SetSegmentRow, StoredExercise};

/// 履歴の 1 行。トレーニングの日付と終了の時刻を、セットの列に重ねる。
#[derive(Debug, Deserialize)]
struct HistoryRow {
    history_training_id: String,
    history_date: String,
    history_finished_at: String,
    #[serde(flatten)]
    segment: SetSegmentRow,
}

/// 種目の履歴の、1 トレーニング分の行。
#[derive(Debug)]
struct TrainingGroup {
    training_id: String,
    date: String,
    finished_at: String,
    segments: Vec<SetSegmentRow>,
}

/// 1 トレーニング分の、変換済みの種目とセット。
#[derive(Debug)]
pub(crate) struct ExerciseTrainingSets {
    pub(crate) training_id: String,
    pub(crate) date: String,
    pub(crate) finished_at: String,
    pub(crate) exercises: Vec<StoredExercise>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BestRow {
    pub(crate) grip: String,
    pub(crate) attachment: String,
    pub(crate) kind: String,
    pub(crate) value: f64,
    pub(crate) achieved_on: String,
}

/// 履歴とグラフの目標の組み立てに渡す、目標の行と自己ベストの行。
pub(crate) struct GoalData {
    pub(crate) rows: Vec<GoalRow>,
    pub(crate) bests: Vec<BestRow>,
}

/// `GET /api/exercises/:id/history`
///
/// 種目の過去の記録を新しい順に返し、自己ベストを添える。`grip` と `attachment` の
/// クエリは `GripSelection::comparison_key` の値で絞り込む。指定の無い種目は
/// 絞り込まない (REQ-SET-011)。
pub async fn history(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(exercise_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let filter = match grip_filter(&request) {
        Ok(filter) => filter,
        Err(error) => {
            console_error!("failed to read the grip filter: {error}");
            return Ok(response(ErrorCode::BadRequest, "the query is invalid"));
        }
    };
    let db = Db::new(env.d1("DB")?);
    let Some(exercise) = load_exercise(&db, &authenticated.id, &exercise_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let trainings = match load_trainings(&db, &authenticated.id, &exercise_id).await {
        Ok(trainings) => trainings,
        Err(error) => {
            console_error!("failed to load the exercise history: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let bests = load_bests(&db, &authenticated.id, &exercise_id).await?;
    let goals = goal::load_rows(&db, &authenticated.id, &exercise_id).await?;
    let today = current_date()?;
    match history_response(&exercise, &filter, today, trainings, bests, goals) {
        Ok(body) => Response::from_json(&body),
        Err(error) => {
            console_error!("failed to build the exercise history: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}

/// `GET /api/exercises/:id/graph`
///
/// `period` は 1m、3m、6m、1y、all のいずれか、`metric` は記録方式に合う指標である。
/// 期間の終わりは、この要求を処理する時点の日本時間の日付とする。点は日付の古い順に返す。
pub async fn graph(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(exercise_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let query = match request.url() {
        Ok(url) => url.query_pairs().into_owned().collect::<HashMap<_, _>>(),
        Err(error) => {
            console_error!("failed to read the graph query: {error}");
            return Ok(response(ErrorCode::BadRequest, "the query is invalid"));
        }
    };
    let Some(period) = query
        .get("period")
        .and_then(|text| GraphPeriod::parse(text))
    else {
        return Ok(response(ErrorCode::BadRequest, "the period is unknown"));
    };
    let Some(metric) = query
        .get("metric")
        .and_then(|text| GraphMetric::parse(text))
    else {
        return Ok(response(ErrorCode::BadRequest, "the metric is unknown"));
    };
    let filter = GripFilter::new(query.get("grip").cloned(), query.get("attachment").cloned());
    let db = Db::new(env.d1("DB")?);
    let Some(exercise) = load_exercise(&db, &authenticated.id, &exercise_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if !metric.is_allowed_for(exercise.record_mode) {
        return Ok(response(
            ErrorCode::BadRequest,
            "the metric is unknown for the record mode",
        ));
    }
    let trainings = match load_trainings(&db, &authenticated.id, &exercise_id).await {
        Ok(trainings) => trainings,
        Err(error) => {
            console_error!("failed to load the exercise history: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let bests = load_bests(&db, &authenticated.id, &exercise_id).await?;
    let goals = goal::load_rows(&db, &authenticated.id, &exercise_id).await?;
    match graph_response(
        &exercise,
        period,
        metric,
        &filter,
        &trainings,
        GoalData { rows: goals, bests },
        current_date()?,
    ) {
        Ok(body) => Response::from_json(&body),
        Err(error) => {
            console_error!("failed to build the exercise graph: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}

async fn load_exercise(db: &Db, user_id: &str, exercise_id: &str) -> Result<Option<Exercise>> {
    if let Some(exercise) = exercise::load_master(db, exercise_id).await? {
        return Ok(Some(exercise));
    }
    exercise::load_user(db, user_id, exercise_id).await
}

fn grip_filter(request: &Request) -> Result<GripFilter> {
    let query = request
        .url()?
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    Ok(GripFilter::new(
        query.get("grip").cloned(),
        query.get("attachment").cloned(),
    ))
}

fn history_statement(user_id: &str, exercise_id: &str) -> SqlStatement {
    SqlStatement::new(
        format!(
            "SELECT ts.id AS history_training_id, ts.date AS history_date, \
             ts.finished_at AS history_finished_at, {} \
             FROM training_sessions ts \
             JOIN training_exercises te ON te.training_id = ts.id AND te.user_id = ts.user_id \
             JOIN sets s ON s.training_exercise_id = te.id AND s.user_id = ts.user_id \
              AND s.completed = 1 \
             LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = ts.user_id \
             WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL AND te.exercise_id = ? \
             ORDER BY ts.date DESC, ts.finished_at DESC, ts.id DESC, te.position, s.position, \
              sg.position",
            stored_sets::SET_SEGMENT_COLUMNS
        ),
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(exercise_id.to_owned()),
        ],
    )
}

pub(crate) async fn load_trainings(
    db: &Db,
    user_id: &str,
    exercise_id: &str,
) -> Result<Vec<ExerciseTrainingSets>> {
    let rows = db
        .all(&history_statement(user_id, exercise_id))
        .await?
        .results::<HistoryRow>()?;
    group_trainings(rows)
}

fn group_trainings(rows: Vec<HistoryRow>) -> Result<Vec<ExerciseTrainingSets>> {
    let mut groups = Vec::<TrainingGroup>::new();
    for row in rows {
        let index = match groups
            .iter()
            .position(|group| group.training_id == row.history_training_id)
        {
            Some(index) => index,
            None => {
                groups.push(TrainingGroup {
                    training_id: row.history_training_id,
                    date: row.history_date,
                    finished_at: row.history_finished_at,
                    segments: Vec::new(),
                });
                groups.len() - 1
            }
        };
        groups[index].segments.push(row.segment);
    }
    groups
        .into_iter()
        .map(|group| {
            let exercises = stored_sets::build_training(&group.training_id, group.segments)
                .map_err(|error| {
                    console_error!("the stored history sets are invalid: {error:?}");
                    worker::Error::RustError("the stored sets are invalid".to_owned())
                })?;
            Ok(ExerciseTrainingSets {
                training_id: group.training_id,
                date: group.date,
                finished_at: group.finished_at,
                exercises,
            })
        })
        .collect()
}

pub(crate) async fn load_bests(db: &Db, user_id: &str, exercise_id: &str) -> Result<Vec<BestRow>> {
    db.all(&SqlStatement::new(
        "SELECT grip, attachment, kind, value, achieved_on FROM personal_bests \
         WHERE user_id = ? AND exercise_id = ?",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(exercise_id.to_owned()),
        ],
    ))
    .await?
    .results::<BestRow>()
}

fn history_response(
    exercise: &Exercise,
    filter: &GripFilter,
    today: JstDate,
    trainings: Vec<ExerciseTrainingSets>,
    bests: Vec<BestRow>,
    goals: Vec<GoalRow>,
) -> Result<ExerciseHistory> {
    let mut history_trainings = Vec::new();
    for training in &trainings {
        let date = parse_date(&training.date)?;
        ensure_instant(&training.finished_at)?;
        let sets = training_sets(exercise.grip_mode, filter, &training.exercises);
        if sets.is_empty() {
            continue;
        }
        history_trainings.push(HistoryTraining {
            training_id: training.training_id.clone(),
            date,
            finished_at: training.finished_at.clone(),
            sets,
        });
    }
    sort_trainings(&mut history_trainings);
    let bests_for_history =
        exercise_bests(exercise.grip_mode, exercise.record_mode, filter, &bests)?;
    let goal_progress = goal::statuses(
        exercise.grip_mode,
        filter,
        GoalData { rows: goals, bests },
        &trainings,
        today,
    )?;
    Ok(ExerciseHistory {
        exercise_id: exercise.id.clone(),
        name: exercise.name.clone(),
        record_mode: exercise.record_mode,
        laterality: exercise.laterality,
        grip_mode: exercise.grip_mode,
        grip_options: exercise.grip_options.clone(),
        bests: bests_for_history,
        goals: goal_progress,
        trainings: history_trainings,
    })
}

fn training_sets(
    mode: GripMode,
    filter: &GripFilter,
    exercises: &[StoredExercise],
) -> Vec<warrior_core::calendar::CalendarSet> {
    exercises
        .iter()
        .flat_map(|exercise| exercise.sets.iter())
        .filter(|record| {
            matches_grip(
                mode,
                filter,
                record.key().grip.as_deref(),
                record.key().attachment.as_deref(),
            )
        })
        .map(calendar::set_detail)
        .collect()
}

fn exercise_bests(
    grip_mode: GripMode,
    record_mode: warrior_core::set_calc::RecordMode,
    filter: &GripFilter,
    rows: &[BestRow],
) -> Result<Vec<ExerciseBest>> {
    let kinds = best_kinds(record_mode);
    let mut bests = Vec::new();
    for row in rows {
        let Some(kind) = BestKind::parse(&row.kind) else {
            console_error!("the stored personal best kind is invalid");
            return Err(worker::Error::RustError(
                "the stored personal best kind is invalid".to_owned(),
            ));
        };
        if !kinds.contains(&kind) {
            continue;
        }
        let grip = optional_key(&row.grip);
        let attachment = optional_key(&row.attachment);
        if !matches_grip(grip_mode, filter, grip.as_deref(), attachment.as_deref()) {
            continue;
        }
        bests.push(ExerciseBest {
            kind,
            value: row.value,
            achieved_on: parse_date(&row.achieved_on)?,
            grip,
            attachment,
        });
    }
    bests.sort_by(|left, right| {
        kind_rank(kinds, left.kind)
            .cmp(&kind_rank(kinds, right.kind))
            .then_with(|| left.grip.cmp(&right.grip))
            .then_with(|| left.attachment.cmp(&right.attachment))
    });
    Ok(bests)
}

fn kind_rank(kinds: &[BestKind], kind: BestKind) -> usize {
    kinds
        .iter()
        .position(|known| *known == kind)
        .unwrap_or(kinds.len())
}

fn graph_response(
    exercise: &Exercise,
    period: GraphPeriod,
    metric: GraphMetric,
    filter: &GripFilter,
    trainings: &[ExerciseTrainingSets],
    goal_data: GoalData,
    end: JstDate,
) -> Result<ExerciseGraph> {
    let sets = graph_sets(exercise.grip_mode, filter, trainings)?;
    Ok(ExerciseGraph {
        exercise_id: exercise.id.clone(),
        period,
        metric,
        start: period.start(end),
        end,
        points: graph_points(period, end, metric, &sets),
        goals: goal::statuses(exercise.grip_mode, filter, goal_data, trainings, end)?,
    })
}

fn graph_sets<'a>(
    mode: GripMode,
    filter: &GripFilter,
    trainings: &'a [ExerciseTrainingSets],
) -> Result<Vec<GraphSet<'a>>> {
    let mut sets = Vec::new();
    for training in trainings {
        let date = parse_date(&training.date)?;
        for exercise in &training.exercises {
            for record in &exercise.sets {
                if !matches_grip(
                    mode,
                    filter,
                    record.key().grip.as_deref(),
                    record.key().attachment.as_deref(),
                ) {
                    continue;
                }
                sets.push(GraphSet { date, record });
            }
        }
    }
    Ok(sets)
}

pub(crate) fn parse_date(text: &str) -> Result<JstDate> {
    JstDate::parse(text).ok_or_else(|| {
        console_error!("the stored date is invalid");
        worker::Error::RustError("the stored date is invalid".to_owned())
    })
}

fn ensure_instant(text: &str) -> Result<()> {
    UtcInstant::parse(text).ok_or_else(|| {
        console_error!("the stored finished time is invalid");
        worker::Error::RustError("the stored finished time is invalid".to_owned())
    })?;
    Ok(())
}

pub(crate) fn current_date() -> Result<JstDate> {
    UtcInstant::from_epoch_seconds(auth::now_seconds())
        .jst_date()
        .ok_or_else(|| {
            console_error!("the current time is outside the representable dates");
            worker::Error::RustError("the current date is invalid".to_owned())
        })
}

fn optional_key(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    //! 種目別の履歴とグラフの応答の組み立ての例示テスト。
    //!
    //! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`、
    //! `REQ-SET-006`、`REQ-SET-011`。
    //! 要求の受入基準は E2E が担い、ここは D1 の行から応答への変換、絞り込み、並びを
    //! 確かめる。期間と指標の集計は `warrior_core::history` のテストが担う。

    use warrior_core::exercise::{
        BodyPart, Exercise, ExerciseSnapshot, GripMode, GripSelection, GripStyle, GripWidth,
        Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
    };
    use warrior_core::history::{
        ExerciseBest, ExerciseHistory, GraphMetric, GraphPeriod, GripFilter,
    };
    use warrior_core::jst::JstDate;
    use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};
    use warrior_core::sql::SqlValue;
    use warrior_core::training_session::BestKind;

    use super::{
        exercise_bests, graph_response, group_trainings, history_response, history_statement,
        BestRow, ExerciseTrainingSets, GoalData, HistoryRow,
    };
    use crate::stored_sets::StoredExercise;

    const USER_ID: &str = "user-a";
    const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4803";

    #[test]
    fn the_history_statement_scopes_the_user_the_exercise_and_the_finished_sessions() {
        let statement = history_statement(USER_ID, EXERCISE_ID);
        assert!(statement.sql().contains(
            "WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL AND te.exercise_id = ?"
        ));
        assert!(statement
            .sql()
            .contains("ORDER BY ts.date DESC, ts.finished_at DESC, ts.id DESC"));
        assert!(statement
            .sql()
            .contains("ON sg.set_id = s.id AND sg.user_id = ts.user_id"));
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text(USER_ID.to_owned()),
                SqlValue::Text(EXERCISE_ID.to_owned()),
            ]
        );
    }

    #[test]
    fn the_history_rows_are_grouped_by_the_training_in_the_query_order() {
        let rows = vec![
            history_row(
                "training-b",
                "2026-11-12",
                "2026-11-12T06:00:00Z",
                "set-b",
                "normal",
            ),
            history_row(
                "training-a",
                "2026-11-11",
                "2026-11-11T03:00:00Z",
                "set-a",
                "normal",
            ),
        ];
        let trainings = group_trainings(rows).expect("the grouped trainings");
        assert_eq!(trainings.len(), 2);
        assert_eq!(trainings[0].training_id, "training-b");
        assert_eq!(trainings[0].date, "2026-11-12");
        assert_eq!(trainings[0].finished_at, "2026-11-12T06:00:00Z");
        assert_eq!(trainings[0].exercises.len(), 1);
        assert_eq!(trainings[0].exercises[0].sets.len(), 1);
        assert_eq!(trainings[1].training_id, "training-a");
    }

    #[test]
    fn the_history_response_orders_the_trainings_and_keeps_the_set_values() {
        let exercise = exercise(GripMode::None);
        let trainings = vec![
            training_sets(
                "training-b",
                "2026-11-10",
                "2026-11-10T03:00:00Z",
                vec![weight_record(SetKind::Normal, 100.0, 5)],
            ),
            training_sets(
                "training-a",
                "2026-11-12",
                "2026-11-12T03:00:00Z",
                vec![weight_record(SetKind::Warmup, 60.0, 10)],
            ),
            training_sets(
                "training-c",
                "2026-11-12",
                "2026-11-12T06:00:00Z",
                vec![weight_record(SetKind::Normal, 80.0, 10)],
            ),
        ];
        let history = history_response(
            &exercise,
            &GripFilter::default(),
            date(2026, 11, 13),
            trainings,
            Vec::new(),
            Vec::new(),
        )
        .expect("a history response");
        assert_eq!(history.name, "ベンチプレス");
        assert_eq!(history.record_mode, RecordMode::WeightAndReps);
        assert_eq!(
            history
                .trainings
                .iter()
                .map(|training| training.training_id.as_str())
                .collect::<Vec<_>>(),
            vec!["training-c", "training-a", "training-b"]
        );
        let warmup = &history.trainings[1].sets[0];
        assert_eq!(warmup.kind, SetKind::Warmup);
        assert_eq!(warmup.total_weight_kg, 0.0);
        let normal = &history.trainings[0].sets[0];
        assert_eq!(normal.kind, SetKind::Normal);
        assert_eq!(normal.total_weight_kg, 800.0);
        assert_eq!(normal.segments[0].reps, 10);
    }

    #[test]
    fn the_history_response_skips_a_training_without_matching_sets() {
        let exercise = exercise(GripMode::Grip);
        let trainings = vec![training_sets(
            "training-a",
            "2026-11-12",
            "2026-11-12T03:00:00Z",
            vec![grip_record(SetKind::Normal, 100.0, 5, "pronated_medium")],
        )];
        let filter = GripFilter::new(Some("supinated_narrow".to_owned()), None);
        let history = history_response(
            &exercise,
            &filter,
            date(2026, 11, 13),
            trainings,
            Vec::new(),
            Vec::new(),
        )
        .expect("a history response");
        assert!(history.trainings.is_empty());

        let matching = GripFilter::new(Some("pronated_medium".to_owned()), None);
        let history = history_response(
            &exercise,
            &matching,
            date(2026, 11, 13),
            vec![training_sets(
                "training-a",
                "2026-11-12",
                "2026-11-12T03:00:00Z",
                vec![grip_record(SetKind::Normal, 100.0, 5, "pronated_medium")],
            )],
            Vec::new(),
            Vec::new(),
        )
        .expect("a history response");
        assert_eq!(history.trainings.len(), 1);
        assert_eq!(
            history.trainings[0].sets[0].grip.as_deref(),
            Some("pronated_medium")
        );
    }

    #[test]
    fn the_bests_follow_the_filter_and_the_record_mode() {
        let rows = vec![
            best("max_weight", 100.0, "2026-11-10", "pronated_medium", ""),
            best("max_weight", 120.0, "2026-11-11", "supinated_narrow", ""),
            best(
                "estimated_one_rep_max",
                110.0,
                "2026-11-11",
                "supinated_narrow",
                "",
            ),
            best("total_weight", 900.0, "2026-11-11", "supinated_narrow", ""),
            best(
                "longest_duration",
                90.0,
                "2026-11-11",
                "supinated_narrow",
                "",
            ),
        ];
        let filter = GripFilter::new(Some("supinated_narrow".to_owned()), None);
        let bests = exercise_bests(GripMode::Grip, RecordMode::WeightAndReps, &filter, &rows)
            .expect("the filtered bests");
        assert_eq!(
            bests,
            vec![
                best_value(BestKind::MaxWeight, 120.0, "2026-11-11", "supinated_narrow"),
                best_value(
                    BestKind::EstimatedOneRepMax,
                    110.0,
                    "2026-11-11",
                    "supinated_narrow"
                ),
                best_value(
                    BestKind::TotalWeight,
                    900.0,
                    "2026-11-11",
                    "supinated_narrow"
                ),
            ]
        );
    }

    #[test]
    fn the_time_bests_keep_only_the_longest_duration() {
        let rows = vec![
            best("max_weight", 100.0, "2026-11-10", "", ""),
            best("longest_duration", 90.0, "2026-11-11", "", ""),
        ];
        let bests = exercise_bests(
            GripMode::None,
            RecordMode::Time,
            &GripFilter::default(),
            &rows,
        )
        .expect("the time bests");
        assert_eq!(
            bests,
            vec![ExerciseBest {
                kind: BestKind::LongestDuration,
                value: 90.0,
                achieved_on: date(2026, 11, 11),
                grip: None,
                attachment: None,
            }]
        );
    }

    #[test]
    fn an_invalid_stored_best_kind_is_rejected() {
        let rows = vec![BestRow {
            grip: String::new(),
            attachment: String::new(),
            kind: "unknown".to_owned(),
            value: 100.0,
            achieved_on: "2026-11-11".to_owned(),
        }];
        assert!(exercise_bests(
            GripMode::None,
            RecordMode::WeightAndReps,
            &GripFilter::default(),
            &rows,
        )
        .is_err());
    }

    #[test]
    fn the_graph_response_uses_the_period_and_the_metric() {
        let exercise = exercise(GripMode::None);
        let trainings = vec![training_sets(
            "training-a",
            "2026-11-12",
            "2026-11-12T03:00:00Z",
            vec![
                weight_record(SetKind::Normal, 100.0, 5),
                weight_record(SetKind::Normal, 80.0, 10),
            ],
        )];
        let graph = graph_response(
            &exercise,
            GraphPeriod::All,
            GraphMetric::TotalWeight,
            &GripFilter::default(),
            &trainings,
            GoalData {
                rows: Vec::new(),
                bests: Vec::new(),
            },
            date(2026, 11, 13),
        )
        .expect("a graph response");
        assert_eq!(graph.exercise_id, EXERCISE_ID);
        assert_eq!(graph.period, GraphPeriod::All);
        assert_eq!(graph.metric, GraphMetric::TotalWeight);
        assert_eq!(graph.start, None);
        assert_eq!(graph.points.len(), 1);
        assert_eq!(graph.points[0].date, date(2026, 11, 12));
        assert_eq!(graph.points[0].value, 500.0 + 800.0);
    }

    #[test]
    fn the_graph_response_rejects_an_invalid_stored_date() {
        let exercise = exercise(GripMode::None);
        let trainings = vec![training_sets(
            "training-a",
            "2026-02-30",
            "2026-02-30T03:00:00Z",
            vec![weight_record(SetKind::Normal, 100.0, 5)],
        )];
        assert!(graph_response(
            &exercise,
            GraphPeriod::All,
            GraphMetric::MaxWeight,
            &GripFilter::default(),
            &trainings,
            GoalData {
                rows: Vec::new(),
                bests: Vec::new(),
            },
            date(2026, 11, 13),
        )
        .is_err());
    }

    fn exercise(grip_mode: GripMode) -> Exercise {
        Exercise {
            id: EXERCISE_ID.to_owned(),
            name: "ベンチプレス".to_owned(),
            name_kana: "べんちぷれす".to_owned(),
            body_part: BodyPart::Chest,
            equipment: warrior_core::exercise::Equipment::Barbell,
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode,
            other_scope: None,
            instructions: String::new(),
            cautions: String::new(),
            tips: String::new(),
            sort_order: 1,
            muscles: vec![MuscleAssignment {
                muscle: MuscleUnit::PectoralisMajorMiddle,
                role: MuscleRole::Primary,
            }],
            grip_options: Vec::new(),
        }
    }

    fn snapshot(exercise: &Exercise) -> ExerciseSnapshot {
        ExerciseSnapshot {
            exercise_id: exercise.id.clone(),
            name: exercise.name.clone(),
            body_part: exercise.body_part,
            muscles: exercise.muscles.clone(),
            record_mode: exercise.record_mode,
            laterality: exercise.laterality,
            grip_mode: exercise.grip_mode,
            grip_options: exercise.grip_options.clone(),
        }
    }

    fn training_sets(
        training_id: &str,
        date: &str,
        finished_at: &str,
        sets: Vec<SetRecord>,
    ) -> ExerciseTrainingSets {
        ExerciseTrainingSets {
            training_id: training_id.to_owned(),
            date: date.to_owned(),
            finished_at: finished_at.to_owned(),
            exercises: vec![StoredExercise {
                training_id: training_id.to_owned(),
                snapshot: snapshot(&exercise(GripMode::None)),
                sets,
                memo: String::new(),
                set_grips: Vec::new(),
                set_body_weights: Vec::new(),
            }],
        }
    }

    fn key(grip: Option<&str>) -> BestKey {
        BestKey {
            exercise_id: EXERCISE_ID.to_owned(),
            grip: grip.map(str::to_owned),
            attachment: None,
        }
    }

    fn weight_record(kind: SetKind, weight_kg: f64, reps: u32) -> SetRecord {
        SetRecord::new(
            key(None),
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

    fn grip_record(kind: SetKind, weight_kg: f64, reps: u32, grip: &str) -> SetRecord {
        SetRecord::new(
            key(Some(grip)),
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
        .expect("a valid gripping set")
    }

    fn history_row(
        training_id: &str,
        date: &str,
        finished_at: &str,
        set_id: &str,
        kind: &str,
    ) -> HistoryRow {
        HistoryRow {
            history_training_id: training_id.to_owned(),
            history_date: date.to_owned(),
            history_finished_at: finished_at.to_owned(),
            segment: crate::stored_sets::SetSegmentRow {
                training_id: training_id.to_owned(),
                training_date: None,
                training_exercise_id: format!("{training_id}-exercise"),
                snapshot: serde_json::to_string(&snapshot(&exercise(GripMode::None)))
                    .expect("a serializable snapshot"),
                set_id: set_id.to_owned(),
                training_exercise_memo: String::new(),
                kind: kind.to_owned(),
                set_rpe: None,
                grip: None,
                body_weight_kg: None,
                segment_id: Some(format!("{set_id}-segment")),
                weight_kg: Some(100.0),
                reps: Some(5),
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

    fn best(kind: &str, value: f64, achieved_on: &str, grip: &str, attachment: &str) -> BestRow {
        BestRow {
            grip: grip.to_owned(),
            attachment: attachment.to_owned(),
            kind: kind.to_owned(),
            value,
            achieved_on: achieved_on.to_owned(),
        }
    }

    fn best_value(kind: BestKind, value: f64, achieved_on: &str, grip: &str) -> ExerciseBest {
        ExerciseBest {
            kind,
            value,
            achieved_on: JstDate::parse(achieved_on).expect("a valid date"),
            grip: Some(grip.to_owned()),
            attachment: None,
        }
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }

    #[test]
    fn the_history_response_keeps_the_exercise_metadata() {
        let mut exercise = exercise(GripMode::Grip);
        exercise.grip_options = vec![GripSelection::Grip {
            style: GripStyle::Pronated,
            width: GripWidth::Medium,
        }];
        let history = history_response(
            &exercise,
            &GripFilter::default(),
            date(2026, 11, 13),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("an empty history");
        let json = serde_json::to_string(&history).expect("a serializable history");
        assert_eq!(
            serde_json::from_str::<ExerciseHistory>(&json).expect("a readable history"),
            history
        );
    }
}
