# ローカル用の Worker 設定と環境ごとのバインディングを用意する

- Requirements: なし (環境分離は ADR-023 の判断であり、PRD の要求 ID を持たない。設定のキーを用意するだけで、上限の判定は 0018 が実装する)
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-local-worker-config

## 目的

ローカルの `wrangler dev` で Worker、D1、R2、Queues、Cron Triggers、Workers AI のバインディングを起動できるようにする。
ローカルと production の設定を同じ `wrangler.toml` に置き、環境の取り違えを防ぐ。
staging は必要になった時点で 0067 で追加する。

## 優先度根拠

すべてのサーバー側の issue が `wrangler dev` とバインディングに依存する。
環境の名前とデプロイのコマンドを先に固定しないと、後続の設定が環境ごとにばらける。

## 現状

リポジトリには `wrangler.toml` がなく、Worker を起動する手段もバインディングの定義もない。
`mise` のデプロイのタスクもないため、どの環境へ何を配信するかが決まっていない。

## 設計方針

`wrangler.toml` を 1 ファイルとし、トップレベルをローカル専用にする。
トップレベルの `name` は `warrior-local` とし、`[env.production]` を `warrior` とする。
staging の設定は必要になった時点で 0067 で追加する。
リモートの環境へデプロイするときは必ず `--env` を付ける。
`mise` のデプロイのタスクは、production の名前を含み、`--env production` を付けたコマンドを持つ。

バインディングは環境ごとに分ける。
D1 は `DB`、R2 は `BUCKET`、Queues の producer は `JOBS_QUEUE`、Workers AI は `AI` とする。
Cron Triggers は 1 分間隔で実行する。
Queues は AI の生成とレストの通知の予約を 1 つのキューで扱い、メッセージの種別で消費側の処理を分ける。
メッセージにはジョブの ID だけを入れ、記録の本文を入れない。

`[assets]` はフロントエンドのビルド成果物のディレクトリを指し、`run_worker_first` を `["/api/*"]`、`not_found_handling` を `single-page-application` にする。
`/api/*` は Worker の Rust の処理に渡し、それ以外は静的ファイルから返す。
CORS は許可しない。
Cloudflare の Preview URLs は使わない。

環境ごとの vars に、Relying Party ID、Origin、Google OAuth の認可 URL、トークン URL、UserInfo の URL、Google のクライアント ID、Google OAuth のリダイレクト URI、AI のモデル名、VAPID の公開鍵と subject、レート制限の上限値と時間窓を置く。
Google のクライアントシークレットと VAPID の秘密鍵は Secret に置く。
リポジトリには Secret の値を書かない。
ローカルの `.dev.vars` には production の資格情報を置かない。
ローカルの認証は、0008 のテスト用の OIDC の提供者を使う。

production の D1 の `database_id`、R2 のバケット名、Queues の名前はアカウントに依存する。
この issue では値の場所と検証のコマンドを用意し、実際の作成と値の設定は 0003 で行う。
staging の値は、必要になった時点で 0067 で追加する。
値が未設定の環境は `wrangler deploy --dry-run` が失敗してよい。
ローカルの `wrangler dev` は 0003 の値に依存せずに起動できるようにする。

## 完了条件

- [ ] `wrangler dev` がローカルの D1、R2、Queues、Cron Triggers、Workers AI のバインディングを起動し、`GET /api/health` が応答する。これは PRD の要求 ID を持たない基盤の条件である。
- [ ] トップレベルの `name` が `warrior-local` であり、`--env` を付けない `wrangler deploy` が production を更新しない。
- [ ] `mise` のデプロイのタスクが production の名前を必須にし、`--env production` を付ける。staging のタスクは必要になった時点で追加する。
- [ ] `[assets]` の `run_worker_first` が `["/api/*"]` で、`not_found_handling` が `single-page-application` である。
- [ ] vars と Secret の一覧が環境ごとに揃い、リポジトリに Secret の値がない。
- [ ] Preview URLs を使う設定がない。
- [ ] 設定の検証は `wrangler deploy --dry-run` と `wrangler dev` の起動で行う。コードの振る舞いを持たないため、PBT の対象はない。

## 解決方法

リポジトリのルートに `wrangler.toml` を置く。
トップレベルに `name = "warrior-local"`、`main`、`compatibility_date`、`[assets]`、`[[d1_databases]]`、`[[r2_buckets]]`、`[[queues.producers]]`、`[[queues.consumers]]`、`[ai]`、`[triggers]`、`[vars]` を置く。
`[env.production]` にトップレベルと同じキーを置き、production の値を書く。
`mise.toml` に `dev`、`deploy-production`、`db-migrate-local` のタスクを加える。
staging の設定と `deploy-staging` は、必要になった時点で 0067 で追加する。
`.dev.vars.example` に必要なキーの名前だけを書き、`.dev.vars` を `.gitignore` に加える。
`wrangler.toml` の値の意味と、0003 で設定する値を `docs/` の外に置かない。設定の手順は 0003 の issue に書く。
