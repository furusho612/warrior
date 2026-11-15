//! 代わりの種目の候補の取得と、過去に記録した種目の ID の取得。
//!
//! 候補の選択と並び順は `warrior_core::alternative` が持つ。このモジュールは種目
//! マスターと、終了したトレーニングで記録した種目の ID を D1 から読み、応答を組み立てる。
//! AI の Queue にはメッセージを送らない。

use std::collections::HashSet;

use serde::Deserialize;
use warrior_core::alternative::{
    select_alternatives, AlternativeList, AlternativeOrigin, RecordedExercises,
};
use warrior_core::exercise::Exercise;
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;

#[derive(Debug, Deserialize)]
struct RecordedExerciseRow {
    exercise_id: String,
}

/// `GET /api/exercises/:id/alternatives`
///
/// 同じ部位区分で主働筋が重なる種目を候補として返す。候補は種目マスターだけから選ぶ。
pub async fn alternatives(request: Request, context: RouteContext<()>) -> Result<Response> {
    let exercise_id = context.param("id").cloned();
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let Some(exercise_id) = exercise_id else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let db = Db::new(env.d1("DB")?);
    let origin = match load_origin(&db, &authenticated.id, &exercise_id).await {
        Ok(origin) => origin,
        Err(error) => {
            console_error!("failed to load the exercise: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let Some(origin) = origin else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let library = match exercise::list_master(&db).await {
        Ok(library) => library,
        Err(error) => {
            console_error!("failed to list the exercises: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let recorded = match recorded_exercise_ids(&db, &authenticated.id).await {
        Ok(recorded) => recorded,
        Err(error) => {
            console_error!("failed to list the recorded exercises: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let exercises = select_alternatives(&AlternativeOrigin::from(&origin), &library, &recorded);
    Response::from_json(&AlternativeList { exercises })
}

/// `GET /api/exercises/recorded`
///
/// 終了したトレーニングで記録した種目の ID を返す。候補の並びに使う。
pub async fn recorded(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let recorded = match recorded_exercise_ids(&db, &authenticated.id).await {
        Ok(recorded) => recorded,
        Err(error) => {
            console_error!("failed to list the recorded exercises: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let mut exercise_ids = recorded.into_iter().collect::<Vec<_>>();
    exercise_ids.sort();
    Response::from_json(&RecordedExercises { exercise_ids })
}

async fn load_origin(db: &Db, user_id: &str, exercise_id: &str) -> Result<Option<Exercise>> {
    if let Some(exercise) = exercise::load_master(db, exercise_id).await? {
        return Ok(Some(exercise));
    }
    exercise::load_user(db, user_id, exercise_id).await
}

async fn recorded_exercise_ids(db: &Db, user_id: &str) -> Result<HashSet<String>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT DISTINCT te.exercise_id FROM training_exercises te \
             JOIN training_sessions ts ON ts.id = te.training_id AND ts.user_id = te.user_id \
             WHERE te.user_id = ? AND ts.finished_at IS NOT NULL",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<RecordedExerciseRow>()?;
    Ok(rows
        .into_iter()
        .map(|row| row.exercise_id)
        .collect::<HashSet<_>>())
}
