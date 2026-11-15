//! 種目ライブラリの一覧、詳細、お気に入りの D1 の読み書き。
//!
//! 並びと絞り込みとページングの規則は `warrior_core::library` が持つ。このモジュールは
//! 種目マスターとお気に入りの行を読み、条件を適用して応答を組み立てる。
//! 記録への種目の選択は全件を使うため、`paged=true` を指定しない一覧は全件を返す。

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use serde::Deserialize;
use warrior_core::cursor::{decode_cursor, Cursor};
use warrior_core::exercise::{BodyPart, Equipment, Exercise};
use warrior_core::library::{
    filter_exercises, paginate, sort_exercises, ExerciseFilter, ExerciseList, FavoriteState,
    LibraryExercise,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;

/// マスターの種目を表す種別。
const MASTER_KIND: &str = "master";
/// 利用者の独自種目を表す種別。
const USER_KIND: &str = "user";

#[derive(Debug, Deserialize)]
struct FavoriteRow {
    exercise_kind: String,
    exercise_id: String,
}

/// `GET /api/exercises`
///
/// 絞り込みの条件または `paged=true` があるときは 1 ページを返し、それ以外は全件を返す。
pub async fn exercises(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let params = request
        .url()?
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    let Some(query) = parse_query(&params) else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the exercise filters are invalid",
        ));
    };
    let page_size = if query.paged {
        match page_size(&env) {
            Ok(size) => Some(size),
            Err(error) => {
                console_error!("failed to read the library page size: {error}");
                return Ok(response(ErrorCode::Internal, "internal error"));
            }
        }
    } else {
        None
    };
    let db = Db::new(env.d1("DB")?);
    let favorites = favorite_keys(&db, &authenticated.id).await?;
    let exercises = match exercise::list_master(&db).await {
        Ok(exercises) => exercises,
        Err(error) => {
            console_error!("failed to list the exercises: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let items = exercises
        .into_iter()
        .map(|exercise| LibraryExercise {
            favorite: favorites.contains(&(MASTER_KIND.to_owned(), exercise.id.clone())),
            exercise,
        })
        .collect::<Vec<_>>();
    let mut items = filter_exercises(items, &query.filter);
    sort_exercises(&mut items);
    match page_size {
        Some(size) => Response::from_json(&paginate(&items, query.cursor.as_ref(), size)),
        None => Response::from_json(&ExerciseList {
            exercises: items,
            next_cursor: None,
        }),
    }
}

/// `GET /api/exercises/:id`
pub async fn exercise_detail(request: Request, context: RouteContext<()>) -> Result<Response> {
    let exercise_id = context.param("id").cloned();
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let Some(exercise_id) = exercise_id else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some((kind, exercise)) = resolve(&db, &authenticated.id, &exercise_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let favorite = is_favorite(&db, &authenticated.id, &kind, &exercise.id).await?;
    Response::from_json(&LibraryExercise { exercise, favorite })
}

/// `PUT /api/exercises/:id/favorite`
pub async fn add_favorite(request: Request, context: RouteContext<()>) -> Result<Response> {
    set_favorite(request, context, true).await
}

/// `DELETE /api/exercises/:id/favorite`
pub async fn remove_favorite(request: Request, context: RouteContext<()>) -> Result<Response> {
    set_favorite(request, context, false).await
}

/// 一覧のクエリ。
struct ListQuery {
    filter: ExerciseFilter,
    paged: bool,
    cursor: Option<Cursor>,
}

fn parse_query(params: &HashMap<String, String>) -> Option<ListQuery> {
    let body_part = match params.get("body_part") {
        None => None,
        Some(text) => Some(BodyPart::parse(text)?),
    };
    let equipment = match params.get("equipment") {
        None => None,
        Some(text) => Some(Equipment::parse(text)?),
    };
    let favorite_only = match params.get("favorite").map(String::as_str) {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return None,
    };
    let paged = match params.get("paged").map(String::as_str) {
        None => ["body_part", "equipment", "favorite", "q", "cursor"]
            .iter()
            .any(|key| params.contains_key(*key)),
        Some("false") => false,
        Some("true") => true,
        Some(_) => return None,
    };
    let cursor = match params.get("cursor") {
        None => None,
        Some(encoded) => Some(decode_cursor(encoded).ok()?),
    };
    Some(ListQuery {
        filter: ExerciseFilter {
            body_part,
            equipment,
            favorite_only,
            query: params.get("q").cloned().unwrap_or_default(),
        },
        paged,
        cursor,
    })
}

fn page_size(env: &worker::Env) -> Result<NonZeroUsize> {
    let value = auth::var(env, "LIBRARY_PAGE_SIZE")?;
    let size = value.parse::<usize>().map_err(|_| {
        worker::Error::RustError(format!("LIBRARY_PAGE_SIZE is not a number: {value}"))
    })?;
    NonZeroUsize::new(size).ok_or_else(|| {
        worker::Error::RustError("LIBRARY_PAGE_SIZE must be greater than zero".to_owned())
    })
}

async fn favorite_keys(db: &Db, user_id: &str) -> Result<HashSet<(String, String)>> {
    let rows = db
        .all(&SqlStatement::new(
            "SELECT exercise_kind, exercise_id FROM exercise_favorites WHERE user_id = ?",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?
        .results::<FavoriteRow>()?;
    Ok(rows
        .into_iter()
        .map(|row| (row.exercise_kind, row.exercise_id))
        .collect())
}

async fn is_favorite(db: &Db, user_id: &str, kind: &str, exercise_id: &str) -> Result<bool> {
    let row = db
        .first::<FavoriteRow>(&SqlStatement::new(
            "SELECT exercise_kind, exercise_id FROM exercise_favorites \
             WHERE user_id = ? AND exercise_kind = ? AND exercise_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(kind.to_owned()),
                SqlValue::Text(exercise_id.to_owned()),
            ],
        ))
        .await?;
    Ok(row.is_some())
}

async fn resolve(db: &Db, user_id: &str, exercise_id: &str) -> Result<Option<(String, Exercise)>> {
    if let Some(exercise) = exercise::load_master(db, exercise_id).await? {
        return Ok(Some((MASTER_KIND.to_owned(), exercise)));
    }
    if let Some(exercise) = exercise::load_user(db, user_id, exercise_id).await? {
        return Ok(Some((USER_KIND.to_owned(), exercise)));
    }
    Ok(None)
}

async fn set_favorite(
    request: Request,
    context: RouteContext<()>,
    favorite: bool,
) -> Result<Response> {
    let exercise_id = context.param("id").cloned();
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let Some(exercise_id) = exercise_id else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some((kind, exercise)) = resolve(&db, &authenticated.id, &exercise_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let statement = if favorite {
        SqlStatement::new(
            "INSERT INTO exercise_favorites (user_id, exercise_kind, exercise_id, created_at) \
             VALUES (?, ?, ?, ?) ON CONFLICT(user_id, exercise_kind, exercise_id) DO NOTHING",
            vec![
                SqlValue::Text(authenticated.id.clone()),
                SqlValue::Text(kind),
                SqlValue::Text(exercise.id),
                SqlValue::Text(auth::now_iso()),
            ],
        )
    } else {
        SqlStatement::new(
            "DELETE FROM exercise_favorites \
             WHERE user_id = ? AND exercise_kind = ? AND exercise_id = ?",
            vec![
                SqlValue::Text(authenticated.id.clone()),
                SqlValue::Text(kind),
                SqlValue::Text(exercise.id),
            ],
        )
    };
    db.run(&statement).await?;
    Response::from_json(&FavoriteState { favorite })
}
