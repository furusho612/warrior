# ADR-014: バックエンドを Rust と workers-rs で Cloudflare Workers に実装する

- Status: Accepted

## Context

アプリは Web ブラウザだけで動作し、Cloudflare Workers、D1、R2、Workers AI を使う。
参照する `../../../brewbook` は、同じ構成を Rust の `worker` クレートで実装し、D1、R2、Workers AI をバインディングから使っている。
バックエンドには、D1 への読み書き、R2 への大きなデータの保存、Workers AI の呼び出し、Queues による遅延処理、Cron Triggers による定期処理、Web Push、Google OAuth、パスキーの検証が必要である。
Cloudflare Workers の Rust は `wasm32-unknown-unknown` にビルドし、JavaScript のランタイム API を `wasm-bindgen` 経由で呼ぶ。

## Decision

バックエンドは Rust で書き、`worker` クレート 0.8.6 と `worker-build` で `wasm32-unknown-unknown` 向けにビルドする。
HTTP のルーティングは `worker` クレートの `Router` を使い、axum などの Web フレームワークを追加しない。
D1 は `d1` feature、R2 は `Bucket`、Workers AI は `Ai`、Queues は `queue` feature、Cron Triggers は `event(scheduled)` で扱う。
JSON のシリアライズは serde と serde_json を使う。

Cargo のワークスペースを置き、Worker のクレート、バインディングに依存しない純粋なロジックのクレート、PBT のクレート、Fuzzing のクレートに分ける。
純粋なロジックには、入力の検証、SQL の組み立て、推定 1RM とカロリーの計算、WebAuthn の検証、AI のプロンプトの組み立てと出力の検証を含める。

Workers Logs と Workers Traces を有効にする。
リクエストごとに、経路名、メソッド、状態コード、処理時間、エラーコードを JSON で記録する。
身体情報、メールアドレス、メモ、AI の入出力、トークン、プッシュ購読の情報は記録しない。

## Consequences

Rust の標準的な非同期ランタイムである tokio は使えない。
非同期処理は、`worker` クレートが提供する JavaScript の Promise ベースのものに限られる。

バックエンドのコードは Cloudflare 固有のバインディング API に依存する。
移植性より依存の少なさを優先する。

ネイティブのデータベースドライバや OpenSSL に依存するクレートは使えない。
WebAuthn と Web Push の暗号は、wasm で動く純 Rust のクレートで実装する。

wasm に依存しない純粋なロジックは、ネイティブのターゲットで単体テスト、PBT、Fuzzing に掛ける。
バインディングに触る部分は、`wrangler dev` に対する結合テストで検証する。

Worker の wasm のサイズを計測し、`wasm-opt`、LTO、strip で削減する。
サイズが上限に近づいた場合は、依存の追加を止めて分割を検討する。

ログの保持期間と量は Workers Logs の設定に従う。
失敗したリクエストの調査に使う。

## References

- `../../../brewbook/docs/adr/0001-backend-rust-on-cloudflare-workers.md`
- `../../../brewbook/backend/brew_book/wrangler.toml`
- `docs/prd/warrior.md` の「非機能要件」
- Cloudflare Workers の Rust のドキュメント
