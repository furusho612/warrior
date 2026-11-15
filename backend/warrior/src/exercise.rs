//! 種目マスター、独自種目、上書きの D1 の読み書き。

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use warrior_core::exercise::{
    Exercise, ExerciseInput, ExerciseSnapshot, GripSelection, MuscleAssignment, MuscleRole,
};
use warrior_core::exercise_override::{apply_override, ExerciseOverride};
use warrior_core::library::ExerciseSource;
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
    source: String,
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

#[derive(Debug, Deserialize)]
struct OverrideRow {
    master_exercise_id: String,
    name: Option<String>,
    name_kana: Option<String>,
    body_part: Option<String>,
    equipment: Option<String>,
    record_mode: Option<String>,
    laterality: Option<String>,
    grip_mode: Option<String>,
    other_scope: Option<String>,
    instructions: Option<String>,
    cautions: Option<String>,
    tips: Option<String>,
    sort_order: Option<i64>,
    grip_options: Option<String>,
    muscles: Option<String>,
    deleted: i64,
}

/// マスターの種目を一覧の順に読む。
///
/// 記録への種目の選択と、種目ライブラリの一覧が使う。筋肉は 1 回のクエリで読み、
/// 種目ごとにまとめる。
pub async fn list_master(db: &Db) -> Result<Vec<(Exercise, ExerciseSource)>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
             grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options, \
             source FROM master_exercises ORDER BY sort_order, id",
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
            let assignments = assignments_for(&muscles, &id)?;
            to_sourced_exercise(row, assignments)
        })
        .collect()
}

/// マスターの種目を読む。
pub async fn load_master(db: &Db, id: &str) -> Result<Option<Exercise>> {
    Ok(load_master_with_source(db, id)
        .await?
        .map(|(exercise, _)| exercise))
}

/// マスターの種目を出典と一緒に読む。
pub async fn load_master_with_source(
    db: &Db,
    id: &str,
) -> Result<Option<(Exercise, ExerciseSource)>> {
    let statement = SqlStatement::new(
        "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
         grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options, \
         source FROM master_exercises WHERE id = ?",
        vec![SqlValue::Text(id.to_owned())],
    );
    let Some(row) = db.first::<ExerciseRow>(&statement).await? else {
        return Ok(None);
    };
    let muscles = load_muscles(db, "master_exercise_muscles", id).await?;
    Ok(Some(to_sourced_exercise(row, muscles)?))
}

/// 利用者の独自種目を一覧の順に読む。
pub async fn list_user(db: &Db, user_id: &str) -> Result<Vec<(Exercise, ExerciseSource)>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
             grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options, \
             source FROM user_exercises WHERE user_id = ? ORDER BY sort_order, id",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<ExerciseRow>()?;
    let muscles = db
        .all(&SqlStatement::new(
            "SELECT exercise_id, muscle_id, role FROM user_exercise_muscles \
             WHERE exercise_id IN (SELECT id FROM user_exercises WHERE user_id = ?) \
             ORDER BY exercise_id, role, muscle_id",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<MuscleAssignmentRow>()?;
    rows.into_iter()
        .map(|row| {
            let id = row.id.clone();
            let assignments = assignments_for(&muscles, &id)?;
            to_sourced_exercise(row, assignments)
        })
        .collect()
}

/// 利用者の独自種目を読む。
pub async fn load_user(db: &Db, user_id: &str, id: &str) -> Result<Option<Exercise>> {
    Ok(load_user_with_source(db, user_id, id)
        .await?
        .map(|(exercise, _)| exercise))
}

/// 利用者の独自種目を出典と一緒に読む。
pub async fn load_user_with_source(
    db: &Db,
    user_id: &str,
    id: &str,
) -> Result<Option<(Exercise, ExerciseSource)>> {
    let statement = SqlStatement::new(
        "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
         grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options, \
         source FROM user_exercises WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    );
    let Some(row) = db.first::<ExerciseRow>(&statement).await? else {
        return Ok(None);
    };
    let muscles = load_muscles(db, "user_exercise_muscles", id).await?;
    Ok(Some(to_sourced_exercise(row, muscles)?))
}

/// 独自種目の所有者を読む。他の利用者の種目も ID だけで引ける。
pub async fn load_user_owner(db: &Db, id: &str) -> Result<Option<String>> {
    let row = db
        .first::<ExerciseUserRow>(&SqlStatement::new(
            "SELECT user_id FROM user_exercises WHERE id = ?",
            vec![SqlValue::Text(id.to_owned())],
        ))
        .await?;
    Ok(row.map(|row| row.user_id))
}

/// マスターの種目への上書きを読む。
pub async fn load_override(
    db: &Db,
    user_id: &str,
    master_exercise_id: &str,
) -> Result<Option<ExerciseOverride>> {
    let row = db
        .first::<OverrideRow>(&SqlStatement::new(
            "SELECT master_exercise_id, name, name_kana, body_part, equipment, record_mode, \
             laterality, grip_mode, other_scope, instructions, cautions, tips, sort_order, \
             grip_options, muscles, deleted FROM master_exercise_overrides \
             WHERE user_id = ? AND master_exercise_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(master_exercise_id.to_owned()),
            ],
        ))
        .await?;
    row.map(to_override).transpose()
}

/// 利用者のマスター種目への上書きをすべて読む。
pub async fn list_overrides(db: &Db, user_id: &str) -> Result<HashMap<String, ExerciseOverride>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT master_exercise_id, name, name_kana, body_part, equipment, record_mode, \
             laterality, grip_mode, other_scope, instructions, cautions, tips, sort_order, \
             grip_options, muscles, deleted FROM master_exercise_overrides WHERE user_id = ?",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<OverrideRow>()?;
    rows.into_iter()
        .map(|row| Ok((row.master_exercise_id.clone(), to_override(row)?)))
        .collect()
}

/// マスターの種目に上書きを適用する。削除の上書きなら `None`。
pub fn effective(
    master: &Exercise,
    value: Option<&ExerciseOverride>,
) -> Option<(Exercise, ExerciseSource)> {
    let Some(value) = value else {
        return Some((master.clone(), ExerciseSource::Master));
    };
    apply_override(master, value).map(|exercise| (exercise, ExerciseSource::Overridden))
}

/// 利用者の独自種目を保存する。同じ ID は更新する。
pub async fn save_user(db: &Db, user_id: &str, exercise: &Exercise) -> Result<()> {
    let statements = user_statements(user_id, exercise)?;
    db.batch(&statements).await?;
    Ok(())
}

/// 利用者の独自種目を保存する文の組。
pub fn user_statements(user_id: &str, exercise: &Exercise) -> Result<Vec<SqlStatement>> {
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
    Ok(statements)
}

/// 独自種目を削除する。筋肉とお気に入りも同じ `batch()` で消す。
pub async fn delete_user(db: &Db, user_id: &str, id: &str) -> Result<()> {
    db.batch(&delete_user_statements(user_id, id)).await?;
    Ok(())
}

/// 独自種目を削除する文の組。
pub fn delete_user_statements(user_id: &str, id: &str) -> Vec<SqlStatement> {
    vec![
        SqlStatement::new(
            "DELETE FROM user_exercise_muscles WHERE exercise_id = ?",
            vec![SqlValue::Text(id.to_owned())],
        ),
        SqlStatement::new(
            "DELETE FROM exercise_favorites \
             WHERE user_id = ? AND exercise_kind = 'user' AND exercise_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(id.to_owned()),
            ],
        ),
        SqlStatement::new(
            "DELETE FROM user_exercises WHERE id = ? AND user_id = ?",
            vec![
                SqlValue::Text(id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ),
    ]
}

/// マスター種目の利用者ごとの上書きを保存する。
///
/// `value` の `None` の項目は列を NULL にし、マスターの値をそのまま使う。
/// 初期の内容に戻すときは `clear_override` を使う。
pub async fn save_override(
    db: &Db,
    user_id: &str,
    master_exercise_id: &str,
    value: &ExerciseOverride,
) -> Result<()> {
    db.run(&override_statement(user_id, master_exercise_id, value)?)
        .await?;
    Ok(())
}

/// 上書きを保存する文を作る。
pub fn override_statement(
    user_id: &str,
    master_exercise_id: &str,
    value: &ExerciseOverride,
) -> Result<SqlStatement> {
    let now = warrior_core::datetime::format_utc((worker::Date::now().as_millis() / 1000) as i64);
    let grip_options = value
        .grip_options
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| Error::RustError(format!("grip options are not serializable: {error}")))?;
    let muscles = value
        .muscles
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| Error::RustError(format!("muscles are not serializable: {error}")))?;
    Ok(SqlStatement::new(
        "INSERT INTO master_exercise_overrides \
         (user_id, master_exercise_id, name, name_kana, body_part, equipment, record_mode, \
          laterality, grip_mode, other_scope, instructions, cautions, tips, sort_order, \
          grip_options, muscles, deleted, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, master_exercise_id) DO UPDATE SET \
          name = excluded.name, name_kana = excluded.name_kana, body_part = excluded.body_part, \
          equipment = excluded.equipment, record_mode = excluded.record_mode, \
          laterality = excluded.laterality, grip_mode = excluded.grip_mode, \
          other_scope = excluded.other_scope, instructions = excluded.instructions, \
          cautions = excluded.cautions, tips = excluded.tips, sort_order = excluded.sort_order, \
          grip_options = excluded.grip_options, muscles = excluded.muscles, \
          deleted = excluded.deleted, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(master_exercise_id.to_owned()),
            optional_text(value.name.as_deref()),
            optional_text(value.name_kana.as_deref()),
            optional_text(value.body_part.map(|body_part| body_part.as_str())),
            optional_text(value.equipment.map(|equipment| equipment.as_str())),
            optional_text(value.record_mode.map(record_mode_text).as_deref()),
            optional_text(value.laterality.map(laterality_text).as_deref()),
            optional_text(value.grip_mode.map(grip_mode_text).as_deref()),
            optional_text(value.other_scope.map(other_scope_text).as_deref()),
            optional_text(value.instructions.as_deref()),
            optional_text(value.cautions.as_deref()),
            optional_text(value.tips.as_deref()),
            value.sort_order.map_or(SqlValue::Null, |sort_order| {
                SqlValue::Integer(i64::from(sort_order))
            }),
            optional_text(grip_options.as_deref()),
            optional_text(muscles.as_deref()),
            SqlValue::Integer(i64::from(value.deleted)),
            SqlValue::Text(now),
        ],
    ))
}

/// 上書きを消し、マスターの元の内容に戻す。
pub async fn clear_override(db: &Db, user_id: &str, master_exercise_id: &str) -> Result<()> {
    db.run(&clear_override_statement(user_id, master_exercise_id))
        .await?;
    Ok(())
}

/// 上書きを消す文。
pub fn clear_override_statement(user_id: &str, master_exercise_id: &str) -> SqlStatement {
    SqlStatement::new(
        "DELETE FROM master_exercise_overrides WHERE user_id = ? AND master_exercise_id = ?",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(master_exercise_id.to_owned()),
        ],
    )
}

/// 操作の適用結果を読む。同じ操作 ID の再送では前回の種目 ID を使う。
pub async fn applied_exercise_operation(
    db: &Db,
    operation_id: &str,
) -> Result<Option<AppliedExerciseOperation>> {
    db.first::<AppliedExerciseOperation>(&SqlStatement::new(
        "SELECT user_id, exercise_id, kind FROM exercise_operations WHERE operation_id = ?",
        vec![SqlValue::Text(operation_id.to_owned())],
    ))
    .await
}

/// 操作を適用済みとして記録する文。
pub fn operation_statement(
    operation_id: &str,
    user_id: &str,
    exercise_id: &str,
    kind: &str,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO exercise_operations (operation_id, user_id, exercise_id, kind, created_at) \
         VALUES (?, ?, ?, ?, ?)",
        vec![
            SqlValue::Text(operation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(exercise_id.to_owned()),
            SqlValue::Text(kind.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

/// 適用済みの操作の行。
#[derive(Debug, Clone, Deserialize)]
pub struct AppliedExerciseOperation {
    pub user_id: String,
    pub exercise_id: String,
    pub kind: String,
}

/// 種目のスナップショットを保存用に組み立てる。
pub fn snapshot(exercise: &Exercise) -> ExerciseSnapshot {
    ExerciseSnapshot::from(exercise)
}

/// 入力から独自種目を組み立てる。ID と並び順は呼び出し側が決める。
pub fn exercise_from_input(input: ExerciseInput, id: String) -> Exercise {
    input.into_exercise(id, 0)
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

fn assignments_for(
    muscles: &[MuscleAssignmentRow],
    exercise_id: &str,
) -> Result<Vec<MuscleAssignment>> {
    muscles
        .iter()
        .filter(|muscle| muscle.exercise_id == exercise_id)
        .map(|muscle| {
            Ok(MuscleAssignment {
                muscle: parse(&muscle.muscle_id)?,
                role: parse(&muscle.role)?,
            })
        })
        .collect()
}

fn to_sourced_exercise(
    row: ExerciseRow,
    muscles: Vec<MuscleAssignment>,
) -> Result<(Exercise, ExerciseSource)> {
    let source = parse_source(&row.source)?;
    Ok((to_exercise(row, muscles)?, source))
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

fn to_override(row: OverrideRow) -> Result<ExerciseOverride> {
    Ok(ExerciseOverride {
        name: row.name,
        name_kana: row.name_kana,
        body_part: row.body_part.as_deref().map(parse).transpose()?,
        equipment: row.equipment.as_deref().map(parse).transpose()?,
        record_mode: row.record_mode.as_deref().map(parse).transpose()?,
        laterality: row.laterality.as_deref().map(parse).transpose()?,
        grip_mode: row.grip_mode.as_deref().map(parse).transpose()?,
        other_scope: row.other_scope.as_deref().map(parse).transpose()?,
        instructions: row.instructions,
        cautions: row.cautions,
        tips: row.tips,
        sort_order: row
            .sort_order
            .map(|sort_order| {
                i32::try_from(sort_order)
                    .map_err(|_| Error::RustError("the sort order does not fit in i32".to_owned()))
            })
            .transpose()?,
        muscles: row
            .muscles
            .as_deref()
            .map(serde_json::from_str::<Vec<MuscleAssignment>>)
            .transpose()
            .map_err(|error| Error::RustError(format!("muscles are invalid: {error}")))?,
        grip_options: row
            .grip_options
            .as_deref()
            .map(serde_json::from_str::<Vec<GripSelection>>)
            .transpose()
            .map_err(|error| Error::RustError(format!("grip options are invalid: {error}")))?,
        deleted: row.deleted != 0,
    })
}

fn parse_source(text: &str) -> Result<ExerciseSource> {
    match text {
        "master" => Ok(ExerciseSource::Master),
        "custom" => Ok(ExerciseSource::Custom),
        "overridden" => Ok(ExerciseSource::Overridden),
        _ => Err(Error::RustError(format!("the source {text} is invalid"))),
    }
}

fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_value(Value::String(text.to_owned()))
        .map_err(|error| Error::RustError(format!("the value {text} is invalid: {error}")))
}

fn optional_text(text: Option<&str>) -> SqlValue {
    text.map_or(SqlValue::Null, |text| SqlValue::Text(text.to_owned()))
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
