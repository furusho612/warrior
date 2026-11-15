//! セットの操作の SQL に渡すバインド値と、操作 ID の条件。
//!
//! `backend/warrior/src/set.rs` のデータ文が使う。段の文はセットの行を結合しないため、
//! トレーニングの ID を直接は確かめられない。編集権の条件だけを付け、操作 ID の保存で
//! セットの適用の成功を確かめる。

use warrior_core::exercise::GripSelection;
use warrior_core::set_calc::RecordMode;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::SetSegmentPayload;

use crate::training_session_exercise::{ownership_condition, ExerciseOwnership};

pub(super) fn segment_bindings(
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
        optional_real(if is_time {
            (segment.weight_kg != 0.0).then_some(segment.weight_kg)
        } else {
            Some(segment.weight_kg)
        }),
        optional_integer(if is_time {
            (segment.reps != 0).then(|| i64::from(segment.reps))
        } else {
            Some(i64::from(segment.reps))
        }),
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
        bindings.push(optional_real(if is_time {
            None
        } else {
            side.and_then(|side| side.weight_kg)
        }));
        bindings.push(optional_integer(if is_time {
            None
        } else {
            side.map(|side| i64::from(side.reps))
        }));
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
pub(super) struct AppliedCondition {
    /// `EXISTS` の SQL。
    pub(super) sql: String,
    /// `EXISTS` に渡すバインド値。
    pub(super) bindings: Vec<SqlValue>,
}

pub(super) fn applied_exists(user_id: &str, operation_id: &str) -> AppliedCondition {
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
pub(super) fn ownership_condition_placeholder(
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

pub(super) fn grip_json(grip: Option<GripSelection>) -> Option<SqlValue> {
    match grip {
        None => Some(SqlValue::Null),
        Some(grip) => serde_json::to_string(&grip).ok().map(SqlValue::Text),
    }
}

pub(super) fn optional_real(value: Option<f64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Real)
}

pub(super) fn optional_integer(value: Option<i64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Integer)
}

/// 段の文を作るための、対象と条件の組。
pub(super) struct SegmentStatements<'a> {
    pub(super) user_id: &'a str,
    pub(super) training_id: &'a str,
    pub(super) set_id: &'a str,
    pub(super) mode: RecordMode,
    pub(super) ownership: ExerciseOwnership<'a>,
    pub(super) applied: &'a AppliedCondition,
    pub(super) now: &'a str,
}

pub(super) fn segment_replace_statements(
    context: &SegmentStatements<'_>,
    segments: &[SetSegmentPayload],
) -> Vec<SqlStatement> {
    let mut statements = vec![delete_segments_statement(context)];
    for segment in segments {
        statements.push(segment_insert_statement(context, segment));
    }
    statements
}

fn delete_segments_statement(context: &SegmentStatements<'_>) -> SqlStatement {
    let (training_condition, training_bindings) =
        ownership_condition_placeholder(context.user_id, context.ownership);
    let mut bindings = vec![
        SqlValue::Text(context.set_id.to_owned()),
        SqlValue::Text(context.user_id.to_owned()),
    ];
    bindings.extend(training_bindings);
    bindings.extend(context.applied.bindings.iter().cloned());
    SqlStatement::new(
        format!(
            "DELETE FROM set_segments WHERE set_id = ? AND user_id = ? \
             AND {training_condition} AND {}",
            context.applied.sql
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
