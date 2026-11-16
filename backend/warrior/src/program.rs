//! プログラムの構成、推奨、取り込み、進行の HTTP ハンドラ。
//!
//! プログラムと日はマスターの表に置く。取り込みは日ごとのルーティンを利用者の
//! ルーティンとして作り、`source_program_day_id` で元の日を指す。
//! 進行は `user_program_progress` の 1 行で持ち、トレーニングの終了で次の日へ進む。

use serde::{Deserialize, Serialize};
use warrior_core::exercise::{Attachment, GripSelection};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::SessionSource;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::profile;

#[derive(Debug, Deserialize)]
struct ProgramRow {
    id: String,
    goal: String,
    level: String,
    days_per_week: u32,
    name: String,
    sort_order: i64,
}

#[derive(Debug, Deserialize)]
struct ProgramDayRow {
    id: String,
    program_id: String,
    position: i64,
    name: String,
    exercises: String,
}

#[derive(Debug, Deserialize)]
struct ProgressRow {
    program_id: Option<String>,
    custom_program_id: Option<String>,
    day_position: i64,
}

#[derive(Debug, Deserialize)]
struct ExerciseSeed {
    name: String,
    sets: u32,
    #[serde(default)]
    reps: Option<u32>,
    #[serde(default)]
    duration: Option<u32>,
    #[serde(default)]
    rest: Option<u32>,
    #[serde(default)]
    attachment: Option<String>,
}

/// プログラムの概要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramSummary {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub level: String,
    pub days_per_week: u32,
}

/// プログラムの日の種目。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgramDayExercise {
    pub exercise_id: String,
    pub name: String,
    pub record_mode: String,
    pub laterality: String,
    pub grip_mode: String,
    pub grip_options: Vec<GripSelection>,
    pub set_count: u32,
    pub reps: Option<u32>,
    pub duration_seconds: Option<u32>,
    pub rest_seconds: u32,
}

/// プログラムの日。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgramDay {
    pub id: String,
    pub name: String,
    pub position: i64,
    pub exercises: Vec<ProgramDayExercise>,
}

/// 進行中のプログラム。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CurrentProgram {
    pub program: ProgramSummary,
    pub routine_id: String,
    pub day: ProgramDay,
    pub day_position: i64,
    pub total_days: i64,
}

/// プログラムの一覧の応答。
#[derive(Debug, Serialize)]
pub struct ProgramsResponse {
    pub recommended: Vec<ProgramSummary>,
    pub references: Vec<ProgramSummary>,
    pub current: Option<CurrentProgram>,
}

/// `GET /api/programs`
pub async fn list(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let profile = profile::load(&db, &authenticated.id).await?;
    let (recommended, references) = recommendation(&db, profile.as_ref()).await?;
    let current = current_program(&db, &authenticated.id).await?;
    Response::from_json(&ProgramsResponse {
        recommended,
        references,
        current,
    })
}

/// `GET /api/programs/current`
pub async fn current(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    match current_program(&db, &authenticated.id).await? {
        Some(current) => Response::from_json(&current),
        None => Response::from_json(&serde_json::Value::Null),
    }
}

/// `POST /api/programs/:id/select`
pub async fn select(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(program_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(program) = load_program(&db, &program_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let days = load_days(&db, &program_id).await?;
    if days.is_empty() {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    if let Some(progress) = load_progress(&db, &authenticated.id).await? {
        if progress.program_id.as_deref() == Some(program_id.as_str()) {
            if let Some(current) = current_program(&db, &authenticated.id).await? {
                return Response::from_json(&current);
            }
        }
    }
    let statements = match import_statements(&db, &authenticated.id, &program, &days).await {
        Ok(statements) => statements,
        Err(error) => {
            console_error!("failed to build the program import: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if let Err(error) = db.batch(&statements).await {
        console_error!("failed to import the program: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    match current_program(&db, &authenticated.id).await? {
        Some(current) => Response::from_json(&current),
        None => Ok(response(ErrorCode::Internal, "internal error")),
    }
}

/// `DELETE /api/programs/current`
pub async fn stop(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    db.run(&SqlStatement::new(
        "DELETE FROM user_program_progress WHERE user_id = ?",
        vec![SqlValue::Text(authenticated.id)],
    ))
    .await?;
    Ok(Response::empty()?.with_status(204))
}

async fn recommendation(
    db: &Db,
    profile: Option<&warrior_core::profile::Profile>,
) -> Result<(Vec<ProgramSummary>, Vec<ProgramSummary>)> {
    let Some(profile) = profile else {
        return Ok((Vec::new(), Vec::new()));
    };
    let (Some(goal), Some(level), Some(days)) = (
        profile.goal,
        profile.training_experience,
        profile.training_days_per_week,
    ) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let programs = load_programs(db).await?;
    let choices = programs
        .iter()
        .map(|program| warrior_core::program::ProgramChoice {
            id: program.id.clone(),
            goal: program.goal.clone(),
            level: program.level.clone(),
            days_per_week: program.days_per_week,
            sort_order: program.sort_order,
        })
        .collect::<Vec<_>>();
    let recommendation =
        warrior_core::program::recommend(&choices, goal.as_str(), level.as_str(), days);
    let selected = |ids: &[String]| {
        programs
            .iter()
            .filter(|program| ids.contains(&program.id))
            .map(summary)
            .collect::<Vec<_>>()
    };
    Ok((
        selected(&recommendation.recommended),
        selected(&recommendation.references),
    ))
}

fn summary(program: &ProgramRow) -> ProgramSummary {
    ProgramSummary {
        id: program.id.clone(),
        name: program.name.clone(),
        goal: program.goal.clone(),
        level: program.level.clone(),
        days_per_week: program.days_per_week,
    }
}

async fn current_program(db: &Db, user_id: &str) -> Result<Option<CurrentProgram>> {
    let Some(progress) = load_progress(db, user_id).await? else {
        return Ok(None);
    };
    if progress.custom_program_id.is_some() {
        return crate::custom_program::current(db, user_id).await;
    }
    let Some(program_id) = progress.program_id.as_deref() else {
        return Ok(None);
    };
    let Some(program) = load_program(db, program_id).await? else {
        return Ok(None);
    };
    let days = load_days(db, program_id).await?;
    let Some(index) = usize::try_from(progress.day_position.saturating_sub(1)).ok() else {
        return Ok(None);
    };
    let Some(day) = days.get(index) else {
        return Ok(None);
    };
    let Some(routine_id) = routine_id_for_day(db, user_id, &day.id).await? else {
        return Ok(None);
    };
    let Some(day) = resolve_day(db, day).await? else {
        return Ok(None);
    };
    Ok(Some(CurrentProgram {
        program: summary(&program),
        routine_id,
        day,
        day_position: progress.day_position,
        total_days: i64::try_from(days.len()).unwrap_or(i64::MAX),
    }))
}

async fn load_programs(db: &Db) -> Result<Vec<ProgramRow>> {
    db.all(&SqlStatement::new(
        "SELECT id, goal, level, days_per_week, name, sort_order FROM programs \
         ORDER BY sort_order, id",
        Vec::new(),
    ))
    .await?
    .results::<ProgramRow>()
}

async fn load_program(db: &Db, id: &str) -> Result<Option<ProgramRow>> {
    db.first(&SqlStatement::new(
        "SELECT id, goal, level, days_per_week, name, sort_order FROM programs WHERE id = ?",
        vec![SqlValue::Text(id.to_owned())],
    ))
    .await
}

async fn load_days(db: &Db, program_id: &str) -> Result<Vec<ProgramDayRow>> {
    db.all(&SqlStatement::new(
        "SELECT id, program_id, position, name, exercises FROM program_days \
         WHERE program_id = ? ORDER BY position",
        vec![SqlValue::Text(program_id.to_owned())],
    ))
    .await?
    .results::<ProgramDayRow>()
}

async fn load_progress(db: &Db, user_id: &str) -> Result<Option<ProgressRow>> {
    db.first(&SqlStatement::new(
        "SELECT program_id, custom_program_id, day_position \
         FROM user_program_progress WHERE user_id = ?",
        vec![SqlValue::Text(user_id.to_owned())],
    ))
    .await
}

async fn routine_id_for_day(db: &Db, user_id: &str, day_id: &str) -> Result<Option<String>> {
    Ok(db
        .first::<IdRow>(&SqlStatement::new(
            "SELECT id FROM routines WHERE user_id = ? AND source_program_day_id = ? \
             ORDER BY position, id LIMIT 1",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(day_id.to_owned()),
            ],
        ))
        .await?
        .map(|row| row.id))
}

#[derive(Debug, Deserialize)]
struct IdRow {
    id: String,
}

pub(crate) fn record_mode_str(mode: warrior_core::set_calc::RecordMode) -> &'static str {
    match mode {
        warrior_core::set_calc::RecordMode::WeightAndReps => "weight_and_reps",
        warrior_core::set_calc::RecordMode::BodyweightAndReps => "bodyweight_and_reps",
        warrior_core::set_calc::RecordMode::Time => "time",
    }
}

pub(crate) fn laterality_str(value: warrior_core::exercise::Laterality) -> &'static str {
    match value {
        warrior_core::exercise::Laterality::Bilateral => "bilateral",
        warrior_core::exercise::Laterality::Unilateral => "unilateral",
    }
}

pub(crate) fn grip_mode_str(value: warrior_core::exercise::GripMode) -> &'static str {
    match value {
        warrior_core::exercise::GripMode::None => "none",
        warrior_core::exercise::GripMode::Grip => "grip",
        warrior_core::exercise::GripMode::Attachment => "attachment",
    }
}

async fn resolve_day(db: &Db, row: &ProgramDayRow) -> Result<Option<ProgramDay>> {
    let seeds: Vec<ExerciseSeed> = match serde_json::from_str(&row.exercises) {
        Ok(seeds) => seeds,
        Err(error) => {
            console_error!("the program day has invalid exercises: {error}");
            return Ok(None);
        }
    };
    let exercises = crate::exercise::list_master(db).await?;
    let mut resolved = Vec::new();
    for seed in seeds {
        let Some((exercise, _)) = exercises
            .iter()
            .find(|(exercise, _)| exercise.name == seed.name)
        else {
            continue;
        };
        resolved.push(ProgramDayExercise {
            exercise_id: exercise.id.clone(),
            name: exercise.name.clone(),
            record_mode: record_mode_str(exercise.record_mode).to_owned(),
            laterality: laterality_str(exercise.laterality).to_owned(),
            grip_mode: grip_mode_str(exercise.grip_mode).to_owned(),
            grip_options: exercise.grip_options.clone(),
            set_count: seed.sets,
            reps: seed.reps,
            duration_seconds: seed.duration,
            rest_seconds: seed.rest.unwrap_or(90),
        });
    }
    Ok(Some(ProgramDay {
        id: row.id.clone(),
        name: row.name.clone(),
        position: row.position,
        exercises: resolved,
    }))
}

async fn import_statements(
    db: &Db,
    user_id: &str,
    program: &ProgramRow,
    days: &[ProgramDayRow],
) -> Result<Vec<SqlStatement>> {
    let exercises = crate::exercise::list_master(db).await?;
    let now = crate::auth::now_iso();
    let mut statements = vec![SqlStatement::new(
        "DELETE FROM user_program_progress WHERE user_id = ?",
        vec![SqlValue::Text(user_id.to_owned())],
    )];
    for day in days {
        let routine_id = crate::random::random_uuid()?;
        let name = format!("{} {}", program.name, day.name);
        statements.push(SqlStatement::new(
            "INSERT INTO routines \
             (id, user_id, name, position, source_program_day_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            vec![
                SqlValue::Text(routine_id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(name),
                SqlValue::Integer(day.position),
                SqlValue::Text(day.id.clone()),
                SqlValue::Text(now.clone()),
                SqlValue::Text(now.clone()),
            ],
        ));
        let seeds: Vec<ExerciseSeed> = serde_json::from_str(&day.exercises).unwrap_or_default();
        for (position, seed) in seeds.into_iter().enumerate() {
            let Some((exercise, _)) = exercises
                .iter()
                .find(|(exercise, _)| exercise.name == seed.name)
            else {
                continue;
            };
            let routine_exercise_id = crate::random::random_uuid()?;
            let snapshot = serde_json::to_string(&crate::exercise::snapshot(exercise))
                .unwrap_or_else(|_| "{}".to_owned());
            statements.push(SqlStatement::new(
                "INSERT INTO routine_exercises \
                 (id, user_id, routine_id, exercise_id, snapshot, position, created_at, updated_at) \
                 SELECT ?, ?, ?, ?, ?, ?, ?, ? \
                 WHERE EXISTS (SELECT 1 FROM routines WHERE id = ? AND user_id = ?)",
                vec![
                    SqlValue::Text(routine_exercise_id.clone()),
                    SqlValue::Text(user_id.to_owned()),
                    SqlValue::Text(routine_id.clone()),
                    SqlValue::Text(exercise.id.clone()),
                    SqlValue::Text(snapshot),
                    SqlValue::Integer(i64::try_from(position).unwrap_or(i64::MAX).saturating_add(1)),
                    SqlValue::Text(now.clone()),
                    SqlValue::Text(now.clone()),
                    SqlValue::Text(routine_id.clone()),
                    SqlValue::Text(user_id.to_owned()),
                ],
            ));
            statements.push(SqlStatement::new(
                "INSERT INTO routine_exercise_targets \
                 (routine_exercise_id, user_id, set_count, target_weight_kg, target_reps, \
                  target_duration_seconds, grip, rest_seconds, created_at, updated_at) \
                 SELECT ?, ?, ?, NULL, ?, ?, ?, ?, ?, ? \
                 WHERE EXISTS (SELECT 1 FROM routine_exercises WHERE id = ? AND user_id = ?)",
                vec![
                    SqlValue::Text(routine_exercise_id.clone()),
                    SqlValue::Text(user_id.to_owned()),
                    SqlValue::Integer(i64::from(seed.sets.clamp(1, 20))),
                    seed.reps
                        .map_or(SqlValue::Null, |reps| SqlValue::Integer(i64::from(reps))),
                    seed.duration.map_or(SqlValue::Null, |seconds| {
                        SqlValue::Integer(i64::from(seconds))
                    }),
                    grip_value(seed.attachment.as_deref(), exercise),
                    SqlValue::Integer(i64::from(seed.rest.unwrap_or(90).clamp(1, 3_600))),
                    SqlValue::Text(now.clone()),
                    SqlValue::Text(now.clone()),
                    SqlValue::Text(routine_exercise_id),
                    SqlValue::Text(user_id.to_owned()),
                ],
            ));
        }
    }
    statements.push(SqlStatement::new(
        "INSERT INTO user_program_progress \
         (user_id, program_id, custom_program_id, day_position, updated_at) \
         VALUES (?, ?, NULL, 1, ?)",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(program.id.clone()),
            SqlValue::Text(now),
        ],
    ));
    Ok(statements)
}

fn grip_value(attachment: Option<&str>, exercise: &warrior_core::exercise::Exercise) -> SqlValue {
    let Some(attachment) = attachment.and_then(parse_attachment) else {
        return SqlValue::Null;
    };
    if !exercise
        .grip_options
        .iter()
        .any(|option| matches!(option, GripSelection::Attachment { attachment: value, .. } if *value == attachment))
    {
        return SqlValue::Null;
    }
    let selection = GripSelection::Attachment {
        attachment,
        style: None,
    };
    serde_json::to_string(&selection).map_or(SqlValue::Null, SqlValue::Text)
}

fn parse_attachment(text: &str) -> Option<Attachment> {
    Attachment::ALL
        .into_iter()
        .find(|value| value.as_str() == text)
}

/// トレーニングの終了で、プログラムの進行を次の日へ進める。
pub(crate) async fn advance_progress(
    db: &Db,
    user_id: &str,
    program_day_id: &str,
    custom_program_day_id: Option<&str>,
) -> Result<()> {
    if let Some(custom_day_id) = custom_program_day_id {
        return crate::custom_program::advance_progress(db, user_id, custom_day_id).await;
    }
    let Some(day) = db
        .first::<ProgramDayRow>(&SqlStatement::new(
            "SELECT id, program_id, position, name, exercises FROM program_days WHERE id = ?",
            vec![SqlValue::Text(program_day_id.to_owned())],
        ))
        .await?
    else {
        return Ok(());
    };
    let total = db
        .first::<CountRow>(&SqlStatement::new(
            "SELECT COUNT(*) AS count FROM program_days WHERE program_id = ?",
            vec![SqlValue::Text(day.program_id.clone())],
        ))
        .await?
        .map_or(0, |row| row.count);
    if total == 0 {
        return Ok(());
    }
    let next = warrior_core::program::next_position(day.position, total);
    let now = crate::auth::now_iso();
    db.run(&SqlStatement::new(
        "UPDATE user_program_progress SET day_position = ?, updated_at = ? \
         WHERE user_id = ? AND program_id = ?",
        vec![
            SqlValue::Integer(next),
            SqlValue::Text(now),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(day.program_id),
        ],
    ))
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

/// ルーティンの開始元を、プログラムの日へ対応させる。
pub(crate) async fn routine_source(
    db: &Db,
    user_id: &str,
    routine_id: &str,
) -> Result<(SessionSource, Option<String>)> {
    let row = db
        .first::<SourceRow>(&SqlStatement::new(
            "SELECT source_program_day_id, source_custom_program_day_id FROM routines \
             WHERE id = ? AND user_id = ?",
            vec![
                SqlValue::Text(routine_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(
            match crate::custom_program::routine_day(db, user_id, routine_id).await? {
                Some(day_id) => (SessionSource::CustomProgram, Some(day_id)),
                None => (SessionSource::Routine, None),
            },
        );
    };
    if row.source_custom_program_day_id.is_none() && row.source_program_day_id.is_none() {
        if let Some(day_id) = crate::custom_program::routine_day(db, user_id, routine_id).await? {
            return Ok((SessionSource::CustomProgram, Some(day_id)));
        }
    }
    if let Some(day_id) = row.source_custom_program_day_id {
        Ok((SessionSource::CustomProgram, Some(day_id)))
    } else if let Some(day_id) = row.source_program_day_id {
        Ok((SessionSource::Program, Some(day_id)))
    } else {
        Ok((SessionSource::Routine, None))
    }
}

#[derive(Debug, Deserialize)]
struct SourceRow {
    source_program_day_id: Option<String>,
    source_custom_program_day_id: Option<String>,
}
