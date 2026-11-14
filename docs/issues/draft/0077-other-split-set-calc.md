# set_calc.rs を責務ごとの module に分割する

- Requirements: なし (挙動を変えないリファクタ)
- Priority: Low
- Created: 2026-11-14
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-split-set-calc

## 目的

`backend/warrior_core/src/set_calc.rs` を責務ごとの module に分け、ファイル長を code-style の閾値である 1000 行以下にする。
公開する型と関数のパスは変えず、計算の挙動も変えない。

## 優先度根拠

計算は正しく、テストも通過している。記録と分析の機能追加を止めないため、優先度を Low とする。

## 現状

`backend/warrior_core/src/set_calc.rs` は 1168 行である。
0026 のドロップセットを取り込むと 1202 行になる。
0027 で左右別の値の総重量と検証、その例示テストが加わり、分割前の 1099 行から増えた。
1 つのファイルに、セットの種別と記録方式、段の値、セットの記録と検証、推定 1RM と自己ベスト、約 480 行の例示テストが混在する。
変更のたびに無関係な節を読み直す必要があり、関数や型の追加でファイルがさらに育つ。

## 設計方針

`backend/warrior_core/src/set_calc/` を作り、公開 API は `mod.rs` の `pub use` で今のパスを保つ。

- `mod.rs`: module doc、公開型と関数の再公開、共通の列挙と値
- `record.rs`: `SetSegment`、`SetRecord`、記録の検証、RPE と時間の検証。対応する例示テストも置く
- `best.rs`: `OneRepMax`、`BestKey`、`BestValue`、`BestUpdates`、`PersonalBest`、推定 1RM と自己ベストの計算。対応する例示テストも置く
- `kind.rs`: `SetKind`、`Side`、`RecordMode`、`SideValue`、`PartRole`

計算規則と検証規則は移さない。テストは対象の module の近くへ移し、PBT の `backend/pbt/tests/set.rs` は変更しない。
公開パスを保つため、`warrior_core::set_calc::SetRecord` などの利用側は変更しない。

## 完了条件

- [ ] `backend/warrior_core/src/set_calc.rs` がなくなり、分割後の各ファイルが 1000 行以下になる。これは要求に対応しない内部リファクタである。
- [ ] `warrior_core::set_calc` の公開する型と関数のパスが変わらない。
- [ ] 既存の例示テストと PBT が同じ内容で通過する。
- [ ] `cargo test -p warrior_core`、`cargo test -p warrior_pbt`、`cargo clippy --workspace --all-targets -- -D warnings`、`mise run fmt`、`mise run lint` が通過する。
- [ ] 計算結果、検証の成否、エラーの値が分割前に一致する。

## 解決方法

`git mv backend/warrior_core/src/set_calc.rs backend/warrior_core/src/set_calc/mod.rs` で移動し、責務ごとの module を追加する。
`mod.rs` から `pub use` して今の公開パスを保つ。
関数とテストは意味を変えずに移動するだけにし、整形以外の差分を出さない。
0026 のドロップセットと 0027 の左右別の値の追加分もこの issue の範囲に含め、同じ方針で移す。
