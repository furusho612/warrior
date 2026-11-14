//! 種目マスター、独自種目、上書きの D1 の読み書き。

use serde::de::DeserializeOwned;
use serde::Deserialize;

use serde_json::Value;
use warrior_core::exercise::{
    Exercise, ExerciseSnapshot, GripSelection, MuscleAssignment, MuscleRole,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{Error, Result};

use crate::db::Db;

#[derive(Debug, Deserialize)]
struct ExerciseRow {
    id: String,
    name: String,
    name_kana: String,
    body_part: String,
    equipment: String,
    record_mode: String,
    laterality: String,
    grip_mode: String,
    other_scope: Option<String>,
    instructions: String,
    cautions: String,
    tips: String,
    sort_order: i64,
    grip_options: String,
}

#[derive(Debug, Deserialize)]
struct ExerciseUserRow {
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct MuscleRow {
    muscle_id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
struct MuscleAssignmentRow {
    exercise_id: String,
    muscle_id: String,
    role: String,
}

/// マスターの種目を一覧の順に読む。
///
/// 記録への種目の選択と、種目ライブラリの一覧が使う。筋肉は 1 回のクエリで読み、
/// 種目ごとにまとめる。
pub async fn list_master(db: &Db) -> Result<Vec<Exercise>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
             grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options \
             FROM master_exercises ORDER BY sort_order, id",
            Vec::new(),
        ))
        .await?
        .results::<ExerciseRow>()?;
    let muscles = db
        .all(&SqlStatement::new(
            "SELECT exercise_id, muscle_id, role FROM master_exercise_muscles \
             ORDER BY exercise_id, role, muscle_id",
            Vec::new(),
        ))
        .await?
        .results::<MuscleAssignmentRow>()?;
    rows.into_iter()
        .map(|row| {
            let id = row.id.clone();
            let assignments = muscles
                .iter()
                .filter(|muscle| muscle.exercise_id == id)
                .map(|muscle| {
                    Ok(MuscleAssignment {
                        muscle: parse(&muscle.muscle_id)?,
                        role: parse(&muscle.role)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            to_exercise(row, assignments)
        })
        .collect()
}

/// マスターの種目を読む。
pub async fn load_master(db: &Db, id: &str) -> Result<Option<Exercise>> {
    let statement = SqlStatement::new(
        "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
         grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options \
         FROM master_exercises WHERE id = ?",
        vec![SqlValue::Text(id.to_owned())],
    );
    let Some(row) = db.first::<ExerciseRow>(&statement).await? else {
        return Ok(None);
    };
    let muscles = load_muscles(db, "master_exercise_muscles", id).await?;
    Ok(Some(to_exercise(row, muscles)?))
}

/// 利用者の独自種目を読む。
pub async fn load_user(db: &Db, user_id: &str, id: &str) -> Result<Option<Exercise>> {
    let statement = SqlStatement::new(
        "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
         grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options \
         FROM user_exercises WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    );
    let Some(row) = db.first::<ExerciseRow>(&statement).await? else {
        return Ok(None);
    };
    let muscles = load_muscles(db, "user_exercise_muscles", id).await?;
    Ok(Some(to_exercise(row, muscles)?))
}

/// 利用者の独自種目を保存する。同じ ID は更新する。
pub async fn save_user(db: &Db, user_id: &str, exercise: &Exercise) -> Result<()> {
    let existing = SqlStatement::new(
        "SELECT user_id FROM user_exercises WHERE id = ?",
        vec![SqlValue::Text(exercise.id.clone())],
    );
    if let Some(row) = db.first::<ExerciseUserRow>(&existing).await? {
        if row.user_id != user_id {
            return Err(Error::RustError(
                "the exercise belongs to another user".to_owned(),
            ));
        }
    }
    exercise
        .validate()
        .map_err(|error| Error::RustError(format!("the exercise is invalid: {error:?}")))?;
    let grip_options = serde_json::to_string(&exercise.grip_options)
        .map_err(|error| Error::RustError(format!("grip options are not serializable: {error}")))?;
    let now = warrior_core::datetime::format_utc((worker::Date::now().as_millis() / 1000) as i64);
    let insert = SqlStatement::new(
        "INSERT INTO user_exercises \
         (id, user_id, name, name_kana, body_part, equipment, record_mode, laterality, \
          grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, name_kana = excluded.name_kana, \
          body_part = excluded.body_part, equipment = excluded.equipment, \
          record_mode = excluded.record_mode, laterality = excluded.laterality, \
          grip_mode = excluded.grip_mode, other_scope = excluded.other_scope, \
          instructions = excluded.instructions, cautions = excluded.cautions, \
          tips = excluded.tips, sort_order = excluded.sort_order, \
          grip_options = excluded.grip_options, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(exercise.id.clone()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(exercise.name.clone()),
            SqlValue::Text(exercise.name_kana.clone()),
            SqlValue::Text(exercise.body_part.as_str().to_owned()),
            SqlValue::Text(exercise.equipment.as_str().to_owned()),
            SqlValue::Text(record_mode_text(exercise.record_mode)),
            SqlValue::Text(laterality_text(exercise.laterality)),
            SqlValue::Text(grip_mode_text(exercise.grip_mode)),
            exercise.other_scope.map_or(SqlValue::Null, |scope| {
                SqlValue::Text(other_scope_text(scope))
            }),
            SqlValue::Text(exercise.instructions.clone()),
            SqlValue::Text(exercise.cautions.clone()),
            SqlValue::Text(exercise.tips.clone()),
            SqlValue::Integer(i64::from(exercise.sort_order)),
            SqlValue::Text(grip_options),
            SqlValue::Text(now.clone()),
            SqlValue::Text(now),
        ],
    );
    let delete_muscles = SqlStatement::new(
        "DELETE FROM user_exercise_muscles WHERE exercise_id = ?",
        vec![SqlValue::Text(exercise.id.clone())],
    );
    let muscle_statements = exercise
        .muscles
        .iter()
        .map(|assignment| {
            SqlStatement::new(
                "INSERT INTO user_exercise_muscles (exercise_id, muscle_id, role) VALUES (?, ?, ?)",
                vec![
                    SqlValue::Text(exercise.id.clone()),
                    SqlValue::Text(assignment.muscle.id().to_owned()),
                    SqlValue::Text(muscle_role_text(assignment.role)),
                ],
            )
        })
        .collect::<Vec<_>>();
    let mut statements = vec![insert, delete_muscles];
    statements.extend(muscle_statements);
    db.batch(&statements).await?;
    Ok(())
}

/// マスター種目の利用者ごとの上書きを保存する。`exercise` が None なら削除の上書き。
pub async fn save_override(
    db: &Db,
    user_id: &str,
    master_exercise_id: &str,
    exercise: Option<&Exercise>,
    deleted: bool,
) -> Result<()> {
    let now = warrior_core::datetime::format_utc((worker::Date::now().as_millis() / 1000) as i64);
    if deleted {
        let statement = SqlStatement::new(
            "INSERT INTO master_exercise_overrides (user_id, master_exercise_id, deleted, updated_at) \
             VALUES (?, ?, 1, ?) \
             ON CONFLICT(user_id, master_exercise_id) DO UPDATE SET deleted = 1, updated_at = excluded.updated_at",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(master_exercise_id.to_owned()),
                SqlValue::Text(now),
            ],
        );
        db.run(&statement).await?;
        return Ok(());
    }
    let Some(exercise) = exercise else {
        return Err(Error::RustError("an override needs an exercise".to_owned()));
    };
    exercise
        .validate()
        .map_err(|error| Error::RustError(format!("the exercise is invalid: {error:?}")))?;
    let grip_options = serde_json::to_string(&exercise.grip_options)
        .map_err(|error| Error::RustError(format!("grip options are not serializable: {error}")))?;
    let muscles = serde_json::to_string(&exercise.muscles)
        .map_err(|error| Error::RustError(format!("muscles are not serializable: {error}")))?;
    let statement = SqlStatement::new(
        "INSERT INTO master_exercise_overrides \
         (user_id, master_exercise_id, name, name_kana, body_part, equipment, record_mode, \
          laterality, grip_mode, other_scope, instructions, cautions, tips, sort_order, \
          grip_options, muscles, deleted, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?) \
         ON CONFLICT(user_id, master_exercise_id) DO UPDATE SET \
          name = excluded.name, name_kana = excluded.name_kana, body_part = excluded.body_part, \
          equipment = excluded.equipment, record_mode = excluded.record_mode, \
          laterality = excluded.laterality, grip_mode = excluded.grip_mode, \
          other_scope = excluded.other_scope, instructions = excluded.instructions, \
          cautions = excluded.cautions, tips = excluded.tips, sort_order = excluded.sort_order, \
          grip_options = excluded.grip_options, muscles = excluded.muscles, deleted = 0, \
          updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(master_exercise_id.to_owned()),
            SqlValue::Text(exercise.name.clone()),
            SqlValue::Text(exercise.name_kana.clone()),
            SqlValue::Text(exercise.body_part.as_str().to_owned()),
            SqlValue::Text(exercise.equipment.as_str().to_owned()),
            SqlValue::Text(record_mode_text(exercise.record_mode)),
            SqlValue::Text(laterality_text(exercise.laterality)),
            SqlValue::Text(grip_mode_text(exercise.grip_mode)),
            exercise.other_scope.map_or(SqlValue::Null, |scope| {
                SqlValue::Text(other_scope_text(scope))
            }),
            SqlValue::Text(exercise.instructions.clone()),
            SqlValue::Text(exercise.cautions.clone()),
            SqlValue::Text(exercise.tips.clone()),
            SqlValue::Integer(i64::from(exercise.sort_order)),
            SqlValue::Text(grip_options),
            SqlValue::Text(muscles),
            SqlValue::Text(now),
        ],
    );
    db.run(&statement).await?;
    Ok(())
}

/// 種目のスナップショットを保存用に組み立てる。
pub fn snapshot(exercise: &Exercise) -> ExerciseSnapshot {
    ExerciseSnapshot::from(exercise)
}

async fn load_muscles(db: &Db, table: &str, exercise_id: &str) -> Result<Vec<MuscleAssignment>> {
    let statement = SqlStatement::new(
        format!("SELECT muscle_id, role FROM {table} WHERE exercise_id = ?"),
        vec![SqlValue::Text(exercise_id.to_owned())],
    );
    let rows = db.all(&statement).await?.results::<MuscleRow>()?;
    rows.into_iter()
        .map(|row| {
            Ok(MuscleAssignment {
                muscle: parse(&row.muscle_id)?,
                role: parse(&row.role)?,
            })
        })
        .collect()
}

fn to_exercise(row: ExerciseRow, muscles: Vec<MuscleAssignment>) -> Result<Exercise> {
    Ok(Exercise {
        id: row.id,
        name: row.name,
        name_kana: row.name_kana,
        body_part: parse(&row.body_part)?,
        equipment: parse(&row.equipment)?,
        record_mode: parse(&row.record_mode)?,
        laterality: parse(&row.laterality)?,
        grip_mode: parse(&row.grip_mode)?,
        other_scope: row.other_scope.as_deref().map(parse).transpose()?,
        instructions: row.instructions,
        cautions: row.cautions,
        tips: row.tips,
        sort_order: i32::try_from(row.sort_order)
            .map_err(|_| Error::RustError("the sort order does not fit in i32".to_owned()))?,
        muscles,
        grip_options: serde_json::from_str::<Vec<GripSelection>>(&row.grip_options)
            .map_err(|error| Error::RustError(format!("grip options are invalid: {error}")))?,
    })
}

fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_value(Value::String(text.to_owned()))
        .map_err(|error| Error::RustError(format!("the value {text} is invalid: {error}")))
}

fn record_mode_text(mode: warrior_core::set_calc::RecordMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn laterality_text(value: warrior_core::exercise::Laterality) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn grip_mode_text(value: warrior_core::exercise::GripMode) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn other_scope_text(value: warrior_core::exercise::OtherScope) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn muscle_role_text(value: MuscleRole) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
