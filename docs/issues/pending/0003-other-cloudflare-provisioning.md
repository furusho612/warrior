# Cloudflare の production の資源を用意する

- Requirements: なし (環境分離は ADR-023 の判断であり、PRD の要求 ID を持たない)
- Priority: Medium
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-cloudflare-provisioning

## 目的

production の D1、R2、Queues、Workers AI、Cron Triggers を実際に作成し、`wrangler.toml` の値を確定する。
Google OAuth とパスキーのホスト名を固定し、デプロイできる状態にする。
staging は必要になった時点で追加する。

## 優先度根拠

ローカルの開発は 0002 で始められる。
production へのデプロイと実機の確認は、この issue の完了を待つ。
staging は今は作らない。

## 現状

`wrangler.toml` はローカルの設定と production のキーを持つが、D1 の `database_id`、R2 のバケット、Queues の名前、Secret の値は未設定である。
Cloudflare のアカウントのサブドメインも決まっていない。
staging の設定は必要になった時点で追加する。

## 設計方針

ADR-023 はローカル、staging、production の 3 環境を定めるが、この issue では production を先に用意する。
staging は必要になった時点で、ADR-023 の手順に従って 0067 で追加する。

production の D1 のデータベース、R2 のバケット、Queues を作る。
R2 には CSV の書き出しを 7 日で削除するライフサイクルルールを設定する。
D1 のマイグレーションを production に適用してからデプロイする。
Google OAuth のクライアント ID とリダイレクト URI、VAPID の鍵、レート制限の上限値を production の vars と Secret に設定する。
Relying Party ID と Origin は production のホスト名に固定する。
ホスト名を変えるとパスキーが無効になり、OAuth のリダイレクト URI も変わるため、ホスト名を後から変えない。

## 完了条件

- [ ] production の D1、R2、Queues、Workers AI、Cron Triggers が作成され、`wrangler.toml` の値が実物と一致する。
- [ ] production の D1 に `migrations/` のすべてのマイグレーションが適用され、`wrangler d1 migrations list --remote --env production` に未適用がない。
- [ ] production の R2 にライフサイクルルールが設定されている。
- [ ] Google OAuth のクライアントとリダイレクト URI が production のホスト名に登録されている。
- [ ] VAPID の公開鍵と subject が production の vars に、秘密鍵が production の Secret に設定されている。
- [ ] `wrangler deploy --dry-run --env production` が成功する。
- [ ] ローカルの `.dev.vars` に production の資格情報がない。
- [ ] staging の資源を作っていない。

## 解決方法

Cloudflare のダッシュボードまたは `wrangler` のコマンドで production の資源を作る。
`wrangler d1 create`、`wrangler r2 bucket create`、`wrangler queues create` を使い、返った ID を `wrangler.toml` の `[env.production]` に書く。
`wrangler.toml` の `[env.production.vars]` に Relying Party ID、Origin、レート制限の上限値と時間窓を置く。
`[env.production.ai]` と `[env.production.triggers]` を production 用に設定する。
`wrangler d1 migrations apply --remote --env production` でマイグレーションを適用する。
`wrangler secret put --env production` で production の Secret を設定する。
Google Cloud の OAuth の設定で、production のリダイレクト URI を追加する。
R2 のライフサイクルルールは `wrangler r2 bucket lifecycle add <BUCKET> <NAME> <PREFIX> --expire-days 7` で設定する。
作業の結果は issue に追記し、Secret の値は書かない。

## 止める理由

Cloudflare のアカウント、サブドメイン、Google OAuth のクライアント、VAPID の鍵、実機の通知の許可が利用者に依存する。
エージェントは実環境へ接続しない。
staging はこの issue の対象外である。

## 再開条件

利用者が Cloudflare のアカウントとサブドメインを用意し、production の資源を作る権限を渡す。
Google OAuth のクライアントと VAPID の鍵を用意する。
staging が必要になった時点で、0067 を着手する。
