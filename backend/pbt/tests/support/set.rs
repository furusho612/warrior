//! セットの計算の性質テストで共有する入力の生成。

use proptest::prelude::*;

/// セットの重量 (kg)。
pub fn weight() -> impl Strategy<Value = f64> {
    0.5f64..=500.0
}

/// 回数、反動の回数、補助の回数の組。合計が回数を超えない。
pub fn valid_breakdown() -> impl Strategy<Value = (u32, u32, u32)> {
    (1u32..=30).prop_flat_map(|reps| {
        (0..=reps).prop_flat_map(move |momentum| {
            (0..=(reps - momentum)).prop_map(move |assisted| (reps, momentum, assisted))
        })
    })
}
