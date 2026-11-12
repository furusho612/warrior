# Cloudflare の staging と production の資源を用意する

- Requirements: なし (環境分離は ADR-023 の判断であり、PRD の要求 ID を持たない)
- Priority: Medium
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/pending-cloudflare-provisioning

## 目的

staging と production の D1、R2、Queues、Workers AI、Cron Triggers を実際に作成し、`wrangler.toml` の値を確定する。
Google OAuth とパスキーのホスト名を固定し、デプロイできる状態にする。

## 優先度根拠

ローカルの開発は 0002 で始められる。
staging へのデプロイと実機の確認は、この issue の完了を待つ。

## 現状

`wrangler.toml` はローカルの設定と環境ごとのキーを持つが、D1 の `database_id`、R2 のバケット、Queues の名前、Secret の値は未設定である。
Cloudflare のアカウントのサブドメインも決まっていない。

## 設計方針

ADR-023 に従い、staging と production を別の資源にする。
D1 のデータベース、R2 のバケット、Queues を環境ごとに作る。
R2 には CSV の書き出しを 7 日で削除するライフサイクルルールを設定する。
D1 のマイグレーションを対象の環境へ適用してからデプロイする。
Google OAuth のクライアント ID とリダイレクト URI、VAPID の鍵、レート制限の上限値を環境ごとの vars と Secret に設定する。
Relying Party ID と Origin は環境ごとのホスト名に固定する。
ホスト名を変えるとパスキーが無効になり、OAuth のリダイレクト URI も変わるため、ホスト名を後から変えない。

## 完了条件

- [ ] staging と production の D1、R2、Queues が作成され、`wrangler.toml` の値が実物と一致する。
- [ ] 各環境の D1 にマイグレーションを適用できる。
- [ ] 各環境の R2 にライフサイクルルールが設定されている。
- [ ] Google OAuth のクライアントとリダイレクト URI が各環境のホスト名に登録されている。
- [ ] VAPID の公開鍵と subject が vars に、秘密鍵が Secret に設定されている。
- [ ] `wrangler deploy --dry-run --env staging` と `wrangler deploy --dry-run --env production` が成功する。
- [ ] ローカルの `.dev.vars` に production の資格情報がない。

## 解決方法

Cloudflare のダッシュボードまたは `wrangler` のコマンドで資源を作る。
`wrangler d1 create`、`wrangler r2 bucket create`、`wrangler queues create` を使い、返った ID を `wrangler.toml` に書く。
`wrangler secret put` で各環境の Secret を設定する。
Google Cloud の OAuth の設定で、各環境のリダイレクト URI を追加する。
R2 のライフサイクルルールは `wrangler r2 bucket lifecycle` で設定する。
作業の結果は issue に追記し、Secret の値は書かない。

## 止める理由

Cloudflare のアカウント、サブドメイン、Google OAuth のクライアント、VAPID の鍵、実機の通知の許可が利用者に依存する。
エージェントは実環境へ接続しない。

## 再開条件

利用者が Cloudflare のアカウントとサブドメインを用意し、staging と production の資源を作る権限を渡す。
Google OAuth のクライアントと VAPID の鍵を用意する。
