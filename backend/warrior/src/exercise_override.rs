//! マスターの種目の編集、削除、初期化 (REQ-LIB-013)。
//!
//! 編集と削除は利用者ごとの上書きとして保存し、マスターの行は変えない。
//! 初期化はその利用者の上書きを消し、マスターの元の内容に戻す。
//! 上書きは編集した項目だけを持ち、操作 ID の再送では状態を変えない。

use warrior_core::exercise::ExerciseInput;
use warrior_core::exercise_override::ExerciseOverride;
use warrior_core::library::{ExerciseChange, ExerciseSource};
use worker::{Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::custom_exercise::{applied_change, operation_id};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;
use crate::library::ChangeOutcome;

/// `POST /api/exercises/:id/reset`
pub async fn reset(request: Request, context: RouteContext<()>) -> Result<Response> {
    let exercise_id = context.param("id").cloned();
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let Some(exercise_id) = exercise_id else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(operation_id) = operation_id(&request) else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the operation id is invalid",
        ));
    };
    let db = Db::new(env.d1("DB")?);
    if exercise::load_master(&db, &exercise_id).await?.is_none() {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    if let Some(change) = applied_change(&db, &operation_id, &authenticated.id).await? {
        return Response::from_json(&change);
    }
    let statements = vec![
        exercise::clear_override_statement(&authenticated.id, &exercise_id),
        exercise::operation_statement(
            &operation_id,
            &authenticated.id,
            &exercise_id,
            "reset",
            &auth::now_iso(),
        ),
    ];
    db.batch(&statements).await?;
    Response::from_json(&ExerciseChange {
        id: exercise_id,
        source: ExerciseSource::Master,
        deleted: false,
    })
}

/// マスターの種目を編集し、上書きを保存する。
pub async fn update(
    db: &Db,
    user_id: &str,
    master_exercise_id: &str,
    input: ExerciseInput,
    operation_id: &str,
) -> Result<ChangeOutcome> {
    let Some(master) = exercise::load_master(db, master_exercise_id).await? else {
        return Ok(ChangeOutcome::NotFound);
    };
    if let Some(change) = applied_change(db, operation_id, user_id).await? {
        return Ok(ChangeOutcome::Changed(change));
    }
    let edited = input.into_exercise(master.id.clone(), master.sort_order);
    if edited.validate().is_err() {
        return Ok(ChangeOutcome::Invalid);
    }
    let value = ExerciseOverride::between(&master, &edited);
    let (statements, source) = if value.is_empty() {
        (
            vec![exercise::clear_override_statement(
                user_id,
                master_exercise_id,
            )],
            ExerciseSource::Master,
        )
    } else {
        (
            vec![exercise::override_statement(
                user_id,
                master_exercise_id,
                &value,
            )?],
            ExerciseSource::Overridden,
        )
    };
    let mut statements = statements;
    statements.push(exercise::operation_statement(
        operation_id,
        user_id,
        master_exercise_id,
        "update",
        &auth::now_iso(),
    ));
    db.batch(&statements).await?;
    Ok(ChangeOutcome::Changed(ExerciseChange {
        id: master_exercise_id.to_owned(),
        source,
        deleted: false,
    }))
}

/// マスターの種目を削除する。マスターの行は残し、削除の上書きを保存する。
pub async fn delete(
    db: &Db,
    user_id: &str,
    master_exercise_id: &str,
    operation_id: &str,
) -> Result<ChangeOutcome> {
    if exercise::load_master(db, master_exercise_id)
        .await?
        .is_none()
    {
        return Ok(ChangeOutcome::NotFound);
    }
    if let Some(change) = applied_change(db, operation_id, user_id).await? {
        return Ok(ChangeOutcome::Changed(change));
    }
    let value = ExerciseOverride::deleted();
    let statements = vec![
        exercise::override_statement(user_id, master_exercise_id, &value)?,
        exercise::operation_statement(
            operation_id,
            user_id,
            master_exercise_id,
            "delete",
            &auth::now_iso(),
        ),
    ];
    db.batch(&statements).await?;
    Ok(ChangeOutcome::Changed(ExerciseChange {
        id: master_exercise_id.to_owned(),
        source: ExerciseSource::Overridden,
        deleted: true,
    }))
}
