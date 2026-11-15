//! セットの追加、更新、削除、並べ替え、完了の操作の適用。
//!
//! 操作は端末の送信待ちの列から届き、`backend/warrior/src/sync.rs` が編集権を検査する。
//! このモジュールは、同じ編集権の `EXISTS` 条件を各データ文に付け、セットの適用と
//! 操作 ID の保存を同じ `batch()` に入れる (ADR-002、ADR-003)。
//! データ文が 0 行のときは操作 ID を残さず、段の文は保存した操作 ID があるときだけ動く。
//! 段の重量は、自重種目ではセットごとの加重を表す。記録した時点の体重は `sets` が持つ。
//! 完了の操作の後には、自己ベストを判定し、更新した項目を応答へ返す。

use serde::Deserialize;
use warrior_core::exercise::{is_lateral, GripMode, GripSelection, Laterality, RecordMode};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    SetCompletePayload, SetCreatePayload, SetDeletePayload, SetReorderPayload, SetUpdatePayload,
    ValidatedOperation, ValidatedPayload,
};
use worker::Result;

use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::sync::{
    applied_statement, owner_condition, ownership_guard_statement, session_row, write_decision,
    BatchOutcome, OwnerCondition,
};
use crate::training_session_exercise::{ownership_condition, ExerciseOwnership};

mod bindings;
mod completion;
mod precheck;
mod validation;

use bindings::{applied_exists, grip_json, optional_real};
pub(crate) use completion::{personal_bests_after_completion, personal_bests_after_operation};
use precheck::precheck;
use validation::grip_rejection;

/// 種目の記録方式、左右の指定、グリップの選択肢。
#[derive(Debug, Clone, Deserialize)]
struct ExerciseRules {
    /// 左右別に行うか。
    laterality: Laterality,
    /// 記録方式。
    record_mode: RecordMode,
    /// グリップの指定の種類。
    grip_mode: GripMode,
    /// 選べるグリップ。グリップを選ばない種目では空である。
    grip_options: Vec<GripSelection>,
}

impl ExerciseRules {
    /// 左右の列を使う種目か。
    fn lateral(&self) -> bool {
        is_lateral(self.laterality, self.record_mode)
    }
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

    if let Some(rejection) = precheck(db, user_id, validated, rules.as_ref(), finished).await? {
        return Ok(BatchOutcome::Reject(rejection));
    }
    if let Some(rules) = rules.as_ref() {
        if let Some(rejection) = grip_rejection(&validated.payload, rules) {
            return Ok(BatchOutcome::Reject(rejection));
        }
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
        rules.as_ref().map(|rules| rules.record_mode),
        rules.as_ref().is_some_and(|rules| rules.lateral()),
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

fn set_statements(
    user_id: &str,
    validated: &ValidatedOperation,
    mode: Option<RecordMode>,
    lateral: bool,
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
                lateral,
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
                lateral,
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
                lateral: false,
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
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
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
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::Delete(_)
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

#[cfg(test)]
mod tests;
