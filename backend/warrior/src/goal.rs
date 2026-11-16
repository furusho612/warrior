//! 目標重量の D1 の読み書きと、履歴とグラフに載せる現在の記録との差の組み立て。
//!
//! 対応する要求: `REQ-GOAL-001`、`REQ-GOAL-002`。
//! 目標は利用者、種目、グリップ、アタッチメントの組ごとに 1 行を持つ。現在の記録の定義と
//! 目標の検証は `warrior_core::goal` が持ち、このモジュールは D1 の行と HTTP の入力を
//! その型へ変換する。履歴とグラフは `statuses` で同じ組み立てを共有する。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use warrior_core::exercise::{Exercise, GripMode};
use warrior_core::goal::{
    current_weight_kg, matches_selection, ExerciseGoal, GoalProgress, GoalSet,
};
use warrior_core::history::GripFilter;
use warrior_core::jst::JstDate;
use warrior_core::set_calc::SetRecord;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::BestKind;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;
use crate::history::{self, BestRow, ExerciseTrainingSets, GoalData};

/// 目標の 1 行。
#[derive(Debug, Deserialize)]
pub(crate) struct GoalRow {
    pub(crate) exercise_id: String,
    pub(crate) grip: String,
    pub(crate) attachment: String,
    pub(crate) target_weight_kg: f64,
    pub(crate) target_date: Option<String>,
}

/// `PUT /api/exercises/:id/goal` の本文。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoalInput {
    #[serde(default)]
    grip: Option<String>,
    #[serde(default)]
    attachment: Option<String>,
    target_weight_kg: f64,
    #[serde(default)]
    target_date: Option<JstDate>,
}

/// 削除の結果。
#[derive(Debug, Serialize)]
struct GoalDeleted {
    deleted: bool,
}

pub(crate) async fn load_rows(db: &Db, user_id: &str, exercise_id: &str) -> Result<Vec<GoalRow>> {
    db.all(&SqlStatement::new(
        "SELECT exercise_id, grip, attachment, target_weight_kg, target_date \
         FROM exercise_goals WHERE user_id = ? AND exercise_id = ? ORDER BY grip, attachment",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(exercise_id.to_owned()),
        ],
    ))
    .await?
    .results::<GoalRow>()
}

/// `PUT /api/exercises/:id/goal`
///
/// 種目とグリップとアタッチメントの組の目標を保存し、現在の記録との差を返す。
/// 同じ組の目標は上書きする。
pub async fn put(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(exercise_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input = match request
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<GoalInput>(&body).ok())
    {
        Some(input) => input,
        None => return Ok(response(ErrorCode::BadRequest, "the goal is invalid")),
    };
    let db = Db::new(env.d1("DB")?);
    let Some(exercise) = load_exercise(&db, &authenticated.id, &exercise_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(today) = current_date() else {
        console_error!("the current Japan date is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let Some(goal) = validated_goal(input, &exercise, today) else {
        return Ok(response(ErrorCode::BadRequest, "the goal is invalid"));
    };
    let now = auth::now_iso();
    if let Err(error) = db
        .run(&upsert_statement(&authenticated.id, &goal, &now))
        .await
    {
        console_error!("failed to save the exercise goal: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    match progress_for(&db, &authenticated.id, &goal, today).await {
        Ok(progress) => Response::from_json(&progress),
        Err(error) => {
            console_error!("failed to read the current record: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}

/// `DELETE /api/exercises/:id/goal`
///
/// クエリの `grip` と `attachment` で組を指定し、その組の目標を取り消す。
pub async fn delete(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(exercise_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let filter = match query_filter(&request) {
        Ok(filter) => filter,
        Err(error) => {
            console_error!("failed to read the goal query: {error}");
            return Ok(response(ErrorCode::BadRequest, "the query is invalid"));
        }
    };
    let db = Db::new(env.d1("DB")?);
    let Some(exercise) = load_exercise(&db, &authenticated.id, &exercise_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if !matches_selection(
        exercise.grip_mode,
        &exercise.grip_options,
        filter.grip.as_deref(),
        filter.attachment.as_deref(),
    ) {
        return Ok(response(ErrorCode::BadRequest, "the goal is invalid"));
    }
    if let Err(error) = db
        .run(&delete_statement(&authenticated.id, &exercise.id, &filter))
        .await
    {
        console_error!("failed to delete the exercise goal: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    Response::from_json(&GoalDeleted { deleted: true })
}

/// 履歴とグラフに載せる、絞り込みに一致する目標の一覧。
pub(crate) fn statuses(
    mode: GripMode,
    filter: &GripFilter,
    data: GoalData,
    trainings: &[ExerciseTrainingSets],
    today: JstDate,
) -> Result<Vec<GoalProgress>> {
    let GoalData { rows, bests } = data;
    let mut progress = Vec::new();
    for row in rows {
        if !warrior_core::history::matches_grip(
            mode,
            filter,
            optional_text(&row.grip).as_deref(),
            optional_text(&row.attachment).as_deref(),
        ) {
            continue;
        }
        let goal = row.into_goal()?;
        let sets = sets_for(&goal, trainings)?;
        let current = current_weight_kg(today, &sets, best_for(&goal, &bests));
        progress.push(GoalProgress::new(goal, current));
    }
    Ok(progress)
}

async fn progress_for(
    db: &Db,
    user_id: &str,
    goal: &ExerciseGoal,
    today: JstDate,
) -> Result<GoalProgress> {
    let trainings = history::load_trainings(db, user_id, &goal.exercise_id).await?;
    let bests = history::load_bests(db, user_id, &goal.exercise_id).await?;
    let sets = sets_for(goal, &trainings)?;
    let current = current_weight_kg(today, &sets, best_for(goal, &bests));
    Ok(GoalProgress::new(goal.clone(), current))
}

fn sets_for<'a>(
    goal: &ExerciseGoal,
    trainings: &'a [ExerciseTrainingSets],
) -> Result<Vec<GoalSet<'a>>> {
    let mut sets = Vec::new();
    for training in trainings {
        let date = parse_date(&training.date)?;
        for exercise in &training.exercises {
            for record in &exercise.sets {
                if matches_goal(goal, record) {
                    sets.push(GoalSet {
                        training_id: &training.training_id,
                        date,
                        record,
                    });
                }
            }
        }
    }
    Ok(sets)
}

fn matches_goal(goal: &ExerciseGoal, record: &SetRecord) -> bool {
    record.key().grip == goal.grip && record.key().attachment == goal.attachment
}

fn best_for(goal: &ExerciseGoal, bests: &[BestRow]) -> Option<f64> {
    bests
        .iter()
        .find(|best| {
            best.kind == BestKind::MaxWeight.as_str()
                && optional_text(&best.grip) == goal.grip
                && optional_text(&best.attachment) == goal.attachment
        })
        .map(|best| best.value)
}

fn validated_goal(input: GoalInput, exercise: &Exercise, today: JstDate) -> Option<ExerciseGoal> {
    let filter = GripFilter::new(input.grip, input.attachment);
    if !matches_selection(
        exercise.grip_mode,
        &exercise.grip_options,
        filter.grip.as_deref(),
        filter.attachment.as_deref(),
    ) {
        return None;
    }
    let goal = ExerciseGoal {
        exercise_id: exercise.id.clone(),
        grip: filter.grip,
        attachment: filter.attachment,
        target_weight_kg: input.target_weight_kg,
        target_date: input.target_date,
        current_weight_kg: None,
    };
    goal.validate(today).ok()?;
    Some(goal)
}

fn upsert_statement(user_id: &str, goal: &ExerciseGoal, now: &str) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO exercise_goals \
         (user_id, exercise_id, grip, attachment, target_weight_kg, target_date, created_at, \
          updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, exercise_id, grip, attachment) DO UPDATE SET \
         target_weight_kg = excluded.target_weight_kg, target_date = excluded.target_date, \
         updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(goal.exercise_id.clone()),
            SqlValue::Text(key_text(goal.grip.as_deref())),
            SqlValue::Text(key_text(goal.attachment.as_deref())),
            SqlValue::Real(goal.target_weight_kg),
            optional_date(goal.target_date),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn delete_statement(user_id: &str, exercise_id: &str, filter: &GripFilter) -> SqlStatement {
    SqlStatement::new(
        "DELETE FROM exercise_goals \
         WHERE user_id = ? AND exercise_id = ? AND grip = ? AND attachment = ?",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(exercise_id.to_owned()),
            SqlValue::Text(key_text(filter.grip.as_deref())),
            SqlValue::Text(key_text(filter.attachment.as_deref())),
        ],
    )
}

async fn load_exercise(db: &Db, user_id: &str, exercise_id: &str) -> Result<Option<Exercise>> {
    if let Some(exercise) = exercise::load_master(db, exercise_id).await? {
        return Ok(Some(exercise));
    }
    exercise::load_user(db, user_id, exercise_id).await
}

fn query_filter(request: &Request) -> Result<GripFilter> {
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

fn current_date() -> Option<JstDate> {
    JstDate::parse(&warrior_core::datetime::japan_date(auth::now_seconds()))
}

fn parse_date(text: &str) -> Result<JstDate> {
    JstDate::parse(text).ok_or_else(|| {
        console_error!("the stored training date is invalid");
        worker::Error::RustError("the stored training date is invalid".to_owned())
    })
}

impl GoalRow {
    fn into_goal(self) -> Result<ExerciseGoal> {
        let target_date = self
            .target_date
            .as_deref()
            .map(|text| {
                JstDate::parse(text).ok_or_else(|| {
                    console_error!("the stored target date is invalid");
                    worker::Error::RustError("the stored target date is invalid".to_owned())
                })
            })
            .transpose()?;
        Ok(ExerciseGoal {
            exercise_id: self.exercise_id,
            grip: optional_text(&self.grip),
            attachment: optional_text(&self.attachment),
            target_weight_kg: self.target_weight_kg,
            target_date,
            current_weight_kg: None,
        })
    }
}

fn optional_text(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn key_text(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

fn optional_date(value: Option<JstDate>) -> SqlValue {
    value.map_or(SqlValue::Null, |date| SqlValue::Text(date.to_string()))
}

#[cfg(test)]
mod tests {
    use warrior_core::exercise::{
        BodyPart, Equipment, Exercise, ExerciseSnapshot, GripMode, GripSelection, GripStyle,
        GripWidth, Laterality,
    };
    use warrior_core::goal::{ExerciseGoal, MAX_TARGET_WEIGHT_KG};
    use warrior_core::history::GripFilter;
    use warrior_core::jst::JstDate;
    use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};
    use warrior_core::sql::SqlValue;

    use super::{
        delete_statement, statuses, upsert_statement, validated_goal, GoalData, GoalInput, GoalRow,
    };
    use crate::history::ExerciseTrainingSets;
    use crate::stored_sets::StoredExercise;

    #[test]
    fn a_goal_is_validated_with_the_exercise_selection_and_the_weight_range() {
        let exercise = exercise(GripMode::Grip);
        let today = date(2026, 11, 12);
        let input = GoalInput {
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
            target_weight_kg: 100.0,
            target_date: Some(date(2026, 12, 1)),
        };
        let goal = validated_goal(input, &exercise, today).expect("a valid goal");
        assert_eq!(goal.grip.as_deref(), Some("pronated_medium"));
        assert_eq!(goal.target_weight_kg, 100.0);
        assert_eq!(goal.target_date, Some(date(2026, 12, 1)));

        let invalid_selection = GoalInput {
            grip: None,
            attachment: None,
            target_weight_kg: 100.0,
            target_date: None,
        };
        assert!(validated_goal(invalid_selection, &exercise, today).is_none());
        let too_heavy = GoalInput {
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
            target_weight_kg: MAX_TARGET_WEIGHT_KG + 0.1,
            target_date: None,
        };
        assert!(validated_goal(too_heavy, &exercise, today).is_none());
    }

    #[test]
    fn the_upsert_keeps_one_row_per_user_and_comparison_key() {
        let goal = ExerciseGoal {
            exercise_id: "bench".to_owned(),
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
            target_weight_kg: 100.0,
            target_date: Some(date(2026, 12, 1)),
            current_weight_kg: None,
        };
        let statement = upsert_statement("user-a", &goal, "2026-11-12T00:00:00Z");
        assert!(statement
            .sql()
            .contains("ON CONFLICT(user_id, exercise_id, grip, attachment) DO UPDATE"));
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Text("bench".to_owned()),
                SqlValue::Text("pronated_medium".to_owned()),
                SqlValue::Text(String::new()),
                SqlValue::Real(100.0),
                SqlValue::Text("2026-12-01".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
            ]
        );
    }

    #[test]
    fn the_delete_uses_the_user_and_the_comparison_key() {
        let filter = GripFilter::new(None, Some("wide_bar".to_owned()));
        let statement = delete_statement("user-a", "bench", &filter);
        assert!(statement
            .sql()
            .contains("WHERE user_id = ? AND exercise_id = ? AND grip = ? AND attachment = ?"));
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Text("bench".to_owned()),
                SqlValue::Text(String::new()),
                SqlValue::Text("wide_bar".to_owned()),
            ]
        );
    }

    #[test]
    fn the_history_goal_statuses_use_the_comparison_key_and_the_current_record() {
        let filter = GripFilter::new(Some("pronated_medium".to_owned()), None);
        let progress = statuses(
            GripMode::Grip,
            &filter,
            GoalData {
                rows: vec![goal_row()],
                bests: Vec::new(),
            },
            &[training_sets()],
            date(2026, 11, 13),
        )
        .expect("the goal status");
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0].goal.target_weight_kg, 120.0);
        assert_eq!(progress[0].current_weight_kg, Some(100.0));
        assert_eq!(progress[0].difference_kg, Some(20.0));

        let other = GripFilter::new(Some("supinated_narrow".to_owned()), None);
        let progress = statuses(
            GripMode::Grip,
            &other,
            GoalData {
                rows: vec![goal_row()],
                bests: Vec::new(),
            },
            &[training_sets()],
            date(2026, 11, 13),
        )
        .expect("the filtered goal status");
        assert!(progress.is_empty());
    }

    fn training_sets() -> ExerciseTrainingSets {
        ExerciseTrainingSets {
            training_id: "training".to_owned(),
            date: "2026-11-12".to_owned(),
            finished_at: "2026-11-12T03:00:00Z".to_owned(),
            exercises: vec![StoredExercise {
                training_id: "training".to_owned(),
                snapshot: ExerciseSnapshot {
                    exercise_id: "bench".to_owned(),
                    name: "ベンチプレス".to_owned(),
                    body_part: BodyPart::Chest,
                    muscles: Vec::new(),
                    record_mode: RecordMode::WeightAndReps,
                    laterality: Laterality::Bilateral,
                    grip_mode: GripMode::Grip,
                    grip_options: Vec::new(),
                },
                sets: vec![weight_record()],
                memo: String::new(),
                set_grips: Vec::new(),
                set_body_weights: Vec::new(),
            }],
        }
    }

    fn weight_record() -> SetRecord {
        SetRecord::new(
            BestKey {
                exercise_id: "bench".to_owned(),
                grip: Some("pronated_medium".to_owned()),
                attachment: None,
            },
            SetKind::Normal,
            vec![SetSegment {
                weight_kg: 100.0,
                reps: 5,
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
        .expect("a valid set")
    }

    fn goal_row() -> GoalRow {
        GoalRow {
            exercise_id: "bench".to_owned(),
            grip: "pronated_medium".to_owned(),
            attachment: String::new(),
            target_weight_kg: 120.0,
            target_date: Some("2026-12-01".to_owned()),
        }
    }

    fn exercise(grip_mode: GripMode) -> Exercise {
        Exercise {
            id: "bench".to_owned(),
            name: "ベンチプレス".to_owned(),
            name_kana: "べんちぷれす".to_owned(),
            body_part: BodyPart::Chest,
            equipment: Equipment::Barbell,
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode,
            other_scope: None,
            instructions: String::new(),
            cautions: String::new(),
            tips: String::new(),
            sort_order: 1,
            muscles: Vec::new(),
            grip_options: vec![GripSelection::Grip {
                style: GripStyle::Pronated,
                width: GripWidth::Medium,
            }],
        }
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }
}
