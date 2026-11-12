# Fuzzing の対象と corpus を整備する

- Requirements: なし (Fuzzing の整備は ADR-024 の判断であり、PRD の要求 ID を持たない)
- Priority: Medium
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-fuzzing-harness

## 目的

任意のバイト列と任意のイベント列をパーサに与え、パニックとタイムアウトが起きないことを確かめる。
Fuzzing の対象と corpus をリポジトリに置き、nightly の CI で実行する。
Fuzzing はクラッシュ耐性だけを担い、性質の検証は PBT に置く。

## 優先度根拠

Fuzzing の対象は、認証、同期、AI、CSV、プッシュのパーサである。
パーサが実装された後に作るため、後半の issue に置く。

## 現状

0001 でワークスペース、0014 で AI の出力の検証、0016 で WebAuthn のパーサ、0021 で同期の操作、0036 と 0066 でプッシュ通知の payload、0064 で CSV を実装する。
しかし、`backend/fuzz` のクレート、Fuzzing の対象、corpus、nightly の CI がない。

## 設計方針

`backend/fuzz` を Cargo のワークスペースに追加し、`cargo-fuzz` で実行する。
`cargo-fuzz` は ADR-022 の 0.13.2 とする。

対象は次のとおりとする。

- WebAuthn の CBOR、COSE、clientDataJSON、attestation object、authenticator data。
- Google の ID トークンのクレームのパーサ。
- AI の出力の JSON とテキスト。
- 同期の操作の JSON。
- CSV のセル。
- プッシュ通知の payload。
- Web Push の購読の鍵。

corpus はリポジトリに含め、手元の実行で更新する。
Fuzzing の実行は nightly の CI で行い、corpus をジョブのキャッシュに保存する。
PR の CI では、Fuzzing の対象の型検査だけを行う。
`mise run fuzz` に時間の上限を置く。
見つかったクラッシュは、Example-based test に回帰ケースとして加える。

Fuzzing は、パニックとタイムアウトが起きないことだけを確かめる。
値の正しさ、不変条件、往復の性質は PBT が担う。
同じ性質を Fuzzing と PBT の両方で確かめない。

## 完了条件

- [ ] `backend/fuzz` が Cargo のワークスペースのメンバーであり、`cargo fuzz build` が成功する。
- [ ] 上の 7 つの対象の Fuzzing のターゲットがある。
- [ ] corpus がリポジトリに含まれ、手元の実行で更新できる。
- [ ] `mise run fuzz` が時間の上限を持ち、Fuzzing を実行する。
- [ ] PR の CI が Fuzzing の対象の型検査を行い、nightly の CI が Fuzzing を実行する。
- [ ] 見つかったクラッシュが、Example-based test の回帰ケースとして追加される。
- [ ] Fuzzing がクラッシュ耐性だけを確かめ、性質の検証を PBT に置く。
- [ ] `mise run check` が Fuzzing の型検査を含む。
- [ ] 例示テストで、Fuzzing の対象のパーサが、既知の corpus をパニックせずに処理することを確かめる。
- [ ] Fuzzing 自体の性質テストは置かない。Fuzzing は任意入力のクラッシュ耐性を担う。

## 解決方法

`backend/fuzz/Cargo.toml` と `backend/fuzz/fuzz_targets/` に、7 つの対象のターゲットを置く。
`backend/fuzz/corpus/` に corpus を置く。
`mise.toml` に `fuzz` タスクを追加し、`check` に Fuzzing の型検査を追加する。
`.github/workflows/ci.yml` に nightly の Fuzzing のジョブを追加する。
見つかったクラッシュの回帰ケースは、対象のパーサの単体テストに追加する。
