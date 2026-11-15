# 左右別のセットとスーパーセットを実装する

- Requirements: REQ-SET-004, REQ-SET-005
- Priority: High
- Created: 2026-11-12
- Completed: 2026-11-14
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-unilateral-superset

## 目的

左右別に行う種目で、左右それぞれの重量と回数を記録できるようにする。
複数の種目をスーパーセットとしてまとめ、まとめた最後の種目のセットを完了したときにレストタイマーを起動する。

## 優先度根拠

左右別の種目とスーパーセットは、セットの構造とタイマーの起動の条件を変える。
構造を後から変えると、セットの総重量とタイマーの起動の回数が変わる。

## 現状

0025 でセットの追加と削除、セット種別、0026 でドロップセットの段を実装する。
しかし、左右それぞれの値、スーパーセットの組、組の最後の種目でのタイマーの起動がない。

## 設計方針

左右別の種目は、1 つのセットに左右それぞれの重量と回数を持つ。
1 行を左右 2 つのセットに分けない。
1 セットの総重量は、左右の値を合算した値とする。
ドロップセットの段も、左右別の種目では左右それぞれの値を持つ。
左右別でない種目では、左右の値を持たない。

0030 で時間で記録する種目も左右別の対象になった。
この issue の `is_lateral` が時間種目を除外する前提は 0030 が変更し、`laterality=unilateral` の時間種目は左右それぞれの duration を保存する。

左右別の値は、セットの段の `left_weight`、`left_reps`、`right_weight`、`right_reps` に保存する。
記録画面では、左右の列を持つ表として表示する。
左右の入力のどちらかが空の場合は、セットを完了できない。

スーパーセットは、種目の列とは別に、種目の参照の組として持つ。
組は順序を持ち、最後の種目を組の最後とする。
レストタイマーは、組の最後の種目のセットを完了したときにだけ起動する。
組の途中の種目のセットを完了しても、タイマーは起動しない。
スーパーセットでない種目は、セットの完了のたびにタイマーを起動する。タイマーの実装は 0034 が担う。
この issue では、セットの完了のときに、タイマーを起動するかどうかのイベントを 1 回発行する。

種目を削除した場合は、組からその種目を除く。
組の種目が 1 つになった場合は、組を解散する。
種目を並べ替えた場合は、組の中の順序も並べ替える。
組の最後の種目が変わった場合は、タイマーを起動する種目も変わる。

組の種目は、同じトレーニングの種目であることを検証する。
組の種目が 2 つ未満の組を作らない。

## 完了条件

- [ ] 左右別に行う種目では、左右それぞれの重量と回数を入力して保存できる。1 セットの総重量は、左右の値を合算した値になる。これは REQ-SET-004 の受入基準に対応する。
- [ ] 重量と回数で記録する種目では、左右別のセットで左右のどちらか、または重量が空のセットを完了できない。
- [ ] 左右別のドロップセットの段が、左右それぞれの値を持つ。
- [ ] 複数の種目をスーパーセットとしてまとめられる。これは REQ-SET-005 の受入基準に対応する。
- [ ] まとめた種目の途中のセットを完了してもレストタイマーは起動せず、最後の種目のセットを完了したときに起動する。これは REQ-SET-005 の受入基準に対応する。
- [ ] スーパーセットでない種目は、セットの完了のたびにタイマーを起動する。
- [ ] 種目を削除すると組から除かれ、組の種目が 1 つになると解散する。
- [ ] 種目を並べ替えると組の中の順序も変わり、最後の種目が変わるとタイマーを起動する種目も変わる。
- [ ] 組の種目を並べ替えても、組の種目は変わらない。並べ替えで種目を追加、削除、移動できない。
- [ ] 組の種目が同じトレーニングの種目であることを検証し、組の種目が 2 つ未満の組を拒否する。
- [ ] 左右別の値と組が、端末内の保存と送信待ちの列を通る。
- [ ] PBT で、左右別のセットの総重量が左右の合算と一致し、スーパーセットのタイマーの起動が最後の種目の完了のときだけであることを確かめる。
- [ ] PBT で、種目の削除と並べ替えの後に、組の種目が同じトレーニングに属し、組の種目が 2 つ未満にならないことを確かめる。
- [ ] 例示テストで、左右別の種目、ドロップセットの左右、2 つの種目の組、3 つの種目の組、組の解散を確かめる。左右別の自重種目は加重が空でも完了できることを例示テストで確かめる。
- [ ] E2E で、左右別のセットを入力し、スーパーセットを組み、最後の種目の完了でタイマーが起動することを確かめる。

## 解決方法

`backend/warrior_core/src/superset.rs` に、組の型、最後の種目の判定、タイマーを起動するかの判定を置く。
`backend/warrior/src/set.rs` に左右別の値の保存を追加する。
`backend/warrior/src/superset.rs` に組の作成、削除、並べ替えを置く。
`frontend/src/screens/record.rs` に左右の列、組の表示、組の作成と解除を置く。
`backend/warrior/tests/e2e_record_unilateral_superset.rs` に E2E を追加する。
`backend/pbt/tests/superset.rs` に性質テストを置く。

実装では、計画に挙げたファイルに加えて次のファイルを変更した。

- `backend/warrior_core/src/sync.rs`、`backend/warrior_core/src/sync_set.rs`、`backend/warrior_core/src/sync_superset.rs`、`backend/warrior_core/src/superset.rs`: 左右の段の重量を任意にし、組の種目が 2 つ未満の組を拒否する。
- `backend/warrior/src/set.rs`、`backend/warrior/src/set/bindings.rs`: 種目の左右の指定と記録方式を読み、段の形と完了の値を保存の前に検証する。
- `backend/warrior/src/training_session_exercise.rs`: 種目の削除と並べ替えで組を追随させる。
- `frontend/src/screens/record/exercise_view.rs` (operations.rs、superset.rs、totals.rs、view.rs)、`exercises.rs`、`set_inputs.rs`、`set_table.rs`、`sets.rs`、`superset_panel.rs`: 左右の入力と表示、組の楽観更新と重複した作成の防止を置く。
- `frontend/src/storage/exercise.rs`、`set.rs`、`superset.rs`、`frontend/src/superset.rs`、`frontend/src/training_set.rs`: 左右の値と組の保存、復元、送信待ちの列への追加を置く。
- `backend/warrior/src/stored_sets.rs`、`backend/warrior_core/src/exercise.rs` の `is_lateral`、`backend/warrior/src/set/validation.rs`: 左右の段の読み書き、左右別かどうかの判定、完了したセットの値の検証を置く。
- `backend/warrior/src/set/tests.rs`、`backend/warrior/tests/wrangler_record_unilateral_superset.rs` (exercise.rs、superset.rs)、`backend/warrior/tests/support/` (record_unilateral_superset.rs)、`frontend/tests/storage_exercise.rs` (restore.rs、discard.rs)、`frontend/tests/storage_set.rs`、`frontend/tests/storage_superset.rs`、`frontend/tests/support/` (exercise_tree.rs): 左右の値、組の子操作、破棄の復元を確かめるテストと、E2E とブラウザテストの共通の準備を置く。
- `backend/warrior/src/lib.rs`、`backend/warrior_core/src/lib.rs`、`frontend/src/lib.rs`、`frontend/src/screens/record.rs`、`frontend/src/storage/mod.rs`: module を登録する。
- `backend/pbt/tests/set_calc.rs`、`backend/warrior/tests/e2e_calendar.rs`、`e2e_ownership.rs`、`e2e_record_drop_set.rs`、`e2e_record_exercise.rs`、`e2e_record_set.rs`、`backend/warrior/tests/support/browser.rs`、`backend/warrior/tests/support/mod.rs`: 要求 ID と共通のテスト支援を合わせる。
- `CHANGES.md`: 利用者向けの変更履歴を追加する。
- `mise.toml`: 結合テストと E2E の一覧へこの issue のテストを追加する。
- `backend/warrior_core/src/set_calc.rs`、`backend/warrior/src/sync.rs`、`backend/warrior/src/sync/exercise.rs`、`backend/warrior/tests/support/record_common.rs`、`frontend/src/screens/record/failed_operations.rs`、`frontend/src/screens/record/set_events.rs`、`frontend/src/text.rs`、`docs/issues/0030-feature-time-exercises.md`、`docs/issues/draft/0077-other-split-set-calc.md`: 左右の値の計算、同期のエラー変換、共通のテスト準備、破棄の復元、タイマーのイベント、文言、0030 の時間種目と 0077 の分割の追跡を合わせる。

計画のファイルを分けた理由は、`backend/warrior/src/set.rs` と `frontend/src/screens/record/sets.rs` が code-style の 1000 行に近づいたためである。`backend/warrior/src/set/bindings.rs` に SQL のバインド値と操作 ID の条件を、`frontend/src/screens/record/set_table.rs` にセットの表の描画を移した。

再レビューで、スーパーセットの並べ替えが組の種目を変えないことをサーバーで検証し、`superset_members` の `(user_id, training_exercise_id)` に一意索引を追加した (`migrations/0012_superset_member_unique.sql`)。main の 0025/0026 最終版を取り込み、0077 は main の 1 つに統合した。`set_calc` は 1168 行で、0077 の範囲に含める。

最終確認で、1000 行を超えたファイルを分割した。`frontend/src/screens/record/exercise_view.rs` はセットの操作 (`exercise_view/operations.rs`)、組の操作 (`exercise_view/superset.rs`)、集計 (`exercise_view/totals.rs`)、画面が読む状態 (`exercise_view/view.rs`) に分けた。`frontend/tests/storage_exercise.rs` は復元 (`storage_exercise/restore.rs`) と破棄 (`storage_exercise/discard.rs`) に分け、共通の準備を `frontend/tests/support/exercise_tree.rs` へ移した。`backend/warrior/tests/wrangler_record_unilateral_superset.rs` は種目 (`wrangler_record_unilateral_superset/exercise.rs`) とスーパーセット (`wrangler_record_unilateral_superset/superset.rs`) に分け、操作の組み立てを `backend/warrior/tests/support/record_unilateral_superset.rs` へ移した。`backend/warrior/src/set.rs` と `frontend/src/screens/record/sets.rs` も先に 1000 行の閾値へ達したため、`backend/warrior/src/set/bindings.rs`、`frontend/src/screens/record/sets/tests.rs` などへ分けた。`set_calc` は 1168 行で 1000 行を超えるため、分割は 0077 に送る。

完了条件の「重量が空」は、加重を任意とする自重種目の仕様と食い違っていた。0025 と REQ-PRF-005 の解釈は現行 (加重が空でも完了できる) を正とし、条件文を「重量と回数で記録する種目では」に改めた。左右別の自重種目で加重が空でも完了できることを、`wrangler_record_unilateral_superset` の例示テストで確かめる。
