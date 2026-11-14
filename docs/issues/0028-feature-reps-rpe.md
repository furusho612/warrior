# 反動と補助の回数、RPE を実装する

- Requirements: REQ-SET-006, REQ-SET-007, REQ-SET-008, REQ-SET-009
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-reps-rpe

## 目的

セットごとに、反動を使った回数と補助を受けた回数を記録できるようにする。
セットごとに RPE を 6 から 10 まで 0.5 刻みで記録できるようにする。
反動と補助の回数を、総重量と推定 1RM の計算に 0012 の規則で反映する。

## 優先度根拠

反動と補助の回数は、ストリクト回数と推定 1RM を変える。
RPE は AI の評価と次回セットの提案の入力になる。
どちらも記録のデータの形に関わるため、セットの機能の中で先に固定する。

## 現状

0025 でセットの重量と回数、0026 でドロップセットの段、0027 で左右別とスーパーセットを実装する。
しかし、反動と補助の回数、RPE の入力と保存がない。

## 設計方針

反動の回数と補助の回数は、セットの段ごとに持つ。
入力は任意とし、空の場合は 0 とする。
反動の回数と補助の回数の合計が回数を超えるセットを拒否する。
ストリクト回数は、回数から反動の回数と補助の回数を引いた値とし、0 を下回る場合は 0 とする。
ストリクト回数は推定 1RM と自己ベストの種目別の総重量に使う。
総重量は、反動の回数を含め、補助の回数を含めない。
左右別では、同じ反動・補助の回数を左右それぞれに適用する。
推定 1RM と自己ベストの計算規則は 0012 と 0029 が所有する。
この issue は、反動と補助の回数を入力したときに、その値が計算へ届くことを確かめる。

RPE はセットごとに持つ。
入力は任意とし、6.0 から 10.0 まで 0.5 刻みとする。
空のままでもセットを保存できる。
値は既存の `sets.rpe` の REAL に保存する。範囲と刻みは `migrations/0006_training_sessions.sql` の CHECK 制約と `warrior_core::rpe` の検証で確かめる。
0.5 刻みは f64 で正確に表せるため、整数への変換を持ち込まない。
画面には 6.0 から 10.0 の表記で出す。
ドロップセットの RPE は、段の行ではなく親セットの drawer に 1 つ持つ。デザインの D 行に欄を出さない規則と両立させる。

反動と補助の回数、RPE は、端末内の保存と送信待ちの列を通る。
セットの詳細の drawer に表示する。
種目別の履歴は、0049 がこれらの値を表示する。

## 完了条件

- [ ] セットごとに、反動を使った回数と補助を受けた回数を入力して保存できる。保存した値はセットの詳細に表示される。種目別の履歴は 0049 が表示する。これは REQ-SET-006 の受入基準に対応する。
- [ ] 反動の回数と補助の回数の合計が回数を超えるセットを拒否する。これは REQ-SET-006 の受入基準に対応する。
- [ ] ストリクト回数が、回数から反動と補助の回数を引いた値になり、0 以上で回数以下になる。これは REQ-SET-006 と REQ-SET-008 の受入基準のうち、総重量と推定 1RM の計算の部分に対応する。
- [ ] 総重量の回数が、回数から補助の回数を引いた値になり、反動の回数を含む。推定 1RM と自己ベストの種目別の総重量がストリクト回数を使う。これは REQ-SET-006、REQ-SET-008、REQ-SET-009 の受入基準を 0012 の計算で確かめる。
- [ ] セットごとに 6 から 10 まで 0.5 刻みで RPE を記録できる。RPE を空のままでもセットを保存できる。これは REQ-SET-007 の受入基準に対応する。
- [ ] 6.0 未満、10.0 を超える値、0.5 刻みでない値を拒否する。これは REQ-SET-007 の受入基準に対応する。
- [ ] RPE が `sets.rpe` の REAL に保存され、範囲と刻みが migration の CHECK 制約と画面の検証で保証され、画面に 6.0 から 10.0 の表記で出る。これは REQ-SET-007 の受入基準に対応する。
- [ ] ドロップセットの RPE が、段の行ではなく親セットに 1 つ保存される。これは REQ-SET-007 の受入基準に対応する。
- [ ] 反動と補助の回数、RPE が、端末内の保存と送信待ちの列を通る。これは REQ-SET-006 と REQ-SET-007 の受入基準に対応する。
- [ ] PBT で、任意の回数と反動と補助の回数に対して、ストリクト回数が 0 以上で回数以下であり、0012 の計算と一致することを確かめる。これは REQ-SET-006 の受入基準に対応する。
- [ ] 例示テストで、RPE の 6.0、6.5、10.0、空、5.5、10.5、0.25 刻みを確かめる。これは REQ-SET-007 の受入基準に対応する。
- [ ] 例示テストで、反動と補助の合計が回数と等しい場合と超える場合を確かめる。これは REQ-SET-006 の受入基準に対応する。
- [ ] E2E で、反動と補助の回数と RPE を入力し、保存と表示を確かめる。これは REQ-SET-006 と REQ-SET-007 の受入基準に対応する。

## 解決方法

既存の `migrations/0006_training_sessions.sql` の `set_segments.momentum_reps` と `assisted_reps` を使い、`sets.rpe` の REAL に RPE を保存する。`sets.rpe` の範囲と刻みを定める CHECK 制約は既にあるため、新しい migration は追加しない。
`backend/warrior_core/src/set_calc.rs` の `SetSegment` と `SetRecord` は既に値を持つため、検証とテストを追加する。
`backend/warrior_core/src/rpe.rs` に RPE の検証と表記を置く。
`backend/warrior/src/set.rs` は既存の列への保存を持つため、値の経路を変えない。
`frontend/src/screens/record/set_drawer.rs` のセット詳細の drawer に、反動と補助の回数、RPE の入力を置く。
`backend/warrior/tests/wrangler_record_reps_rpe.rs` に結合テストを追加する。
`backend/warrior/tests/e2e_record_reps_rpe.rs` に E2E を追加する。
`backend/pbt/tests/reps_rpe.rs` に性質テストを置く。

## 完了時の実装差分

- RPE は `sets.rpe` の REAL に保存した。0.5 刻みは f64 で正確に表せ、既存のスキーマと API の形に合わせられ、整数への変換を増やさないため、`rpe_tenths` は追加しなかった。
- `sets.rpe` の範囲と刻みは `migrations/0006_training_sessions.sql` の CHECK 制約が既に定めていたため、migration を追加しなかった。
- `set_segments.momentum_reps` と `assisted_reps` は `migrations/0006_training_sessions.sql` に既にあるため、migration を追加しなかった。
- `backend/warrior/src/set.rs` は既存の列への保存を持っていたため、変更しなかった。検証は `backend/warrior_core/src/sync_set.rs` と `backend/warrior_core/src/set_calc.rs` に置いた。
- セット詳細の drawer は `frontend/src/screens/record/set_drawer.rs` に置き、`frontend/src/screens/record/exercises.rs` から描画した。drawer は選択中の種目のセット状態を読むため、記録画面のトップレベルではなく種目の一覧の側に置いた。
- カレンダーのセットの注記は `warrior_core::rpe::format` を使い、`RPE 8.0` の表記に統一した。
- 端末内の保存では、操作の payload とセットの行の位置、種別、完了、RPE、グリップ、体重、段が一致することを確かめてから送信待ちの列へ積むようにした。
- セットの保存のエラーはセット ID ごとに保持し、保存に成功したセットのエラーだけを消すようにした。別のセットの保存でエラーが消えない。
- PBT の入力の生成は `backend/pbt/tests/support/set.rs` に集約し、`reps_rpe.rs` を REQ-SET-006 と REQ-SET-007 の所有者にした。
- `backend/warrior_core/src/set_calc.rs` は 1000 行を超えたままであり、分割は 0077 で追跡する。
- 左右別の段では、同じ反動と補助の回数を左右それぞれの回数に適用する。左 10kg×10、右 20kg×8、補助 1 の総重量は 230kg になる。検証は左右それぞれの回数に対して行う。
- セット詳細の drawer は、セットを選ぶまで表示せず、選択したセットだけを開く。E2E は drawer が開くまで存在しないことを確かめる。
- 端末側でも同じ規則で検証した。`frontend/src/training_set.rs` は送信前に RPE と回数の内訳を拒否し、`frontend/src/screens/record/sets.rs` は drawer の入力の誤りを検出し、記録画面がエラーを表示する。
- 種目別の履歴は 0049 が実装する。0028 はセット詳細の表示までを担う。
