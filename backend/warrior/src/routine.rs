//! ルーティンの一覧、保存、編集、複製、削除の SQL と、ルーティンからの開始。
//!
//! 保存、編集、複製、削除は端末の送信待ちの列から届き、`backend/warrior/src/sync.rs` が
//! 利用者の所有を検査してからこのモジュールの文を同じ `batch()` で適用する (ADR-002)。
//! 開始は既存のトレーニングの作成の文に `routine_id` を渡し、種目とセットを
//! サーバーで写す。写すときのスナップショットは種目マスターの現在の内容から作る。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::exercise::{ExerciseSnapshot, GripSelection, Laterality};
use warrior_core::routine::{Routine, RoutineExercise, RoutineList, RoutineTarget};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{Operation, SessionCreatePayload, ValidatedOperation, ValidatedPayload};
use warrior_core::training_session::SessionSource;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::ownership;
use crate::sync::{applied_statement, BatchOutcome};

/// `GET /api/routines`
pub async fn list(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let routines = match load_all(&db, &authenticated.id).await {
        Ok(routines) => routines,
        Err(error) => {
            console_error!("failed to load the routines: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    Response::from_json(&RoutineList { routines })
}

/// `POST /api/routines/:id/start`
pub async fn start(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(routine_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(routine) = load_routine(&db, &authenticated.id, &routine_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(terminal_id) = ownership::ensure(&db, &request, &authenticated.id).await? else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the terminal token is required",
        ));
    };
    let now = datetime::format_utc(crate::auth::now_seconds());
    let Some(date) = datetime::japan_date_from_utc(&now) else {
        console_error!("the current time is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let training_id = match crate::random::random_uuid() {
        Ok(id) => id,
        Err(error) => {
            console_error!("failed to create the training id: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let payload = SessionCreatePayload {
        id: training_id.clone(),
        date,
        started_at: now.clone(),
        memo: String::new(),
        source_kind: SessionSource::Routine,
        routine_id: Some(routine.id.clone()),
        program_day_id: None,
        custom_program_day_id: None,
    };
    let statements = match start_statements(
        &db,
        &authenticated.id,
        &terminal_id,
        &payload,
        &routine,
        &now,
    )
    .await
    {
        Ok(statements) => statements,
        Err(error) => {
            console_error!("failed to build the routine start: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    if let Err(error) = db.batch(&statements).await {
        console_error!("failed to start the routine: {error}");
        return Ok(response(ErrorCode::Internal, "internal error"));
    }
    let record =
        match crate::training_record::load_record(&db, &training_id, &authenticated.id).await {
            Ok(Some(record)) => record,
            Ok(None) => {
                console_error!("the started training is missing");
                return Ok(response(ErrorCode::Internal, "internal error"));
            }
            Err(error) => {
                console_error!("failed to load the started training: {error}");
                return Ok(response(ErrorCode::Internal, "internal error"));
            }
        };
    Response::from_json(&record)
}

/// ルーティンの操作の batch。対象のルーティンが別の利用者のものなら 404 を返す。
pub(crate) async fn routine_batch(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    match &validated.payload {
        ValidatedPayload::RoutineCreate(payload) => {
            if !owned_or_missing(db, &payload.routine_id, user_id).await? {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::NotFound,
                    "resource not found",
                )));
            }
            Ok(BatchOutcome::Apply(create_statements(
                user_id,
                &validated.operation,
                payload,
                now,
            )))
        }
        ValidatedPayload::RoutineUpdate(payload) => {
            if !owned(db, &payload.routine_id, user_id).await? {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::NotFound,
                    "resource not found",
                )));
            }
            Ok(BatchOutcome::Apply(update_statements(
                user_id,
                &validated.operation,
                payload,
                now,
            )))
        }
        ValidatedPayload::RoutineDelete(payload) => {
            if !owned(db, &payload.routine_id, user_id).await? {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::NotFound,
                    "resource not found",
                )));
            }
            Ok(BatchOutcome::Apply(delete_statements(
                user_id,
                &validated.operation,
                payload,
                now,
            )))
        }
        ValidatedPayload::RoutineCopy(payload) => {
            if !owned(db, &payload.routine_id, user_id).await? {
                return Ok(BatchOutcome::Reject(response(
                    ErrorCode::NotFound,
                    "resource not found",
                )));
            }
            Ok(BatchOutcome::Apply(copy_statements(
                user_id,
                &validated.operation,
                payload,
                now,
            )))
        }
        _ => Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        ))),
    }
}

/// ルーティンがその利用者のものなら true。行が無いときも true を返す。
///
/// 作成の ID の衝突は、同じ利用者の行なら上書き、別の利用者の行なら 404 にする。
async fn owned_or_missing(db: &Db, routine_id: &str, user_id: &str) -> Result<bool> {
    Ok(match routine_owner(db, routine_id).await? {
        Some(owner) => owner == user_id,
        None => true,
    })
}

async fn owned(db: &Db, routine_id: &str, user_id: &str) -> Result<bool> {
    Ok(routine_owner(db, routine_id).await?.as_deref() == Some(user_id))
}

async fn routine_owner(db: &Db, routine_id: &str) -> Result<Option<String>> {
    let row = db
        .first::<RoutineOwnerRow>(&SqlStatement::new(
            "SELECT user_id FROM routines WHERE id = ?",
            vec![SqlValue::Text(routine_id.to_owned())],
        ))
        .await?;
    Ok(row.map(|row| row.user_id))
}

fn create_statements(
    user_id: &str,
    operation: &Operation,
    payload: &warrior_core::sync::RoutineCreatePayload,
    now: &str,
) -> Vec<SqlStatement> {
    let mut statements = vec![
        routine_insert(
            user_id,
            &payload.routine_id,
            &payload.name,
            payload.position,
            now,
        ),
        applied_statement(user_id, operation, now),
    ];
    statements.extend(replace_exercises(
        user_id,
        &payload.routine_id,
        &payload.exercises,
        now,
    ));
    statements
}

fn update_statements(
    user_id: &str,
    operation: &Operation,
    payload: &warrior_core::sync::RoutineUpdatePayload,
    now: &str,
) -> Vec<SqlStatement> {
    let mut statements = vec![
        SqlStatement::new(
            "UPDATE routines SET name = ?, position = ?, updated_at = ? \
             WHERE id = ? AND user_id = ?",
            vec![
                SqlValue::Text(payload.name.clone()),
                SqlValue::Integer(payload.position),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(payload.routine_id.clone()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ),
        applied_statement(user_id, operation, now),
    ];
    statements.extend(replace_exercises(
        user_id,
        &payload.routine_id,
        &payload.exercises,
        now,
    ));
    statements
}

fn delete_statements(
    user_id: &str,
    operation: &Operation,
    payload: &warrior_core::sync::RoutineDeletePayload,
    now: &str,
) -> Vec<SqlStatement> {
    let routine_id = SqlValue::Text(payload.routine_id.clone());
    let user = SqlValue::Text(user_id.to_owned());
    vec![
        SqlStatement::new(
            "DELETE FROM routines WHERE id = ? AND user_id = ?",
            vec![routine_id.clone(), user.clone()],
        ),
        applied_statement(user_id, operation, now),
        delete_targets_statement(user_id, &payload.routine_id),
        SqlStatement::new(
            "DELETE FROM routine_exercises WHERE user_id = ? AND routine_id = ?",
            vec![user.clone(), routine_id.clone()],
        ),
        SqlStatement::new(
            "UPDATE training_sessions SET routine_id = NULL WHERE user_id = ? AND routine_id = ?",
            vec![user, routine_id],
        ),
    ]
}

fn copy_statements(
    user_id: &str,
    operation: &Operation,
    payload: &warrior_core::sync::RoutineCopyPayload,
    now: &str,
) -> Vec<SqlStatement> {
    let mut statements = vec![
        routine_insert(
            user_id,
            &payload.new_routine_id,
            &payload.name,
            payload.position,
            now,
        ),
        applied_statement(user_id, operation, now),
    ];
    statements.extend(replace_exercises(
        user_id,
        &payload.new_routine_id,
        &payload.exercises,
        now,
    ));
    statements
}

fn routine_insert(
    user_id: &str,
    routine_id: &str,
    name: &str,
    position: i64,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO routines (id, user_id, name, position, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, position = excluded.position, \
         updated_at = excluded.updated_at \
         WHERE routines.user_id = excluded.user_id",
        vec![
            SqlValue::Text(routine_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(name.to_owned()),
            SqlValue::Integer(position),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn replace_exercises(
    user_id: &str,
    routine_id: &str,
    exercises: &[warrior_core::sync::RoutineExercisePayload],
    now: &str,
) -> Vec<SqlStatement> {
    let mut statements = vec![
        delete_targets_statement(user_id, routine_id),
        SqlStatement::new(
            "DELETE FROM routine_exercises WHERE user_id = ? AND routine_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(routine_id.to_owned()),
            ],
        ),
    ];
    for exercise in exercises {
        statements.push(exercise_insert_statement(
            user_id, routine_id, exercise, now,
        ));
        statements.push(target_insert_statement(user_id, exercise, now));
    }
    statements
}

fn delete_targets_statement(user_id: &str, routine_id: &str) -> SqlStatement {
    SqlStatement::new(
        "DELETE FROM routine_exercise_targets WHERE user_id = ? AND routine_exercise_id IN \
         (SELECT id FROM routine_exercises WHERE routine_id = ? AND user_id = ?)",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(routine_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

fn exercise_insert_statement(
    user_id: &str,
    routine_id: &str,
    exercise: &warrior_core::sync::RoutineExercisePayload,
    now: &str,
) -> SqlStatement {
    let snapshot = snapshot_json(&exercise.snapshot);
    SqlStatement::new(
        "INSERT INTO routine_exercises \
         (id, user_id, routine_id, exercise_id, snapshot, position, created_at, updated_at) \
         SELECT ?, ?, ?, ?, ?, ?, ?, ? \
         WHERE EXISTS (SELECT 1 FROM routines WHERE id = ? AND user_id = ?)",
        vec![
            SqlValue::Text(exercise.routine_exercise_id.clone()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(routine_id.to_owned()),
            SqlValue::Text(exercise.exercise_id.clone()),
            SqlValue::Text(snapshot),
            SqlValue::Integer(exercise.position),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(routine_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

fn target_insert_statement(
    user_id: &str,
    exercise: &warrior_core::sync::RoutineExercisePayload,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO routine_exercise_targets \
         (routine_exercise_id, user_id, set_count, target_weight_kg, target_reps, \
          target_duration_seconds, grip, rest_seconds, created_at, updated_at) \
         SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
         WHERE EXISTS (SELECT 1 FROM routine_exercises \
          WHERE id = ? AND user_id = ?)",
        vec![
            SqlValue::Text(exercise.routine_exercise_id.clone()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Integer(i64::from(exercise.target.set_count)),
            optional_real(exercise.target.weight_kg),
            optional_integer(exercise.target.reps.map(i64::from)),
            optional_integer(exercise.target.duration_seconds.map(i64::from)),
            optional_text(exercise.target.grip.map(grip_json)),
            optional_integer(exercise.target.rest_seconds.map(i64::from)),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(exercise.routine_exercise_id.clone()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

fn snapshot_json(snapshot: &ExerciseSnapshot) -> String {
    serde_json::to_string(snapshot).unwrap_or_else(|_| "{}".to_owned())
}

fn grip_json(grip: GripSelection) -> String {
    serde_json::to_string(&grip).unwrap_or_else(|_| "null".to_owned())
}

fn optional_real(value: Option<f64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Real)
}

fn optional_integer(value: Option<i64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Integer)
}

fn optional_text(value: Option<String>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Text)
}

async fn start_statements(
    db: &Db,
    user_id: &str,
    terminal_id: &str,
    payload: &SessionCreatePayload,
    routine: &Routine,
    now: &str,
) -> Result<Vec<SqlStatement>> {
    let mut statements = vec![crate::sync::session_create_statement(
        user_id,
        terminal_id,
        payload,
        now,
    )];
    for exercise in &routine.exercises {
        let snapshot = current_snapshot(db, user_id, exercise).await?;
        let training_exercise_id = crate::random::random_uuid()?;
        statements.push(training_exercise_statement(
            user_id,
            &payload.id,
            &training_exercise_id,
            exercise,
            &snapshot,
            now,
        ));
        statements.extend(set_statements(
            user_id,
            &training_exercise_id,
            &snapshot,
            &exercise.target,
            now,
        )?);
    }
    Ok(statements)
}

async fn current_snapshot(
    db: &Db,
    user_id: &str,
    exercise: &RoutineExercise,
) -> Result<ExerciseSnapshot> {
    if let Some(master) = crate::exercise::load_master(db, &exercise.exercise_id).await? {
        return Ok(ExerciseSnapshot::from(&master));
    }
    if let Some(user_exercise) =
        crate::exercise::load_user(db, user_id, &exercise.exercise_id).await?
    {
        return Ok(ExerciseSnapshot::from(&user_exercise));
    }
    Ok(exercise.snapshot.clone())
}

fn training_exercise_statement(
    user_id: &str,
    training_id: &str,
    training_exercise_id: &str,
    exercise: &RoutineExercise,
    snapshot: &ExerciseSnapshot,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO training_exercises \
         (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        vec![
            SqlValue::Text(training_exercise_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(exercise.exercise_id.clone()),
            SqlValue::Text(snapshot_json(snapshot)),
            SqlValue::Integer(exercise.position),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn set_statements(
    user_id: &str,
    training_exercise_id: &str,
    snapshot: &ExerciseSnapshot,
    target: &RoutineTarget,
    now: &str,
) -> Result<Vec<SqlStatement>> {
    let mut statements = Vec::new();
    for position in 1..=i64::from(target.set_count) {
        let set_id = crate::random::random_uuid()?;
        statements.push(SqlStatement::new(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, grip, \
              created_at, updated_at) \
             VALUES (?, ?, ?, ?, 'normal', 0, ?, ?, ?)",
            vec![
                SqlValue::Text(set_id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_exercise_id.to_owned()),
                SqlValue::Integer(position),
                optional_text(target.grip.map(grip_json)),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(now.to_owned()),
            ],
        ));
        let segment_id = crate::random::random_uuid()?;
        statements.push(initial_segment_statement(
            user_id,
            &set_id,
            &segment_id,
            snapshot,
            target,
            now,
        ));
    }
    Ok(statements)
}

fn initial_segment_statement(
    user_id: &str,
    set_id: &str,
    segment_id: &str,
    snapshot: &ExerciseSnapshot,
    target: &RoutineTarget,
    now: &str,
) -> SqlStatement {
    let lateral = snapshot.laterality == Laterality::Unilateral;
    let weight = target.weight_kg.unwrap_or(0.0);
    let reps = target.reps.map_or(0, i64::from);
    let duration = target.duration_seconds.map(i64::from);
    let (
        weight,
        reps,
        duration,
        left_weight,
        left_reps,
        left_duration,
        right_weight,
        right_reps,
        right_duration,
    ) = if lateral {
        (
            Some(0.0),
            Some(0),
            None,
            Some(weight),
            Some(reps),
            duration,
            Some(weight),
            Some(reps),
            duration,
        )
    } else {
        (
            Some(weight),
            Some(reps),
            duration,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    };
    SqlStatement::new(
        "INSERT INTO set_segments \
         (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
          duration_seconds, left_weight_kg, left_reps, left_duration_seconds, \
          right_weight_kg, right_reps, right_duration_seconds, created_at, updated_at) \
         VALUES (?, ?, ?, 1, ?, ?, 0, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        vec![
            SqlValue::Text(segment_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(set_id.to_owned()),
            optional_real(weight),
            optional_integer(reps),
            optional_integer(duration),
            optional_real(left_weight),
            optional_integer(left_reps),
            optional_integer(left_duration),
            optional_real(right_weight),
            optional_integer(right_reps),
            optional_integer(right_duration),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

#[derive(Debug, Deserialize)]
struct RoutineOwnerRow {
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct RoutineRow {
    id: String,
    name: String,
    position: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct RoutineExerciseRow {
    id: String,
    routine_id: String,
    exercise_id: String,
    snapshot: String,
    position: i64,
    set_count: i64,
    target_weight_kg: Option<f64>,
    target_reps: Option<i64>,
    target_duration_seconds: Option<i64>,
    grip: Option<String>,
    rest_seconds: Option<i64>,
    created_at: String,
    updated_at: String,
}

async fn load_all(db: &Db, user_id: &str) -> Result<Vec<Routine>> {
    let routines = db
        .all(&SqlStatement::new(
            "SELECT id, name, position, created_at, updated_at FROM routines \
             WHERE user_id = ? ORDER BY position, id",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<RoutineRow>()?;
    let exercises = load_exercises(db, user_id, None).await?;
    routines
        .into_iter()
        .map(|row| routine_from_row(row, &exercises))
        .collect()
}

async fn load_routine(db: &Db, user_id: &str, routine_id: &str) -> Result<Option<Routine>> {
    let Some(row) = db
        .first::<RoutineRow>(&SqlStatement::new(
            "SELECT id, name, position, created_at, updated_at FROM routines \
             WHERE id = ? AND user_id = ?",
            vec![
                SqlValue::Text(routine_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?
    else {
        return Ok(None);
    };
    let exercises = load_exercises(db, user_id, Some(routine_id)).await?;
    routine_from_row(row, &exercises).map(Some)
}

async fn load_exercises(
    db: &Db,
    user_id: &str,
    routine_id: Option<&str>,
) -> Result<Vec<RoutineExerciseRow>> {
    let (sql, bindings) = match routine_id {
        Some(routine_id) => (
            "SELECT re.id, re.routine_id, re.exercise_id, re.snapshot, re.position, \
             t.set_count, t.target_weight_kg, t.target_reps, t.target_duration_seconds, \
             t.grip, t.rest_seconds, re.created_at, re.updated_at \
             FROM routine_exercises re \
             JOIN routine_exercise_targets t \
             ON t.routine_exercise_id = re.id AND t.user_id = re.user_id \
             WHERE re.user_id = ? AND re.routine_id = ? \
             ORDER BY re.position, re.id",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(routine_id.to_owned()),
            ],
        ),
        None => (
            "SELECT re.id, re.routine_id, re.exercise_id, re.snapshot, re.position, \
             t.set_count, t.target_weight_kg, t.target_reps, t.target_duration_seconds, \
             t.grip, t.rest_seconds, re.created_at, re.updated_at \
             FROM routine_exercises re \
             JOIN routine_exercise_targets t \
             ON t.routine_exercise_id = re.id AND t.user_id = re.user_id \
             WHERE re.user_id = ? \
             ORDER BY re.routine_id, re.position, re.id",
            vec![SqlValue::Text(user_id.to_owned())],
        ),
    };
    db.all(&SqlStatement::new(sql, bindings))
        .await?
        .results::<RoutineExerciseRow>()
}

fn routine_from_row(row: RoutineRow, exercises: &[RoutineExerciseRow]) -> Result<Routine> {
    let exercises = exercises
        .iter()
        .filter(|exercise| exercise.routine_id == row.id)
        .map(stored_exercise)
        .collect::<Result<Vec<_>>>()?;
    Ok(Routine {
        id: row.id,
        name: row.name,
        position: row.position,
        exercises,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn stored_exercise(row: &RoutineExerciseRow) -> Result<RoutineExercise> {
    let snapshot = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot)
        .map_err(|_| invalid_stored_data("the routine snapshot is invalid"))?;
    let grip = row
        .grip
        .as_deref()
        .map(serde_json::from_str::<GripSelection>)
        .transpose()
        .map_err(|_| invalid_stored_data("the routine grip is invalid"))?;
    let set_count = u32::try_from(row.set_count)
        .ok()
        .filter(|count| *count >= 1)
        .ok_or_else(|| invalid_stored_data("the routine set count is invalid"))?;
    Ok(RoutineExercise {
        id: row.id.clone(),
        routine_id: row.routine_id.clone(),
        exercise_id: row.exercise_id.clone(),
        snapshot,
        position: row.position,
        target: RoutineTarget {
            set_count,
            weight_kg: row.target_weight_kg,
            reps: to_u32(row.target_reps),
            duration_seconds: to_u32(row.target_duration_seconds),
            grip,
            rest_seconds: to_u32(row.rest_seconds),
        },
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    })
}

fn to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

fn invalid_stored_data(message: &str) -> worker::Error {
    console_error!("{message}");
    worker::Error::RustError(message.to_owned())
}

#[cfg(test)]
mod tests;
