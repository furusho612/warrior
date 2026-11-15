//! セットの追加、更新、削除、並べ替え、完了の操作の適用。
//!
//! 操作は端末の送信待ちの列から届き、`backend/warrior/src/sync.rs` が編集権を検査する。
//! このモジュールは、同じ編集権の `EXISTS` 条件を各データ文に付け、セットの適用と
//! 操作 ID の保存を同じ `batch()` に入れる (ADR-002、ADR-003)。
//! データ文が 0 行のときは操作 ID を残さず、段の文は保存した操作 ID があるときだけ動く。
//! 段の重量は、自重種目ではセットごとの加重を表す。記録した時点の体重は `sets` が持つ。

use serde::Deserialize;
use warrior_core::exercise::{is_lateral, ExerciseSnapshot, Laterality, RecordMode};
use warrior_core::set_calc::SetKind;
use warrior_core::set_rules;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    SetCompletePayload, SetCreatePayload, SetDeletePayload, SetReorderPayload, SetUpdatePayload,
    ValidatedOperation, ValidatedPayload,
};
use worker::{Response, Result};

use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::sync::{
    applied_statement, owner_condition, ownership_guard_statement, session_row, write_decision,
    BatchOutcome, OwnerCondition,
};
use crate::training_session_exercise::{ownership_condition, ExerciseOwnership};

mod bindings;
mod validation;

use bindings::{applied_exists, grip_json, optional_real};
use validation::{completed_set_has_values, completed_set_is_valid};

/// 種目の記録方式と左右の指定。
#[derive(Debug, Clone, Copy, Deserialize)]
struct ExerciseRules {
    /// 左右別に行うか。
    laterality: Laterality,
    /// 記録方式。
    record_mode: RecordMode,
}

impl ExerciseRules {
    /// 左右の列を使う種目か。
    fn lateral(self) -> bool {
        is_lateral(self.laterality, self.record_mode)
    }
}

/// トレーニングに属するセットの状態。
#[derive(Debug, Deserialize)]
struct SetRow {
    /// 完了したか。0 か 1 である。
    completed: i64,
    /// セットを持つ種目の行の ID。
    training_exercise_id: String,
    /// 記録した時点の種目の内容。
    snapshot: String,
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

/// 段の件数。
#[derive(Debug, Deserialize)]
struct CountRow {
    /// 件数。
    count: i64,
}

/// 種目のスナップショット。
#[derive(Debug, Deserialize)]
struct SnapshotRow {
    /// 記録した時点の種目の JSON。
    snapshot: String,
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

    let rules = payload_rules(
        db,
        user_id,
        &validated.operation.training_id,
        &validated.payload,
    )
    .await?;
    if matches!(
        validated.payload,
        ValidatedPayload::SetCreate(_)
            | ValidatedPayload::SetUpdate(_)
            | ValidatedPayload::SetComplete(_)
    ) && rules.is_none()
    {
        return Ok(BatchOutcome::Reject(response(
            ErrorCode::NotFound,
            "resource not found",
        )));
    }

    if let Some(rejection) = precheck(db, user_id, validated, rules).await? {
        return Ok(BatchOutcome::Reject(rejection));
    }
    let ownership = match owner {
        OwnerCondition::Claim(terminal_id) | OwnerCondition::Owned(terminal_id) => {
            ExerciseOwnership::Terminal(terminal_id)
        }
        OwnerCondition::Finished => ExerciseOwnership::Finished,
    };
    let Some(data) = set_statements(
        user_id,
        validated,
        rules.map(|rules| rules.record_mode),
        ownership,
        now,
    ) else {
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
    rules: Option<ExerciseRules>,
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
            precheck_update(db, user_id, training_id, payload, rules).await
        }
        ValidatedPayload::SetDelete(payload) => {
            precheck_delete(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetReorder(payload) => {
            precheck_reorder(db, user_id, training_id, payload).await
        }
        ValidatedPayload::SetComplete(payload) => {
            let Some(rules) = rules else {
                return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
            };
            precheck_complete(db, user_id, training_id, payload, rules).await
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
    rules: ExerciseRules,
) -> Result<Option<Response>> {
    if let Some(rejection) = validation::segments_shape_rejection(&payload.segments, rules) {
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
    rules: ExerciseRules,
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
    if let Some(rejection) = validation::segments_shape_rejection(&payload.segments, rules) {
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
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
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
    _rules: ExerciseRules,
) -> Result<Option<Response>> {
    let Some(row) = set_row(db, user_id, training_id, &payload.set_id).await? else {
        return Ok(Some(response(ErrorCode::NotFound, "resource not found")));
    };
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
            let context = bindings::SegmentStatements {
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
                children: bindings::segment_replace_statements(&context, &payload.segments),
            })
        }
        ValidatedPayload::SetUpdate(payload) => {
            let mode = mode?;
            let context = bindings::SegmentStatements {
                user_id,
                training_id,
                set_id: &payload.set_id,
                mode,
                ownership,
                applied: &applied,
                now,
            };
            Some(SetData {
                main: update_set_statement(user_id, training_id, payload, ownership, now),
                children: bindings::segment_replace_statements(&context, &payload.segments),
            })
        }
        ValidatedPayload::SetDelete(payload) => {
            let context = bindings::SegmentStatements {
                user_id,
                training_id,
                set_id: &payload.set_id,
                mode: RecordMode::WeightAndReps,
                ownership,
                applied: &applied,
                now,
            };
            Some(SetData {
                main: delete_set_statement(user_id, training_id, payload, ownership),
                children: bindings::segment_replace_statements(&context, &[]),
            })
        }
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
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> SqlStatement {
    let (condition, mut condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let completed = payload.completed;
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

fn parse_rules(snapshot: &str) -> Result<ExerciseRules> {
    serde_json::from_str(snapshot).map_err(|error| {
        worker::Error::RustError(format!("the stored exercise snapshot is invalid: {error}"))
    })
}

/// 操作の対象の種目の記録方式と左右の指定を返す。
async fn payload_rules(
    db: &Db,
    user_id: &str,
    training_id: &str,
    payload: &ValidatedPayload,
) -> Result<Option<ExerciseRules>> {
    match payload {
        ValidatedPayload::SetCreate(payload) => {
            exercise_rules(db, user_id, training_id, &payload.training_exercise_id).await
        }
        ValidatedPayload::SetUpdate(payload) => {
            set_rules_for_set(db, user_id, training_id, &payload.set_id).await
        }
        ValidatedPayload::SetComplete(payload) => {
            set_rules_for_set(db, user_id, training_id, &payload.set_id).await
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseDelete(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SetDelete(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_) => Ok(None),
    }
}

async fn exercise_rules(
    db: &Db,
    user_id: &str,
    training_id: &str,
    training_exercise_id: &str,
) -> Result<Option<ExerciseRules>> {
    let row = db
        .first::<SnapshotRow>(&SqlStatement::new(
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
    parse_rules(&row.snapshot).map(Some)
}

async fn set_rules_for_set(
    db: &Db,
    user_id: &str,
    training_id: &str,
    set_id: &str,
) -> Result<Option<ExerciseRules>> {
    let row = db
        .first::<SnapshotRow>(&SqlStatement::new(
            "SELECT te.snapshot FROM sets s \
             JOIN training_exercises te ON te.id = s.training_exercise_id \
             WHERE s.id = ? AND s.user_id = ? AND te.training_id = ? AND te.user_id = ?",
            vec![
                SqlValue::Text(set_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    parse_rules(&row.snapshot).map(Some)
}

fn drop_position_rejection() -> Response {
    response(
        ErrorCode::BadRequest,
        "a drop set cannot be first or follow a warmup set",
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

fn same_set_ids(current: &[String], ordered: &[String]) -> bool {
    let mut current = current.to_vec();
    let mut ordered = ordered.to_vec();
    current.sort_unstable();
    ordered.sort_unstable();
    current == ordered
}

#[cfg(test)]
mod tests;
