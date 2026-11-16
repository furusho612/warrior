# AI モデルの暫定既定値を決める

- Requirements: なし (ADR-018 のモデルの選定であり、PRD の要求 ID を持たない)
- Priority: Medium
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-ai-model-selection
- Completed: 2026-11-17

## 目的

Workers AI の無料枠で動くモデルのうち、第一候補の Llama 4 Scout を `AI_MODEL` の暫定既定値に置く。
候補と無料枠の条件、品質評価の手順を用意する。

## 優先度根拠

AI の評価と提案の品質は、モデルの日本語の能力と指示の遵守に依存する。
暫定既定値がないと、0055 以降の AI の生成を起動できない。
無料枠を超えると生成が失敗するため、1 日あたりの Neurons も見積もる。

## 現状

0014 でプロンプトと出力の検証、0055 で Workers AI の呼び出し、0056 から 0060 で評価と提案を実装する。
しかし、`AI_MODEL` に置く暫定既定値が決まっておらず、無料枠の消費も見積もっていない。

## 設計方針

`AI_MODEL` の暫定既定値を `@cf/meta/llama-4-scout-17b-16e-instruct` とする。
このモデルは、131,000 トークンの文脈、関数呼び出し、ゼロデータ保持に対応し、ADR-018 の候補でもある。

無料枠は 1 日 10,000 Neurons で、毎日 00:00 UTC にリセットされる。
代表的なリクエストを入力 8,000 トークン、出力 1,200 トークンとすると、Llama 4 Scout は 1 リクエストあたり約 289 Neurons であり、1 日 20 回で約 5,782 Neurons になる。
この見積もりが 10,000 Neurons 以下であることを確かめる。
実際の記録で測った最大の入力と出力がこれを超える場合は、0069 で再計算する。

候補は次のとおりとする。

- `@cf/meta/llama-4-scout-17b-16e-instruct` (暫定既定値)
- `@cf/openai/gpt-oss-20b`
- `@cf/google/gemma-3-12b-it`
- `@cf/meta/llama-3.3-70b-instruct-fp8-fast`
- `@cf/deepseek-ai/deepseek-v4.1-flash-1017`

`@cf/qwen/qwen3-30b-a3b-fp8` は 2026-11-20 に廃止予定のため、候補から外す。

候補は、Workers AI の公式のモデルカタログと料金ページに載っており、Cloudflare の「Your Data and Workers AI」の条件で、学習に使わないこととゼロデータ保持を確認できるものに限る。
確認できないモデルは候補から外す。

`wrangler.toml` の production の `AI_MODEL` に暫定既定値を設定する。
staging を追加したときは、production と同じ値を staging の `AI_MODEL` に置く。
外部の AI サービスは使わない。ADR-018 は外部の提供者へのフォールバックを含まない。

品質評価は 0069 で行う。
この issue では、0069 が使う評価の記録の fixture と、評価のプロンプトを出力する `mise` のタスクを用意する。
評価の結果で暫定既定値を確定する。合格しない場合は、0069 が `AI_MODEL` を更新する。

## 完了条件

- [ ] `wrangler.toml` の production の `AI_MODEL` が暫定既定値 `@cf/meta/llama-4-scout-17b-16e-instruct` である。
- [ ] 候補のモデル、無料枠の 10,000 Neurons、1 日 20 回の見積もりが issue または ADR に記録されている。
- [ ] 候補が Workers AI の公式のモデルカタログと料金ページに載っている。
- [ ] 候補が Cloudflare のデータ使用の条件で、学習に使わないこととゼロデータ保持を確認できる。
- [ ] `@cf/qwen/qwen3-30b-a3b-fp8` を、2026-11-20 の廃止予定を理由に候補から外す。
- [ ] 0069 が使う評価の記録の fixture と、評価のプロンプトを出力する `mise` のタスクがある。
- [ ] `AI_MODEL` の設定を確かめるテストがある。
- [ ] 外部の AI サービスを使わない。
- [ ] 設定の変更であるため、PBT は使わない。

## 解決方法

`wrangler.toml` の `[env.production]` の vars の `AI_MODEL` を、暫定既定値 `@cf/meta/llama-4-scout-17b-16e-instruct` にする。
`mise.toml` に `ai-eval` タスクを追加する。このタスクは、評価の記録の fixture を読み、PRD のプロンプトを出力する。外部の AI は呼ばない。
`backend/warrior/tests/ai_model_config.rs` に `AI_MODEL` の設定を確かめるテストを置く。
`backend/warrior/tests/fixtures/ai_eval/` に、目的 5 つ、レベル 3 つ、評価の日、週、月、次回セットの提案、メニューの最適化、目標重量の達成が難しい場合、痛みの記述がある場合の記録を置く。
ADR-018 に、暫定既定値と候補、無料枠の条件を追記する。

## 結果

production と staging の `AI_MODEL` を `@cf/meta/llama-4-scout-17b-16e-instruct` に揃えた。
`backend/warrior/tests/fixtures/ai_eval/` に目的、レベル、評価の期間、提案、メニュー、目標重量、痛みの 8 件の記録を置いた。
`mise run ai-eval` で fixture と評価用の入力を出力する。`backend/warrior/tests/ai_model_config.rs` で設定と fixture の JSON を確認した。
外部の AI は呼ばない。
