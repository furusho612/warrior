# set_calc の module を分割する

- Requirements: なし (内部のリファクタ。公開 API と振る舞いを変えない)
- Priority: Low
- Created: 2026-11-14
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-split-set-calc

## 目的

`backend/warrior_core/src/set_calc.rs` が 1200 行を超え、セットの型、検証、計算、自己ベストの判定が 1 つの module に集中している。
公開 API と振る舞いを変えずに module を分割し、変更の影響範囲を読み取れる状態にする。

## 優先度根拠

現在の機能追加は `sync_set.rs`、`set.rs`、画面側に分散しており、`set_calc.rs` の分割だけを理由に機能を止める必要はない。
ただし、今後の計算規則の追加は同じファイルへ集中するため、早い段階で境界を決める。

## 現状

0028 の時点で `backend/warrior_core/src/set_calc.rs` は 1202 行である。
0028 は `0026` と `0027` の共有ファイルを追加行に留める制約があったため、分割しなかった。
計算規則、入力の検証、自己ベストの判定が同じファイルにあり、変更時に読む範囲が広い。

## 設計方針

`backend/warrior_core/src/set_calc.rs` を `backend/warrior_core/src/set_calc/` 配下の module に分ける。
分割の候補は、セットの型、入力の検証、推定 1RM とストリクト回数、自己ベストの判定である。
`set_calc` の公開 API は維持し、呼び出し側の import を変えない。
振る舞いを変えないため、分割の前後で既存の単体テストと PBT をそのまま通す。
公開 API の変更が必要になった場合は、この issue ではなく新しい issue を起こす。

## 完了条件

- [ ] `backend/warrior_core/src/set_calc.rs` が 1000 行未満になる。これは内部のリファクタであり、要求 ID を持たない。
- [ ] `set_calc` の公開 API と振る舞いが変わらない。既存の単体テストと PBT がすべて通る。
- [ ] 分割後の module の責務が、型、検証、計算、自己ベストのいずれか 1 つに閉じる。
- [ ] 呼び出し側の import とテストの参照が、分割前と同じ `warrior_core::set_calc::...` で通る。

## 解決方法

`backend/warrior_core/src/set_calc.rs` を `backend/warrior_core/src/set_calc/` の module に分ける。
`backend/warrior_core/src/set_calc.rs` は公開 API の re-export と module 宣言だけを持つ。
`backend/warrior_core/src/lib.rs` の `pub mod set_calc;` は変えない。
既存の `backend/pbt/tests/set_calc.rs`、`backend/pbt/tests/reps_rpe.rs`、`backend/pbt/tests/set.rs` を分割後も実行する。
