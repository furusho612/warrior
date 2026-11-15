//! セットの操作の適用前の検査。
//!
//! 完了済みのセットは、更新、削除、完了の取消、同じ ID の作成の再送を拒否する。
//! ドロップセットの位置と、完了に必要な値も、データ文を組み立てる前に確かめる。
//! 段の形と、時間種目の値は、種目の記録方式と左右の指定に照らして確かめる。

use serde::Deserialize;
use warrior_core::exercise::ExerciseSnapshot;
use warrior_core::set_calc::SetKind;
use warrior_core::set_rules;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    SetCompletePayload, SetCreatePayload, SetDeletePayload, SetReorderPayload, SetUpdatePayload,
    ValidatedOperation, ValidatedPayload,
};
use worker::{Response, Result};

use super::validation::{
    completed_set_has_values, completed_set_is_valid, segments_shape_rejection,
    time_values_rejection,
};
use super::ExerciseRules;
use crate::db::Db;
use crate::error::{response, ErrorCode};

/// トレーニングに属するセットの状態。
#[derive(Debug, Deserialize)]
pub(super) struct SetRow {
    /// 完了したか。0 か 1 である。
    pub(super) completed: i64,
    /// セットを持つ種目の行の ID。
    training_exercise_id: String,
    /// 記録した時点の種目の内容。
    pub(super) snapshot: String,
}

/// セットの ID とセット種別。
#[derive(Debug, Deserialize)]
struct SetKindRow {
    /// セットの ID。
    id: String,
    /// 並び順。
    position: i64,
    /// セット種別の文字列。
    kind: String,
}

/// セットの ID と、そのセットが属するトレーニングの ID。
#[derive(Debug, Deserialize)]
struct SetOwnerRow {
    /// セットを持つ種目の行の ID。
    training_exercise_id: String,
    /// 種目の行が属するトレーニングの ID。
    training_id: String,
    /// 完了したか。0 か 1 である。
    completed: i64,
}

/// 段の件数。
#[derive(Debug, Deserialize)]
pub(super) struct CountRow {
    /// 件数。
    pub(super) count: i64,
}

/// 適用の前に、セットと種目の行の存在、完了の値、並べ替えの ID を確かめる。
///
/// 終了したトレーニングでは、完了したセットの編集、削除、完了の取消を許可する。
pub(super) async fn precheck(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
    rules: Option<&ExerciseRules>,
    finished: bool,
) -> Result<Option<Response>> {
    let training_id = &validated.operation.training_id;
    match &validated.payload {
        ValidatedPayload::SetCreate(payload) => {
            let Some(rules) = rules else {
                return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
            };
            precheck_create(db, user_id, training_id, payload, rules).await
        }
        ValidatedPayload::SetUpdate(payload) => {
            let Some(rules) = rules else {
                return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
            };
            precheck_update(db, user_id, training_id, payload, rules, finished).await
        }
        ValidatedPayload::SetDelete(payload) => {
            precheck_delete(db, user_id, training_id, payload, finished).await
        }
        ValidatedPayload::SetReorder(payload) => {
            precheck_reorder(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetComplete(payload) => {
            let Some(rules) = rules else {
                return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
            };
            precheck_complete(db, user_id, training_id, payload, rules, finished).await
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::SessionMemo(_)
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::ExerciseMemo(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_)
        | ValidatedPayload::RoutineCreate(_)
        | ValidatedPayload::RoutineUpdate(_)
        | ValidatedPayload::RoutineDelete(_)
        | ValidatedPayload::RoutineCopy(_) => Ok(None),
    }
}

async fn precheck_create(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetCreatePayload,
    rules: &ExerciseRules,
) -> Result<Option<Response>> {
    if let Some(rejection) = segments_shape_rejection(&payload.segments, rules) {
        return Ok(Some(rejection));
    }
    if let Some(rejection) = time_values_rejection(&payload.segments, rules.record_mode) {
        return Ok(Some(rejection));
    }
    if let Some(existing) = set_owner_row(db, user_id, &payload.set_id).await? {
        if existing.training_id != training_id
            || existing.training_exercise_id != payload.training_exercise_id
        {
            return Ok(Some(response(
                ErrorCode::Conflict,
                "the set belongs to another exercise",
            )));
        }
        if existing.completed != 0 {
            return Ok(Some(completed_set_rejection()));
        }
    }
    if !drop_positions_allowed(
        db,
        user_id,
        &payload.training_exercise_id,
        SetChange {
            set_id: &payload.set_id,
            position: payload.position,
            kind: payload.kind,
        },
    )
    .await?
    {
        return Ok(Some(drop_position_rejection()));
    }
    Ok(None)
}

async fn precheck_update(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetUpdatePayload,
    rules: &ExerciseRules,
    finished: bool,
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if row.completed != 0 && !finished {
        return Ok(Some(completed_set_rejection()));
    }
    if let Some(rejection) = segments_shape_rejection(&payload.segments, rules) {
        return Ok(Some(rejection));
    }
    if let Some(rejection) = time_values_rejection(&payload.segments, rules.record_mode) {
        return Ok(Some(rejection));
    }
    if !drop_positions_allowed(
        db,
        user_id,
        &row.training_exercise_id,
        SetChange {
            set_id: &payload.set_id,
            position: payload.position,
            kind: payload.kind,
        },
    )
    .await?
    {
        return Ok(Some(drop_position_rejection()));
    }
    if payload.completed {
        let snapshot = parse_snapshot(&row.snapshot)?;
        if payload
            .to_record(&snapshot.exercise_id, snapshot.record_mode)
            .is_err()
            || !payload
                .segments
                .iter()
                .all(|segment| segment.has_completion_values(rules.record_mode, rules.lateral()))
        {
            return Ok(Some(invalid_completion()));
        }
    }
    Ok(None)
}

async fn precheck_delete(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetDeletePayload,
    finished: bool,
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if row.completed != 0 && !finished {
        return Ok(Some(completed_set_rejection()));
    }
    if !delete_positions_allowed(db, user_id, &row.training_exercise_id, &payload.set_id).await? {
        return Ok(Some(delete_before_drop_rejection()));
    }
    Ok(None)
}

async fn precheck_reorder(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetReorderPayload,
) -> Result<Option<Response>> {
    if !exercise_in_training(db, user_id, training_id, &payload.training_exercise_id).await? {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    }
    let current = set_kind_rows(db, user_id, &payload.training_exercise_id).await?;
    let current_ids = current.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    if !same_set_ids(&current_ids, &payload.ordered_ids) {
        return Ok(Some(response(
            ErrorCode::Conflict,
            "the set order is stale",
        )));
    }
    if !ordered_drop_positions_allowed(&current, &payload.ordered_ids)? {
        return Ok(Some(drop_position_rejection()));
    }
    Ok(None)
}

async fn precheck_complete(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetCompletePayload,
    _rules: &ExerciseRules,
    finished: bool,
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if !payload.completed && row.completed != 0 && !finished {
        return Ok(Some(completed_set_rejection()));
    }
    if payload.completed && row.completed == 0 {
        if !completed_set_has_values(db, user_id, training_id, &payload.set_id).await? {
            return Ok(Some(response(
                ErrorCode::BadRequest,
                "the set has an incomplete segment",
            )));
        }
        if !completed_set_is_valid(db, user_id, training_id, &payload.set_id).await? {
            return Ok(Some(response(
                ErrorCode::BadRequest,
                "the set is not valid for completion",
            )));
        }
    }
    Ok(None)
}

async fn exercise_in_training(
    db: &Db,
    user_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Result<bool> {
    let count = db
        .first::<CountRow>(&SqlStatement::new(
            "SELECT COUNT(*) AS count FROM training_exercises \
             WHERE id = ? AND user_id = ? AND training_id = ?",
            vec![
                SqlValue::Text(training_exercise_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?
        .map_or(0, |row| row.count);
    Ok(count > 0)
}

pub(super) async fn set_row(
    db: &Db,
    user_id: &str,
    training_id: &str,
    set_id: &str,
) -> Result<Option<SetRow>> {
    db.first(&SqlStatement::new(
        "SELECT s.completed, s.training_exercise_id, te.snapshot FROM sets s \
         JOIN training_exercises te ON te.id = s.training_exercise_id \
         WHERE s.id = ? AND s.user_id = ? AND te.training_id = ? AND te.user_id = ?",
        vec![
            SqlValue::Text(set_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

async fn set_owner_row(db: &Db, user_id: &str, set_id: &str) -> Result<Option<SetOwnerRow>> {
    db.first(&SqlStatement::new(
        "SELECT s.training_exercise_id, te.training_id, s.completed FROM sets s \
         JOIN training_exercises te ON te.id = s.training_exercise_id \
         WHERE s.id = ? AND s.user_id = ?",
        vec![
            SqlValue::Text(set_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

async fn set_kind_rows(
    db: &Db,
    user_id: &str,
    training_exercise_id: &str,
) -> Result<Vec<SetKindRow>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT id, position, kind FROM sets \
             WHERE user_id = ? AND training_exercise_id = ? \
             ORDER BY position, id",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_exercise_id.to_owned()),
            ],
        ))
        .await?;
    result.results::<SetKindRow>()
}

/// ドロップセットの位置を検査する対象の変更。
struct SetChange<'a> {
    /// 追加または更新するセットの ID。
    set_id: &'a str,
    /// 変更後の並び順。
    position: i64,
    /// 変更後のセット種別。
    kind: SetKind,
}

async fn drop_positions_allowed(
    db: &Db,
    user_id: &str,
    training_exercise_id: &str,
    change: SetChange<'_>,
) -> Result<bool> {
    let mut rows = set_kind_rows(db, user_id, training_exercise_id).await?;
    let kind = change.kind.as_str().to_owned();
    match rows.iter_mut().find(|row| row.id == change.set_id) {
        Some(row) => {
            row.position = change.position;
            row.kind = kind;
        }
        None => rows.push(SetKindRow {
            id: change.set_id.to_owned(),
            position: change.position,
            kind,
        }),
    }
    rows.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.id.cmp(&right.id))
    });
    let kinds = parse_kinds(rows.iter().map(|row| row.kind.as_str()))?;
    Ok(set_rules::drop_set_positions_valid(&kinds))
}

async fn delete_positions_allowed(
    db: &Db,
    user_id: &str,
    training_exercise_id: &str,
    set_id: &str,
) -> Result<bool> {
    let rows = set_kind_rows(db, user_id, training_exercise_id).await?;
    let Some(index) = rows.iter().position(|row| row.id == set_id) else {
        return Ok(false);
    };
    let next = rows.get(index + 1).map(|row| row.kind.as_str());
    let next = match next {
        None => None,
        Some(kind) => Some(SetKind::parse(kind).ok_or_else(|| {
            worker::Error::RustError("the stored set kind is invalid".to_owned())
        })?),
    };
    Ok(set_rules::delete_set_allowed(next))
}

fn ordered_drop_positions_allowed(rows: &[SetKindRow], ordered_ids: &[String]) -> Result<bool> {
    let mut kinds = Vec::with_capacity(ordered_ids.len());
    for id in ordered_ids {
        let Some(row) = rows.iter().find(|row| row.id == *id) else {
            return Ok(false);
        };
        kinds.push(row.kind.as_str());
    }
    Ok(set_rules::drop_set_positions_valid(&parse_kinds(
        kinds.into_iter(),
    )?))
}

fn parse_kinds<'a>(kinds: impl Iterator<Item = &'a str>) -> Result<Vec<SetKind>> {
    kinds
        .map(|kind| {
            SetKind::parse(kind).ok_or_else(|| {
                worker::Error::RustError("the stored set kind is invalid".to_owned())
            })
        })
        .collect()
}

fn parse_snapshot(snapshot: &str) -> Result<ExerciseSnapshot> {
    serde_json::from_str(snapshot).map_err(|error| {
        worker::Error::RustError(format!("the stored exercise snapshot is invalid: {error}"))
    })
}

fn drop_position_rejection() -> Response {
    response(
        ErrorCode::BadRequest,
        "a drop set cannot be first or follow a warmup set",
    )
}

fn completed_set_rejection() -> Response {
    response(
        ErrorCode::Conflict,
        "a completed set cannot be edited or deleted",
    )
}

fn delete_before_drop_rejection() -> Response {
    response(
        ErrorCode::BadRequest,
        "the next drop set must be deleted first",
    )
}

fn invalid_completion() -> Response {
    response(ErrorCode::BadRequest, "the set is not valid for completion")
}

pub(super) fn same_set_ids(current: &[String], ordered: &[String]) -> bool {
    let mut current = current.to_vec();
    let mut ordered = ordered.to_vec();
    current.sort_unstable();
    ordered.sort_unstable();
    current == ordered
}
