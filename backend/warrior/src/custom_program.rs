//! 独自のプログラムの作成、編集、削除、進行 (REQ-PRG-021、REQ-PRG-022、REQ-PRG-023、REQ-PRG-024)。
//!
//! 独自のプログラムは利用者のルーティンを順番に並べたものである。日は 2 日以上 7 日以下とし、
//! 同じルーティンを複数の日に指定できる。用意したプログラムの推奨と参考には含めない。

use serde::{Deserialize, Serialize};
use warrior_core::exercise::ExerciseSnapshot;
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::program::{
    grip_mode_str, laterality_str, record_mode_str, CurrentProgram, ProgramDay, ProgramDayExercise,
    ProgramSummary,
};

#[derive(Debug, Deserialize)]
struct CustomProgramRow {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CustomDayRow {
    id: String,
    position: i64,
    routine_id: String,
}

#[derive(Debug, Serialize)]
pub struct CustomProgramView {
    pub id: String,
    pub name: String,
    pub routine_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CustomInput {
    name: String,
    routine_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CustomProgramsResponse {
    programs: Vec<CustomProgramView>,
}

/// `GET /api/custom-programs`
pub async fn list(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let programs = match load_programs(&db, &authenticated.id).await {
        Ok(programs) => programs,
        Err(error) => {
            console_error!("failed to load the custom programs: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    Response::from_json(&CustomProgramsResponse { programs })
}

/// `POST /api/custom-programs`
pub async fn create(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: CustomInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the program is invalid")),
    };
    if !valid_input(&input) {
        return Ok(response(ErrorCode::BadRequest, "the program is invalid"));
    }
    let db = Db::new(env.d1("DB")?);
    if !owned_routines(&db, &authenticated.id, &input.routine_ids).await? {
        return Ok(response(ErrorCode::BadRequest, "a routine is invalid"));
    }
    let program_id = crate::random::random_uuid()?;
    let now = crate::auth::now_iso();
    let statements = create_statements(&authenticated.id, &program_id, &input, &now)?;
    if let Err(error) = db.batch(&statements).await {
        console_error!("failed to create the custom program: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    let view = CustomProgramView {
        id: program_id,
        name: input.name,
        routine_ids: input.routine_ids,
    };
    Response::from_json(&view).map(|response| response.with_status(201))
}

/// `PUT /api/custom-programs/:id`
pub async fn update(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(program_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: CustomInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the program is invalid")),
    };
    if !valid_input(&input) {
        return Ok(response(ErrorCode::BadRequest, "the program is invalid"));
    }
    let db = Db::new(env.d1("DB")?);
    if load_program(&db, &authenticated.id, &program_id)
        .await?
        .is_none()
    {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    if !owned_routines(&db, &authenticated.id, &input.routine_ids).await? {
        return Ok(response(ErrorCode::BadRequest, "a routine is invalid"));
    }
    let now = crate::auth::now_iso();
    let mut statements = vec![SqlStatement::new(
        "UPDATE custom_programs SET name = ?, updated_at = ? WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(input.name.clone()),
            SqlValue::Text(now.clone()),
            SqlValue::Text(program_id.clone()),
            SqlValue::Text(authenticated.id.clone()),
        ],
    )];
    statements.push(SqlStatement::new(
        "DELETE FROM custom_program_days WHERE custom_program_id = ?",
        vec![SqlValue::Text(program_id.clone())],
    ));
    for (index, routine_id) in input.routine_ids.iter().enumerate() {
        statements.push(custom_day_insert(
            &authenticated.id,
            &program_id,
            routine_id,
            i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
            &now,
        )?);
    }
    statements.push(SqlStatement::new(
        "UPDATE user_program_progress SET day_position = 1, updated_at = ? \
         WHERE user_id = ? AND custom_program_id = ?",
        vec![
            SqlValue::Text(now),
            SqlValue::Text(authenticated.id),
            SqlValue::Text(program_id.clone()),
        ],
    ));
    if let Err(error) = db.batch(&statements).await {
        console_error!("failed to update the custom program: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    Response::from_json(&CustomProgramView {
        id: program_id,
        name: input.name,
        routine_ids: input.routine_ids,
    })
}

/// `DELETE /api/custom-programs/:id`
pub async fn delete(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(program_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let result = db
        .run(&SqlStatement::new(
            "DELETE FROM custom_programs WHERE id = ? AND user_id = ?",
            vec![SqlValue::Text(program_id), SqlValue::Text(authenticated.id)],
        ))
        .await?;
    if crate::db::changed_rows(&result)? == 0 {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    Ok(Response::empty()?.with_status(204))
}

/// `POST /api/custom-programs/:id/select`
pub async fn select(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(program_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    if load_program(&db, &authenticated.id, &program_id)
        .await?
        .is_none()
    {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    let now = crate::auth::now_iso();
    db.run(&SqlStatement::new(
        "INSERT INTO user_program_progress \
         (user_id, program_id, custom_program_id, day_position, updated_at) \
         VALUES (?, NULL, ?, 1, ?) \
         ON CONFLICT(user_id) DO UPDATE SET program_id = NULL, \
          custom_program_id = excluded.custom_program_id, day_position = 1, \
          updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(authenticated.id.clone()),
            SqlValue::Text(program_id.clone()),
            SqlValue::Text(now),
        ],
    ))
    .await?;
    match current(&db, &authenticated.id).await? {
        Some(current) => Response::from_json(&current),
        None => Ok(response(ErrorCode::Internal, "internal error")),
    }
}

fn valid_input(input: &CustomInput) -> bool {
    warrior_core::custom_program::valid(&input.name, input.routine_ids.len())
}

async fn owned_routines(db: &Db, user_id: &str, routine_ids: &[String]) -> Result<bool> {
    for routine_id in routine_ids {
        let row = db
            .first::<IdRow>(&SqlStatement::new(
                "SELECT id FROM routines WHERE id = ? AND user_id = ?",
                vec![
                    SqlValue::Text(routine_id.clone()),
                    SqlValue::Text(user_id.to_owned()),
                ],
            ))
            .await?;
        if row.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Debug, Deserialize)]
struct IdRow {
    #[allow(dead_code)]
    id: String,
}

fn create_statements(
    user_id: &str,
    program_id: &str,
    input: &CustomInput,
    now: &str,
) -> Result<Vec<SqlStatement>> {
    let mut statements = vec![SqlStatement::new(
        "INSERT INTO custom_programs (id, user_id, name, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?)",
        vec![
            SqlValue::Text(program_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(input.name.trim().to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )];
    for (index, routine_id) in input.routine_ids.iter().enumerate() {
        statements.push(custom_day_insert(
            user_id,
            program_id,
            routine_id,
            i64::try_from(index).unwrap_or(i64::MAX).saturating_add(1),
            now,
        )?);
    }
    Ok(statements)
}

fn custom_day_insert(
    user_id: &str,
    program_id: &str,
    routine_id: &str,
    position: i64,
    _now: &str,
) -> Result<SqlStatement> {
    Ok(SqlStatement::new(
        "INSERT INTO custom_program_days (id, custom_program_id, position, routine_id) \
         SELECT ?, ?, ?, ? \
         WHERE EXISTS (SELECT 1 FROM custom_programs WHERE id = ? AND user_id = ?) \
         AND EXISTS (SELECT 1 FROM routines WHERE id = ? AND user_id = ?)",
        vec![
            SqlValue::Text(crate::random::random_uuid()?),
            SqlValue::Text(program_id.to_owned()),
            SqlValue::Integer(position),
            SqlValue::Text(routine_id.to_owned()),
            SqlValue::Text(program_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(routine_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
}

async fn load_programs(db: &Db, user_id: &str) -> Result<Vec<CustomProgramView>> {
    let programs = db
        .all(&SqlStatement::new(
            "SELECT id, name FROM custom_programs WHERE user_id = ? ORDER BY created_at, id",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<CustomProgramRow>()?;
    let mut views = Vec::new();
    for program in programs {
        let days = load_days(db, user_id, &program.id).await?;
        views.push(CustomProgramView {
            id: program.id,
            name: program.name,
            routine_ids: days.into_iter().map(|day| day.routine_id).collect(),
        });
    }
    Ok(views)
}

async fn load_program(
    db: &Db,
    user_id: &str,
    program_id: &str,
) -> Result<Option<CustomProgramRow>> {
    db.first(&SqlStatement::new(
        "SELECT id, name FROM custom_programs WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(program_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

async fn load_days(db: &Db, user_id: &str, program_id: &str) -> Result<Vec<CustomDayRow>> {
    db.all(&SqlStatement::new(
        "SELECT d.id, d.position, d.routine_id FROM custom_program_days d \
         JOIN custom_programs p ON p.id = d.custom_program_id AND p.user_id = ? \
         WHERE d.custom_program_id = ? ORDER BY d.position",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(program_id.to_owned()),
        ],
    ))
    .await?
    .results::<CustomDayRow>()
}

/// 進行中の独自のプログラムを返す。
pub(crate) async fn current(db: &Db, user_id: &str) -> Result<Option<CurrentProgram>> {
    let Some(progress) = db
        .first::<ProgressRow>(&SqlStatement::new(
            "SELECT custom_program_id, day_position FROM user_program_progress WHERE user_id = ?",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
    else {
        return Ok(None);
    };
    let Some(program_id) = progress.custom_program_id else {
        return Ok(None);
    };
    let Some(program) = load_program(db, user_id, &program_id).await? else {
        return Ok(None);
    };
    let days = load_days(db, user_id, &program_id).await?;
    let Some(index) = usize::try_from(progress.day_position.saturating_sub(1)).ok() else {
        return Ok(None);
    };
    let Some(day) = days.get(index) else {
        return Ok(None);
    };
    let exercises = day_exercises(db, user_id, &day.routine_id).await?;
    Ok(Some(CurrentProgram {
        program: ProgramSummary {
            id: program.id.clone(),
            name: program.name.clone(),
            goal: "custom".to_owned(),
            level: "custom".to_owned(),
            days_per_week: u32::try_from(days.len()).unwrap_or(u32::MAX),
        },
        routine_id: day.routine_id.clone(),
        day: ProgramDay {
            id: day.id.clone(),
            name: format!("{} 日目", day.position),
            position: day.position,
            exercises,
        },
        day_position: progress.day_position,
        total_days: i64::try_from(days.len()).unwrap_or(i64::MAX),
    }))
}

#[derive(Debug, Deserialize)]
struct ProgressRow {
    custom_program_id: Option<String>,
    day_position: i64,
}

#[derive(Debug, Deserialize)]
struct RoutineExerciseRow {
    snapshot: String,
    set_count: i64,
    target_reps: Option<i64>,
    target_duration_seconds: Option<i64>,
    rest_seconds: Option<i64>,
}

async fn day_exercises(
    db: &Db,
    user_id: &str,
    routine_id: &str,
) -> Result<Vec<ProgramDayExercise>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT re.snapshot, t.set_count, t.target_reps, t.target_duration_seconds, \
             t.rest_seconds FROM routine_exercises re \
             JOIN routine_exercise_targets t ON t.routine_exercise_id = re.id \
             WHERE re.user_id = ? AND re.routine_id = ? ORDER BY re.position, re.id",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(routine_id.to_owned()),
            ],
        ))
        .await?
        .results::<RoutineExerciseRow>()?;
    let mut exercises = Vec::new();
    for row in rows {
        let Ok(snapshot) = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot) else {
            continue;
        };
        exercises.push(ProgramDayExercise {
            exercise_id: snapshot.exercise_id.clone(),
            name: snapshot.name,
            record_mode: record_mode_str(snapshot.record_mode).to_owned(),
            laterality: laterality_str(snapshot.laterality).to_owned(),
            grip_mode: grip_mode_str(snapshot.grip_mode).to_owned(),
            grip_options: snapshot.grip_options,
            set_count: u32::try_from(row.set_count).unwrap_or(0),
            reps: row.target_reps.and_then(|value| u32::try_from(value).ok()),
            duration_seconds: row
                .target_duration_seconds
                .and_then(|value| u32::try_from(value).ok()),
            rest_seconds: row
                .rest_seconds
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(90),
        });
    }
    Ok(exercises)
}

/// トレーニングの終了で、独自のプログラムの進行を次の日へ進める。
pub(crate) async fn advance_progress(
    db: &Db,
    user_id: &str,
    _custom_program_day_id: &str,
) -> Result<()> {
    let Some(progress) = db
        .first::<ProgressRow>(&SqlStatement::new(
            "SELECT custom_program_id, day_position FROM user_program_progress WHERE user_id = ?",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
    else {
        return Ok(());
    };
    let Some(program_id) = progress.custom_program_id else {
        return Ok(());
    };
    let count = db
        .first::<CountRow>(&SqlStatement::new(
            "SELECT COUNT(*) AS count FROM custom_program_days WHERE custom_program_id = ?",
            vec![SqlValue::Text(program_id.clone())],
        ))
        .await?
        .map_or(0, |row| row.count);
    if count == 0 {
        return Ok(());
    }
    let next = warrior_core::program::next_position(progress.day_position, count);
    db.run(&SqlStatement::new(
        "UPDATE user_program_progress SET day_position = ?, updated_at = ? \
         WHERE user_id = ? AND custom_program_id = ?",
        vec![
            SqlValue::Integer(next),
            SqlValue::Text(crate::auth::now_iso()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(program_id.clone()),
        ],
    ))
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

/// ルーティンが現在の独自のプログラムの日か。
pub(crate) async fn routine_day(
    db: &Db,
    user_id: &str,
    routine_id: &str,
) -> Result<Option<String>> {
    Ok(db
        .first::<CustomDayRow>(&SqlStatement::new(
            "SELECT d.id, d.position, d.routine_id FROM custom_program_days d \
             JOIN user_program_progress p ON p.custom_program_id = d.custom_program_id \
             WHERE p.user_id = ? AND d.routine_id = ? ORDER BY d.position LIMIT 1",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(routine_id.to_owned()),
            ],
        ))
        .await?
        .map(|row| row.id))
}
