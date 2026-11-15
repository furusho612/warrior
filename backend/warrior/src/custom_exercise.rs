//! 独自種目の追加、編集、削除 (REQ-LIB-005)。
//!
//! 追加では名称、読み、部位区分、記録方式を必須とし、ID はサーバーが UUID v4 で決める。
//! 編集と削除は、その種目の所有者だけに許す。所有者でない場合は 404 にする。
//! 変更は `exercise_operations` の操作 ID で重複を防ぎ、同じ操作の再送では状態を変えない。

use warrior_core::exercise::ExerciseInput;
use warrior_core::ids::is_valid_uuid_v4;
use warrior_core::library::{ExerciseChange, ExerciseSource};
use worker::{Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;
use crate::library::{ChangeOutcome, EXERCISE_OPERATION_HEADER};

/// `POST /api/exercises`
pub async fn create(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let operation_id = match operation_id(&request) {
        Some(operation_id) => operation_id,
        None => {
            return Ok(response(
                ErrorCode::BadRequest,
                "the operation id is invalid",
            ))
        }
    };
    let input: ExerciseInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the exercise is invalid")),
    };
    let id = crate::random::random_uuid()?;
    let exercise = input.into_exercise(id.clone(), 0);
    if exercise.validate().is_err() {
        return Ok(response(ErrorCode::BadRequest, "the exercise is invalid"));
    }
    let db = Db::new(env.d1("DB")?);
    if let Some(change) = applied_change(&db, &operation_id, &authenticated.id).await? {
        return Response::from_json(&change);
    }
    let mut statements = exercise::user_statements(&authenticated.id, &exercise)?;
    statements.push(exercise::operation_statement(
        &operation_id,
        &authenticated.id,
        &id,
        "create",
        &auth::now_iso(),
    ));
    db.batch(&statements).await?;
    Response::from_json(&ExerciseChange {
        id,
        source: ExerciseSource::Custom,
        deleted: false,
    })
}

/// 独自種目を編集する。
pub async fn update(
    db: &Db,
    user_id: &str,
    id: &str,
    input: ExerciseInput,
    operation_id: &str,
) -> Result<ChangeOutcome> {
    let Some(owner) = exercise::load_user_owner(db, id).await? else {
        return Ok(ChangeOutcome::NotFound);
    };
    if auth::owner_result(user_id, &owner).is_err() {
        return Ok(ChangeOutcome::NotFound);
    }
    let Some(current) = exercise::load_user(db, user_id, id).await? else {
        return Ok(ChangeOutcome::NotFound);
    };
    let exercise = input.into_exercise(id.to_owned(), current.sort_order);
    if exercise.validate().is_err() {
        return Ok(ChangeOutcome::Invalid);
    }
    if let Some(change) = applied_change(db, operation_id, user_id).await? {
        return Ok(ChangeOutcome::Changed(change));
    }
    let mut statements = exercise::user_statements(user_id, &exercise)?;
    statements.push(exercise::operation_statement(
        operation_id,
        user_id,
        id,
        "update",
        &auth::now_iso(),
    ));
    db.batch(&statements).await?;
    Ok(ChangeOutcome::Changed(ExerciseChange {
        id: id.to_owned(),
        source: ExerciseSource::Custom,
        deleted: false,
    }))
}

/// 独自種目を削除する。
pub async fn delete(db: &Db, user_id: &str, id: &str, operation_id: &str) -> Result<ChangeOutcome> {
    let Some(owner) = exercise::load_user_owner(db, id).await? else {
        return Ok(ChangeOutcome::NotFound);
    };
    if auth::owner_result(user_id, &owner).is_err() {
        return Ok(ChangeOutcome::NotFound);
    }
    if let Some(change) = applied_change(db, operation_id, user_id).await? {
        return Ok(ChangeOutcome::Changed(change));
    }
    let mut statements = exercise::delete_user_statements(user_id, id);
    statements.push(exercise::operation_statement(
        operation_id,
        user_id,
        id,
        "delete",
        &auth::now_iso(),
    ));
    db.batch(&statements).await?;
    Ok(ChangeOutcome::Changed(ExerciseChange {
        id: id.to_owned(),
        source: ExerciseSource::Custom,
        deleted: true,
    }))
}

/// API の要求の操作 ID を読む。無い場合は新しい UUID v4 を作る。
pub(crate) fn operation_id(request: &Request) -> Option<String> {
    match request.headers().get(EXERCISE_OPERATION_HEADER) {
        Ok(Some(value)) => is_valid_uuid_v4(&value).then_some(value),
        Ok(None) => crate::random::random_uuid().ok(),
        Err(_) => None,
    }
}

/// 同じ操作 ID を適用済みなら、前回の結果を返す。
pub(crate) async fn applied_change(
    db: &Db,
    operation_id: &str,
    user_id: &str,
) -> Result<Option<ExerciseChange>> {
    let Some(applied) = exercise::applied_exercise_operation(db, operation_id).await? else {
        return Ok(None);
    };
    if applied.user_id != user_id {
        return Ok(None);
    }
    let source = match applied.kind.as_str() {
        "create" => ExerciseSource::Custom,
        "reset" => ExerciseSource::Master,
        "update" | "delete" => {
            if exercise::load_master(db, &applied.exercise_id)
                .await?
                .is_some()
            {
                ExerciseSource::Overridden
            } else {
                ExerciseSource::Custom
            }
        }
        _ => ExerciseSource::Custom,
    };
    Ok(Some(ExerciseChange {
        id: applied.exercise_id,
        source,
        deleted: applied.kind == "delete",
    }))
}
