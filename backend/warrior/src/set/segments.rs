//! セットの段の SQL のバインド値。
//!
//! 段の保存は記録方式で列の意味が変わる。時間で記録する段は時間だけを持ち、
//! 重量と回数は NULL にする。重量で記録する段は時間を持たない。

use warrior_core::set_calc::RecordMode;
use warrior_core::sql::SqlValue;
use warrior_core::sync::SetSegmentPayload;

use super::{optional_integer, optional_real};

/// 段の INSERT と UPDATE に渡すバインド値を、保存する列の順に返す。
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
