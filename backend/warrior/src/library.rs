//! 種目ライブラリの一覧、詳細、お気に入り、種目の変更の D1 の読み書き。
//!
//! 並びと絞り込みとページングの規則は `warrior_core::library` が持つ。このモジュールは
//! 種目マスター、独自種目、利用者の上書きとお気に入りの行を読み、条件を適用して応答を
//! 組み立てる。記録への種目の選択は全件を使うため、`paged=true` を指定しない一覧は
//! 全件を返す。

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use serde::Deserialize;
use warrior_core::cursor::{decode_cursor, Cursor};
use warrior_core::exercise::{BodyPart, Equipment, Exercise, ExerciseInput};
use warrior_core::library::{
    filter_exercises, paginate, sort_exercises, ExerciseChange, ExerciseFilter, ExerciseList,
    ExerciseSource, FavoriteState, LibraryExercise,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::custom_exercise;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;
use crate::exercise_override;

/// 種目の変更の操作 ID を載せるヘッダ。
///
/// 端末の送信待ちの列が同じ変更を再送しても、サーバーの状態を変えないために使う。
pub const EXERCISE_OPERATION_HEADER: &str = "X-Operation-Id";

/// 種目の変更の結果。
pub enum ChangeOutcome {
    /// 変更した。
    Changed(ExerciseChange),
    /// 種目が無い、または別の利用者のもの。
    NotFound,
    /// 入力が種目の規則に合わない。
    Invalid,
}

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
    let exercises = match effective_exercises(&db, &authenticated.id).await {
        Ok(exercises) => exercises,
        Err(error) => {
            console_error!("failed to list the exercises: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let items = exercises
        .into_iter()
        .map(|(kind, exercise, source)| LibraryExercise {
            favorite: favorites.contains(&(kind, exercise.id.clone())),
            exercise,
            source,
            deleted: false,
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
    if let Some(master) = exercise::load_master(&db, &exercise_id).await? {
        let value = exercise::load_override(&db, &authenticated.id, &exercise_id).await?;
        if value.as_ref().is_some_and(|value| value.deleted) {
            let favorite = is_favorite(&db, &authenticated.id, MASTER_KIND, &exercise_id).await?;
            return Response::from_json(&LibraryExercise {
                exercise: master,
                favorite,
                source: ExerciseSource::Overridden,
                deleted: true,
            });
        }
        let Some((exercise, source)) = exercise::effective(&master, value.as_ref()) else {
            return Ok(response(ErrorCode::NotFound, "resource not found"));
        };
        let favorite = is_favorite(&db, &authenticated.id, MASTER_KIND, &exercise.id).await?;
        return Response::from_json(&LibraryExercise {
            exercise,
            favorite,
            source,
            deleted: false,
        });
    }
    let Some((kind, exercise, source)) = resolve(&db, &authenticated.id, &exercise_id).await?
    else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let favorite = is_favorite(&db, &authenticated.id, &kind, &exercise.id).await?;
    Response::from_json(&LibraryExercise {
        exercise,
        favorite,
        source,
        deleted: false,
    })
}

/// `PUT /api/exercises/:id/favorite`
pub async fn add_favorite(request: Request, context: RouteContext<()>) -> Result<Response> {
    set_favorite(request, context, true).await
}

/// `DELETE /api/exercises/:id/favorite`
pub async fn remove_favorite(request: Request, context: RouteContext<()>) -> Result<Response> {
    set_favorite(request, context, false).await
}

/// `PUT /api/exercises/:id`
///
/// マスターの種目は利用者の上書きとして保存し、独自種目はその行を更新する。
pub async fn update_exercise(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let exercise_id = context.param("id").cloned();
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let Some(exercise_id) = exercise_id else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(operation_id) = custom_exercise::operation_id(&request) else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the operation id is invalid",
        ));
    };
    let input: ExerciseInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the exercise is invalid")),
    };
    let db = Db::new(env.d1("DB")?);
    let outcome = if exercise::load_master(&db, &exercise_id).await?.is_some() {
        exercise_override::update(&db, &authenticated.id, &exercise_id, input, &operation_id)
            .await?
    } else {
        custom_exercise::update(&db, &authenticated.id, &exercise_id, input, &operation_id).await?
    };
    change_response(outcome)
}

/// `DELETE /api/exercises/:id`
///
/// マスターの種目は削除の上書きを保存し、独自種目は行を削除する。
pub async fn delete_exercise(request: Request, context: RouteContext<()>) -> Result<Response> {
    let exercise_id = context.param("id").cloned();
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let Some(exercise_id) = exercise_id else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(operation_id) = custom_exercise::operation_id(&request) else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the operation id is invalid",
        ));
    };
    let db = Db::new(env.d1("DB")?);
    let outcome = if exercise::load_master(&db, &exercise_id).await?.is_some() {
        exercise_override::delete(&db, &authenticated.id, &exercise_id, &operation_id).await?
    } else {
        custom_exercise::delete(&db, &authenticated.id, &exercise_id, &operation_id).await?
    };
    change_response(outcome)
}

/// 種目の変更の結果を HTTP の応答にする。
fn change_response(outcome: ChangeOutcome) -> Result<Response> {
    match outcome {
        ChangeOutcome::Changed(change) => Response::from_json(&change),
        ChangeOutcome::NotFound => Ok(response(ErrorCode::NotFound, "resource not found")),
        ChangeOutcome::Invalid => Ok(response(ErrorCode::BadRequest, "the exercise is invalid")),
    }
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

pub(crate) async fn resolve(
    db: &Db,
    user_id: &str,
    exercise_id: &str,
) -> Result<Option<(String, Exercise, ExerciseSource)>> {
    if let Some((master, _)) = exercise::load_master_with_source(db, exercise_id).await? {
        let value = exercise::load_override(db, user_id, exercise_id).await?;
        let Some((effective, source)) = exercise::effective(&master, value.as_ref()) else {
            return Ok(None);
        };
        return Ok(Some((MASTER_KIND.to_owned(), effective, source)));
    }
    if let Some((exercise, source)) =
        exercise::load_user_with_source(db, user_id, exercise_id).await?
    {
        return Ok(Some((USER_KIND.to_owned(), exercise, source)));
    }
    Ok(None)
}

/// 上書きを適用した種目を読む。履歴とグラフが使う。
pub async fn effective_exercise(
    db: &Db,
    user_id: &str,
    exercise_id: &str,
) -> Result<Option<Exercise>> {
    Ok(resolve(db, user_id, exercise_id)
        .await?
        .map(|(_, exercise, _)| exercise))
}

/// マスターと独自種目の一覧に上書きを適用する。
async fn effective_exercises(
    db: &Db,
    user_id: &str,
) -> Result<Vec<(String, Exercise, ExerciseSource)>> {
    let overrides = exercise::list_overrides(db, user_id).await?;
    let mut items = Vec::new();
    for (master, _) in exercise::list_master(db).await? {
        let value = overrides.get(&master.id);
        let Some((effective, source)) = exercise::effective(&master, value) else {
            continue;
        };
        items.push((MASTER_KIND.to_owned(), effective, source));
    }
    for (exercise, source) in exercise::list_user(db, user_id).await? {
        items.push((USER_KIND.to_owned(), exercise, source));
    }
    Ok(items)
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
    let Some((kind, exercise, _)) = resolve(&db, &authenticated.id, &exercise_id).await? else {
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
