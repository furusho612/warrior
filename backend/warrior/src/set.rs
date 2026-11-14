//! セットの追加、更新、削除、並べ替え、完了の操作の適用。
//!
//! 操作は端末の送信待ちの列から届き、`backend/warrior/src/sync.rs` が編集権を検査する。
//! このモジュールは、同じ編集権の `EXISTS` 条件を各データ文に付け、セットの適用と
//! 操作 ID の保存を同じ `batch()` に入れる (ADR-002、ADR-003)。
//! データ文が 0 行のときは操作 ID を残さず、段の文は保存した操作 ID があるときだけ動く。
//! 段の重量は、自重種目ではセットごとの加重を表す。記録した時点の体重は `sets` が持つ。

use serde::Deserialize;
use warrior_core::exercise::{ExerciseSnapshot, GripSelection, RecordMode};
use warrior_core::set_calc::SetKind;
use warrior_core::set_rules;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    SetCompletePayload, SetCreatePayload, SetDeletePayload, SetReorderPayload, SetSegmentPayload,
    SetUpdatePayload, ValidatedOperation, ValidatedPayload,
};
use worker::{Response, Result};

use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::sync::{
    applied_statement, owner_condition, ownership_guard_statement, session_row, write_decision,
    BatchOutcome, OwnerCondition,
};
use crate::training_session_exercise::{ownership_condition, ExerciseOwnership};

/// トレーニングに属するセットの状態。
#[derive(Debug, Deserialize)]
struct SetRow {
    /// 完了したか。0 か 1 である。
    completed: i64,
    /// セットを持つ種目の行の ID。
    training_exercise_id: String,
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
}

/// 段のある値の件数。
#[derive(Debug, Deserialize)]
struct SetValueRow {
    /// 段の件数。
    segments: i64,
    /// 完了に必要な値を持つ段の件数。
    ready: i64,
}

/// 種目の記録方式を読むための snapshot。
#[derive(Debug, Deserialize)]
struct ExerciseModeRow {
    /// 記録時点の種目の内容の JSON。
    snapshot: String,
}

/// 行の件数。
#[derive(Debug, Deserialize)]
struct CountRow {
    /// 件数。
    count: i64,
}

/// セットの操作の batch を組み立てる。
///
/// 先頭の文で編集権を確認してトレーニングの更新時刻を進め、セットのデータ文、条件付きの
/// 操作 ID の保存、段のデータ文の順に並べる。段の文は操作 ID を保存したときだけ動く。
pub(crate) async fn set_batch(
    db: &Db,
    user_id: &str,
    terminal_id: Option<&str>,
    validated: &ValidatedOperation,
    now: &str,
) -> Result<BatchOutcome> {
    let Some(session) = session_row(db, &validated.operation.training_id).await? else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    };
    if session.user_id != user_id {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    }
    let finished = session.finished_at.is_some();
    let decision = write_decision(finished, session.owner_terminal_id.as_deref(), terminal_id);
    let Some(owner) = owner_condition(decision, terminal_id) else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        )));
    };
    if let Some(rejection) = precheck(db, user_id, validated).await? {
        return Ok(BatchOutcome::Reject(rejection));
    }
    let mode = payload_record_mode(db, user_id, validated).await?;
    let ownership = match owner {
        OwnerCondition::Claim(terminal_id) | OwnerCondition::Owned(terminal_id) => {
            ExerciseOwnership::Terminal(terminal_id)
        }
        OwnerCondition::Finished => ExerciseOwnership::Finished,
    };
    let Some(data) = set_statements(user_id, validated, mode, ownership, now) else {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::BadRequest,
            "operation is invalid",
        )));
    };
    let mut statements = vec![
        ownership_guard_statement(user_id, owner, &validated.operation.training_id, now),
        data.main,
        applied_statement(user_id, &validated.operation, now),
    ];
    statements.extend(data.children);
    Ok(BatchOutcome::Apply(statements))
}

/// セットのデータ文。先頭の文が操作の成否を決め、残りは操作 ID の保存の後に動く。
struct SetData {
    /// セットへの適用。
    main: SqlStatement,
    /// 操作 ID を保存したときだけ動く段の文。
    children: Vec<SqlStatement>,
}

/// 適用の前に、セットと種目の行の存在、完了の値、並べ替えの ID を確かめる。
async fn precheck(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
) -> Result<Option<Response>> {
    let training_id = &validated.operation.training_id;
    match &validated.payload {
        ValidatedPayload::SetCreate(payload) => {
            precheck_create(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetUpdate(payload) => {
            precheck_update(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetDelete(payload) => {
            precheck_delete(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetReorder(payload) => {
            precheck_reorder(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetComplete(payload) => {
            precheck_complete(db, user_id, training_id, payload).await
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_) => Ok(None),
    }
}

async fn precheck_create(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetCreatePayload,
) -> Result<Option<Response>> {
    let Some(mode) = exercise_mode(db, user_id, training_id, &payload.training_exercise_id).await?
    else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if mode == RecordMode::Time && has_non_time_values(&payload.segments) {
        return Ok(Some(time_value_rejection()));
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
    }
    if !drop_positions_allowed(
        db,
        user_id,
        &payload.training_exercise_id,
        Some(SetChange {
            set_id: &payload.set_id,
            position: payload.position,
            kind: payload.kind,
        }),
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
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    let Some(mode) = exercise_mode(db, user_id, training_id, &row.training_exercise_id).await?
    else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if mode == RecordMode::Time && has_non_time_values(&payload.segments) {
        return Ok(Some(time_value_rejection()));
    }
    if !drop_positions_allowed(
        db,
        user_id,
        &row.training_exercise_id,
        Some(SetChange {
            set_id: &payload.set_id,
            position: payload.position,
            kind: payload.kind,
        }),
    )
    .await?
    {
        return Ok(Some(drop_position_rejection()));
    }
    Ok(None)
}

async fn precheck_delete(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &SetDeletePayload,
) -> Result<Option<Response>> {
    if set_row(db, user_id, training_id, &payload.set_id)
        .await?
        .is_none()
    {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
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
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    let Some(mode) = exercise_mode(db, user_id, training_id, &row.training_exercise_id).await?
    else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if payload.completed
        && row.completed == 0
        && !completed_set_has_values(db, user_id, &payload.set_id, mode).await?
    {
        return Ok(Some(response(
            ErrorCode::BadRequest,
            "the set has no values to complete",
        )));
    }
    Ok(None)
}

fn set_statements(
    user_id: &str,
    validated: &ValidatedOperation,
    mode: Option<RecordMode>,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Option<SetData> {
    let operation = &validated.operation;
    let training_id = &operation.training_id;
    let applied = applied_exists(user_id, &operation.operation_id);
    match &validated.payload {
        ValidatedPayload::SetCreate(payload) => {
            let mode = mode?;
            let context = SegmentStatements {
                user_id,
                training_id,
                set_id: &payload.set_id,
                mode,
                ownership,
                applied: &applied,
                now,
            };
            Some(SetData {
                main: create_set_statement(user_id, training_id, payload, ownership, now)?,
                children: segment_replace_statements(&context, &payload.segments),
            })
        }
        ValidatedPayload::SetUpdate(payload) => {
            let mode = mode?;
            let context = SegmentStatements {
                user_id,
                training_id,
                set_id: &payload.set_id,
                mode,
                ownership,
                applied: &applied,
                now,
            };
            Some(SetData {
                main: update_set_statement(user_id, training_id, payload, mode, ownership, now),
                children: segment_replace_statements(&context, &payload.segments),
            })
        }
        ValidatedPayload::SetDelete(payload) => Some(SetData {
            main: delete_set_statement(user_id, training_id, payload, ownership),
            children: vec![delete_segments_statement(
                user_id,
                &payload.set_id,
                ownership,
                &applied,
            )],
        }),
        ValidatedPayload::SetReorder(payload) => Some(SetData {
            main: reorder_set_statement(user_id, training_id, payload, ownership, now),
            children: Vec::new(),
        }),
        ValidatedPayload::SetComplete(payload) => Some(SetData {
            main: complete_set_statement(user_id, training_id, payload, ownership, now),
            children: Vec::new(),
        }),
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_) => None,
    }
}

fn create_set_statement(
    user_id: &str,
    training_id: &str,
    payload: &SetCreatePayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Option<SqlStatement> {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let grip = grip_json(payload.grip)?;
    let mut bindings = vec![
        SqlValue::Text(payload.set_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Integer(payload.position),
        SqlValue::Text(payload.kind.as_str().to_owned()),
        optional_real(payload.rpe),
        grip,
        optional_real(payload.body_weight_kg),
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    Some(SqlStatement::new(
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, rpe, grip, \
              body_weight_kg, recorded_at, created_at, updated_at) \
             SELECT ?, ?, ?, ?, ?, 0, ?, ?, ?, NULL, ?, ? \
             WHERE EXISTS (SELECT 1 FROM training_exercises te \
              WHERE te.id = ? AND te.user_id = ? AND te.training_id = ?) \
             AND {condition} \
             ON CONFLICT(id) DO UPDATE SET position = excluded.position, \
             kind = excluded.kind, rpe = excluded.rpe, grip = excluded.grip, \
             body_weight_kg = excluded.body_weight_kg, updated_at = excluded.updated_at \
             WHERE sets.user_id = excluded.user_id \
             AND sets.training_exercise_id = excluded.training_exercise_id"
        ),
        bindings,
    ))
}

fn update_set_statement(
    user_id: &str,
    training_id: &str,
    payload: &SetUpdatePayload,
    mode: RecordMode,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let completed = payload.completed && segments_are_complete(&payload.segments, mode);
    let mut bindings = vec![
        SqlValue::Integer(payload.position),
        SqlValue::Text(payload.kind.as_str().to_owned()),
        SqlValue::Integer(if completed { 1 } else { 0 }),
        optional_real(payload.rpe),
        grip_json(payload.grip).unwrap_or(SqlValue::Null),
        optional_real(payload.body_weight_kg),
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(payload.set_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "UPDATE sets SET position = ?, kind = ?, completed = ?, rpe = ?, grip = ?, \
             body_weight_kg = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? \
             AND training_exercise_id IN \
              (SELECT id FROM training_exercises WHERE training_id = ? AND user_id = ?) \
             AND {condition}"
        ),
        bindings,
    )
}

fn delete_set_statement(
    user_id: &str,
    training_id: &str,
    payload: &SetDeletePayload,
    ownership: ExerciseOwnership<'_>,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(payload.set_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "DELETE FROM sets WHERE id = ? AND user_id = ? \
             AND training_exercise_id IN \
              (SELECT id FROM training_exercises WHERE training_id = ? AND user_id = ?) \
             AND {condition}"
        ),
        bindings,
    )
}

fn complete_set_statement(
    user_id: &str,
    training_id: &str,
    payload: &SetCompletePayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut bindings = vec![
        SqlValue::Integer(if payload.completed { 1 } else { 0 }),
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(payload.set_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "UPDATE sets SET completed = ?, updated_at = ? \
             WHERE id = ? AND user_id = ? \
             AND training_exercise_id IN \
              (SELECT id FROM training_exercises WHERE training_id = ? AND user_id = ?) \
             AND {condition}"
        ),
        bindings,
    )
}

fn reorder_set_statement(
    user_id: &str,
    training_id: &str,
    payload: &SetReorderPayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut case = Vec::new();
    let mut bindings = Vec::new();
    for (index, id) in payload.ordered_ids.iter().enumerate() {
        case.push("WHEN ? THEN ?");
        bindings.push(SqlValue::Text(id.clone()));
        bindings.push(SqlValue::Integer(
            i64::try_from(index + 1).unwrap_or(i64::MAX),
        ));
    }
    let placeholders = vec!["?"; payload.ordered_ids.len()].join(", ");
    bindings.push(SqlValue::Text(now.to_owned()));
    for id in &payload.ordered_ids {
        bindings.push(SqlValue::Text(id.clone()));
    }
    bindings.push(SqlValue::Text(user_id.to_owned()));
    bindings.push(SqlValue::Text(payload.training_exercise_id.clone()));
    bindings.append(&mut condition_bindings);
    SqlStatement::new(
        format!(
            "UPDATE sets SET position = CASE id {} ELSE position END, updated_at = ? \
             WHERE id IN ({placeholders}) AND user_id = ? AND training_exercise_id = ? \
             AND {condition}",
            case.join(" ")
        ),
        bindings,
    )
}

/// 段の文を作るための、対象と条件の組。
struct SegmentStatements<'a> {
    user_id: &'a str,
    training_id: &'a str,
    set_id: &'a str,
    mode: RecordMode,
    ownership: ExerciseOwnership<'a>,
    applied: &'a AppliedCondition,
    now: &'a str,
}

fn segment_replace_statements(
    context: &SegmentStatements<'_>,
    segments: &[SetSegmentPayload],
) -> Vec<SqlStatement> {
    let mut statements = vec![delete_segments_statement(
        context.user_id,
        context.set_id,
        context.ownership,
        context.applied,
    )];
    for segment in segments {
        statements.push(segment_insert_statement(context, segment));
    }
    statements
}

fn delete_segments_statement(
    user_id: &str,
    set_id: &str,
    ownership: ExerciseOwnership<'_>,
    applied: &AppliedCondition,
) -> SqlStatement {
    // 段は set_id と user_id に絞る。トレーニングと操作 ID の条件は、セットの適用が
    // 成功したときだけ段を動かすために付ける。
    let (training_condition, training_bindings) =
        ownership_condition_placeholder(user_id, ownership);
    let mut bindings = vec![
        SqlValue::Text(set_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    bindings.extend(training_bindings);
    bindings.extend(applied.bindings.iter().cloned());
    SqlStatement::new(
        format!(
            "DELETE FROM set_segments WHERE set_id = ? AND user_id = ? \
             AND {training_condition} AND {}",
            applied.sql
        ),
        bindings,
    )
}

fn segment_insert_statement(
    context: &SegmentStatements<'_>,
    segment: &SetSegmentPayload,
) -> SqlStatement {
    let (condition, mut condition_bindings) =
        ownership_condition(context.user_id, context.training_id, context.ownership);
    let mut bindings = segment_bindings(
        context.user_id,
        context.set_id,
        segment,
        context.mode,
        context.now,
    );
    bindings.append(&mut condition_bindings);
    bindings.extend(context.applied.bindings.iter().cloned());
    SqlStatement::new(
        format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              duration_seconds, left_weight_kg, left_reps, left_duration_seconds, \
              right_weight_kg, right_reps, right_duration_seconds, created_at, updated_at) \
             SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
             WHERE {condition} AND {} \
             ON CONFLICT(id) DO UPDATE SET position = excluded.position, \
             weight_kg = excluded.weight_kg, reps = excluded.reps, \
             momentum_reps = excluded.momentum_reps, assisted_reps = excluded.assisted_reps, \
             duration_seconds = excluded.duration_seconds, \
             left_weight_kg = excluded.left_weight_kg, left_reps = excluded.left_reps, \
             left_duration_seconds = excluded.left_duration_seconds, \
             right_weight_kg = excluded.right_weight_kg, right_reps = excluded.right_reps, \
             right_duration_seconds = excluded.right_duration_seconds, \
             updated_at = excluded.updated_at \
             WHERE set_segments.user_id = excluded.user_id \
             AND set_segments.set_id = excluded.set_id",
            context.applied.sql
        ),
        bindings,
    )
}

fn segment_bindings(
    user_id: &str,
    set_id: &str,
    segment: &SetSegmentPayload,
    mode: RecordMode,
    now: &str,
) -> Vec<SqlValue> {
    let is_time = mode == RecordMode::Time;
    let mut bindings = vec![
        SqlValue::Text(segment.segment_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(set_id.to_owned()),
        SqlValue::Integer(segment.position),
        optional_real((!is_time).then_some(segment.weight_kg)),
        optional_integer((!is_time).then(|| i64::from(segment.reps))),
        SqlValue::Integer(if is_time {
            0
        } else {
            i64::from(segment.momentum_reps)
        }),
        SqlValue::Integer(if is_time {
            0
        } else {
            i64::from(segment.assisted_reps)
        }),
        optional_integer(if is_time {
            segment.duration_seconds.map(i64::from)
        } else {
            None
        }),
    ];
    for side in [segment.left.as_ref(), segment.right.as_ref()] {
        let weight = if is_time {
            None
        } else {
            side.map(|side| side.weight_kg)
        };
        let reps = if is_time {
            None
        } else {
            side.map(|side| i64::from(side.reps))
        };
        bindings.push(optional_real(weight));
        bindings.push(optional_integer(reps));
        bindings.push(optional_integer(if is_time {
            side.and_then(|side| side.duration_seconds).map(i64::from)
        } else {
            None
        }));
    }
    bindings.push(SqlValue::Text(now.to_owned()));
    bindings.push(SqlValue::Text(now.to_owned()));
    bindings
}

/// 操作 ID を保存したことを表す条件。
struct AppliedCondition {
    /// `EXISTS` の SQL。
    sql: String,
    /// `EXISTS` に渡すバインド値。
    bindings: Vec<SqlValue>,
}

fn applied_exists(user_id: &str, operation_id: &str) -> AppliedCondition {
    AppliedCondition {
        sql: "EXISTS (SELECT 1 FROM applied_operations WHERE operation_id = ? AND user_id = ?)"
            .to_owned(),
        bindings: vec![
            SqlValue::Text(operation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    }
}

/// 段の文に付ける、トレーニングの行の条件。
///
/// 段の文はセットの行を結合しないため、トレーニングの ID を直接は確かめられない。
/// 編集権の条件だけを付け、操作 ID の保存でセットの適用の成功を確かめる。
fn ownership_condition_placeholder(
    user_id: &str,
    ownership: ExerciseOwnership<'_>,
) -> (String, Vec<SqlValue>) {
    match ownership {
        ExerciseOwnership::Terminal(terminal_id) => (
            "EXISTS (SELECT 1 FROM training_sessions ts WHERE ts.user_id = ? \
             AND (ts.finished_at IS NOT NULL OR ts.owner_terminal_id = ?))"
                .to_owned(),
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(terminal_id.to_owned()),
            ],
        ),
        ExerciseOwnership::Finished => (
            "EXISTS (SELECT 1 FROM training_sessions ts WHERE ts.user_id = ? \
             AND ts.finished_at IS NOT NULL)"
                .to_owned(),
            vec![SqlValue::Text(user_id.to_owned())],
        ),
    }
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

async fn set_row(
    db: &Db,
    user_id: &str,
    training_id: &str,
    set_id: &str,
) -> Result<Option<SetRow>> {
    db.first(&SqlStatement::new(
        "SELECT s.completed, s.training_exercise_id FROM sets s \
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

/// 種目の記録方式を返す。種目がトレーニングに無いときは None。
async fn exercise_mode(
    db: &Db,
    user_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Result<Option<RecordMode>> {
    let row = db
        .first::<ExerciseModeRow>(&SqlStatement::new(
            "SELECT snapshot FROM training_exercises \
             WHERE id = ? AND user_id = ? AND training_id = ?",
            vec![
                SqlValue::Text(training_exercise_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let snapshot = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot).map_err(|error| {
        worker::Error::RustError(format!("the exercise snapshot is invalid: {error}"))
    })?;
    Ok(Some(snapshot.record_mode))
}

/// 操作の対象の種目の記録方式を返す。段の値を作らない操作は None。
async fn payload_record_mode(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
) -> Result<Option<RecordMode>> {
    let training_id = &validated.operation.training_id;
    match &validated.payload {
        ValidatedPayload::SetCreate(payload) => {
            exercise_mode(db, user_id, training_id, &payload.training_exercise_id).await
        }
        ValidatedPayload::SetUpdate(payload) => {
            let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
                return Ok(None);
            };
            exercise_mode(db, user_id, training_id, &row.training_exercise_id).await
        }
        ValidatedPayload::SetComplete(payload) => {
            let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
                return Ok(None);
            };
            exercise_mode(db, user_id, training_id, &row.training_exercise_id).await
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_) => Ok(None),
    }
}

async fn set_owner_row(db: &Db, user_id: &str, set_id: &str) -> Result<Option<SetOwnerRow>> {
    db.first(&SqlStatement::new(
        "SELECT s.training_exercise_id, te.training_id FROM sets s \
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
    change: Option<SetChange<'_>>,
) -> Result<bool> {
    let mut rows = set_kind_rows(db, user_id, training_exercise_id).await?;
    if let Some(change) = change {
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
    }
    rows.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.id.cmp(&right.id))
    });
    let kinds = parse_kinds(rows.iter().map(|row| row.kind.as_str()))?;
    Ok(set_rules::drop_set_positions_valid(&kinds))
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

fn drop_position_rejection() -> Response {
    response(
        ErrorCode::BadRequest,
        "a drop set cannot be first or follow a warmup set",
    )
}

fn has_non_time_values(segments: &[SetSegmentPayload]) -> bool {
    segments.iter().any(|segment| {
        segment.weight_kg != 0.0
            || segment.reps != 0
            || segment.momentum_reps != 0
            || segment.assisted_reps != 0
            || [segment.left.as_ref(), segment.right.as_ref()]
                .into_iter()
                .flatten()
                .any(|side| side.weight_kg != 0.0 || side.reps != 0)
    })
}

fn time_value_rejection() -> Response {
    response(
        ErrorCode::BadRequest,
        "a time set cannot store weight, reps, momentum, or assisted reps",
    )
}

async fn completed_set_has_values(
    db: &Db,
    user_id: &str,
    set_id: &str,
    mode: RecordMode,
) -> Result<bool> {
    let sql = match mode {
        RecordMode::Time => {
            "SELECT COUNT(*) AS segments, \
             COALESCE(SUM(CASE WHEN \
              (left_weight_kg IS NULL AND left_reps IS NULL \
               AND left_duration_seconds IS NULL AND right_weight_kg IS NULL \
               AND right_reps IS NULL AND right_duration_seconds IS NULL) \
              AND weight_kg IS NULL AND reps IS NULL AND momentum_reps = 0 \
              AND assisted_reps = 0 AND duration_seconds > 0 THEN 1 \
              WHEN left_duration_seconds > 0 AND right_duration_seconds > 0 \
              AND left_weight_kg IS NULL AND left_reps IS NULL \
              AND right_weight_kg IS NULL AND right_reps IS NULL \
              AND weight_kg IS NULL AND reps IS NULL AND duration_seconds IS NULL \
              AND momentum_reps = 0 AND assisted_reps = 0 THEN 1 \
              ELSE 0 END), 0) AS ready \
             FROM set_segments WHERE set_id = ? AND user_id = ?"
        }
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
            "SELECT COUNT(*) AS segments, \
             COALESCE(SUM(CASE WHEN \
              (left_weight_kg IS NULL AND left_reps IS NULL \
               AND left_duration_seconds IS NULL AND right_weight_kg IS NULL \
               AND right_reps IS NULL AND right_duration_seconds IS NULL) \
              AND reps > 0 THEN 1 \
              WHEN left_reps > 0 AND right_reps > 0 THEN 1 \
              ELSE 0 END), 0) AS ready \
             FROM set_segments WHERE set_id = ? AND user_id = ?"
        }
    };
    let row = db
        .first::<SetValueRow>(&SqlStatement::new(
            sql,
            vec![
                SqlValue::Text(set_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?;
    Ok(row.is_some_and(|row| row.segments > 0 && row.ready == row.segments))
}

fn segments_are_complete(segments: &[SetSegmentPayload], mode: RecordMode) -> bool {
    !segments.is_empty()
        && segments.iter().all(|segment| match mode {
            RecordMode::Time => {
                segment
                    .duration_seconds
                    .is_some_and(|duration| duration > 0)
                    || (segment
                        .left
                        .as_ref()
                        .and_then(|side| side.duration_seconds)
                        .is_some_and(|duration| duration > 0)
                        && segment
                            .right
                            .as_ref()
                            .and_then(|side| side.duration_seconds)
                            .is_some_and(|duration| duration > 0))
            }
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                segment.reps > 0
                    || (segment.left.as_ref().is_some_and(|side| side.reps > 0)
                        && segment.right.as_ref().is_some_and(|side| side.reps > 0))
            }
        })
}

fn same_set_ids(current: &[String], ordered: &[String]) -> bool {
    let mut current = current.to_vec();
    let mut ordered = ordered.to_vec();
    current.sort_unstable();
    ordered.sort_unstable();
    current == ordered
}

fn grip_json(grip: Option<GripSelection>) -> Option<SqlValue> {
    match grip {
        None => Some(SqlValue::Null),
        Some(grip) => serde_json::to_string(&grip).ok().map(SqlValue::Text),
    }
}

fn optional_real(value: Option<f64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Real)
}

fn optional_integer(value: Option<i64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Integer)
}

#[cfg(test)]
mod tests;
