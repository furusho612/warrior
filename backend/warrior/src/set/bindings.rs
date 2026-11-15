//! セットの操作の SQL に渡すバインド値と、操作 ID の条件。
//!
//! `backend/warrior/src/set.rs` のデータ文が使う。段の文はセットの行を結合しないため、
//! トレーニングの ID を直接は確かめられない。編集権の条件だけを付け、操作 ID の保存で
//! セットの適用の成功を確かめる。

use warrior_core::exercise::GripSelection;
use warrior_core::sql::SqlValue;
use warrior_core::sync::SetSegmentPayload;

use crate::training_session_exercise::ExerciseOwnership;

pub(super) fn segment_bindings(
    user_id: &str,
    set_id: &str,
    segment: &SetSegmentPayload,
    now: &str,
) -> Vec<SqlValue> {
    let is_time = is_time_segment(segment);
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
        optional_integer(segment.duration_seconds.map(i64::from)),
    ];
    for side in [segment.left.as_ref(), segment.right.as_ref()] {
        let weight = if is_time {
            None
        } else {
            side.and_then(|side| side.weight_kg)
        };
        let reps = if is_time {
            None
        } else {
            side.map(|side| i64::from(side.reps))
        };
        bindings.push(optional_real(weight));
        bindings.push(optional_integer(reps));
        bindings.push(optional_integer(
            side.and_then(|side| side.duration_seconds).map(i64::from),
        ));
    }
    bindings.push(SqlValue::Text(now.to_owned()));
    bindings.push(SqlValue::Text(now.to_owned()));
    bindings
}

/// 時間の列を持つ段か。時間で記録する段は重量と回数を保存しない。
fn is_time_segment(segment: &SetSegmentPayload) -> bool {
    segment.duration_seconds.is_some()
        || segment
            .left
            .as_ref()
            .is_some_and(|side| side.duration_seconds.is_some())
        || segment
            .right
            .as_ref()
            .is_some_and(|side| side.duration_seconds.is_some())
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
