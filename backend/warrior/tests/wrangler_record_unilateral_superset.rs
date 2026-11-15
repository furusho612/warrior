//! 左右別のセットとスーパーセットの結合テスト。
//!
//! 対応する要求: `REQ-SET-004`、`REQ-SET-005`。
//! 左右別のセットは `exercise.rs`、スーパーセットは `superset.rs` が持つ。

#[path = "wrangler_record_unilateral_superset/exercise.rs"]
mod exercise;
#[allow(dead_code)]
#[path = "support/record_set.rs"]
mod helpers;
#[path = "wrangler_record_unilateral_superset/superset.rs"]
mod superset;
mod support;

use support::DevServer;
