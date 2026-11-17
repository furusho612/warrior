# Cloudflare の production の資源を用意する

- Requirements: REQ-NFR-017 (レート制限の環境ごとの設定。環境分離は ADR-023 の判断であり、ほかの要求 ID を持たない)
- Priority: Medium
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-cloudflare-provisioning

## 目的

production の D1、R2、Queues を作成し、Workers AI のバインディングと Cron Triggers を設定し、`wrangler.toml` の値を確定する。
レート制限の上限値と時間窓も production の vars に置く。
Google OAuth とパスキーのホスト名を固定し、デプロイできる状態にする。
staging は作らない。

## 優先度根拠

ローカルの開発は 0002 で始められる。
production へのデプロイと実機の確認は、この issue の完了を待つ。
staging は作らない。

## 現状

`wrangler.toml` はローカルの設定と production のキーを持つが、D1 の `database_id`、R2 のバケット、Queues の名前、Secret の値は未設定である。
Cloudflare のアカウントのサブドメインも決まっていない。
staging の設定は持たない。

## 設計方針

ADR-023 はローカルと production の 2 環境を定める。


production の D1 のデータベース、R2 のバケット、Queues を作る。
R2 のバケットは production 用に 1 つ用意し、`exports/` の接頭辞には CSV の書き出しを 7 日で削除するライフサイクルルールを設定する。
D1 のマイグレーションを production に適用してからデプロイする。
Google OAuth のクライアント ID、リダイレクト URI、Google のクライアントシークレット、AI のモデル名、VAPID の公開鍵と subject と秘密鍵、レート制限の上限値と時間窓を production の vars と Secret に設定する。
Relying Party ID と Origin は production のホスト名に固定する。
ホスト名を変えるとパスキーが無効になり、OAuth のリダイレクト URI も変わるため、ホスト名を後から変えない。

## 完了条件

- [ ] production の D1、R2、Queues が作成され、Workers AI のバインディングと Cron Triggers の設定が `[env.production]` にあり、`wrangler.toml` の値が実物と一致する。これは ADR-023 の Decision と Consequences に対応する。
- [ ] production の D1 に `migrations/` のすべてのマイグレーションが適用され、`wrangler d1 migrations list DB --remote --env production` に未適用がない。これは ADR-023 の Decision に対応する。
- [x] production の vars にある Relying Party ID と Origin が production のホスト名である。これは ADR-023 の Decision に対応する。
- [x] production のレート制限の上限値と時間窓が production の vars にある。これは `REQ-NFR-017` の受入基準「上限と時間窓は環境ごとの設定で変えられる」に対応する。
- [ ] production の R2 バケットは 1 つで、`exports/` の接頭辞に、書き出しを 7 日で削除するライフサイクルルールが設定され、`wrangler r2 bucket lifecycle list <BUCKET>` が `exports/` の接頭辞の規則だけを返し、`users/` の接頭辞に規則がない。これは ADR-017 の Decision と ADR-023 の Decision と Consequences に対応する。
- [ ] Google OAuth のクライアントとリダイレクト URI が production のホスト名に登録され、production の vars にある Google のクライアント ID とリダイレクト URI が production のホスト名に対応する。これは ADR-023 の Decision と Consequences に対応する。
- [ ] VAPID の公開鍵と subject が production の vars に、Google のクライアントシークレットと VAPID の秘密鍵が production の Secret に設定されている。これは ADR-023 の Decision に対応する。
- [x] `wrangler deploy --dry-run --env production` が成功する。これはデプロイ設定の検証の内部条件である。
- [ ] ローカルの `.dev.vars` に production の資格情報がない。これは ADR-023 の Decision に対応する。
- [x] staging の資源を作っていない。これはこの issue の対象範囲に対応する。

## 解決方法

利用者が Cloudflare のダッシュボードまたは `wrangler` のコマンドで production の資源を作る。
利用者が `wrangler d1 create <DB>`、`wrangler r2 bucket create <BUCKET>`、`wrangler queues create <QUEUE>` を使い、D1 は返った `database_id`、R2 と Queues は返った名前を `wrangler.toml` の `[env.production]` に書く。`<DB>`、`<BUCKET>`、`<QUEUE>` は production 用の別名にし、R2 のバケットは production 用に 1 つだけ作る。
利用者が `wrangler.toml` の `[env.production.vars]` に Relying Party ID、Origin、Google OAuth のクライアント ID、リダイレクト URI、AI のモデル名、VAPID の公開鍵と subject、レート制限の上限値と時間窓を含む production の vars を置く。
利用者が `[env.production.ai]` と `[env.production.triggers]` を production 用に設定する。
利用者が `wrangler d1 migrations apply DB --remote --env production` でマイグレーションを適用する。
利用者が `wrangler secret put <GOOGLE_CLIENT_SECRET> --env production` と `wrangler secret put <VAPID_PRIVATE_KEY> --env production` で production の Secret を設定する。
利用者が Google Cloud の OAuth の設定で、production のリダイレクト URI を追加する。
利用者が R2 のライフサイクルルールを `wrangler r2 bucket lifecycle add <BUCKET> <NAME> exports/ --expire-days 7` で設定する。
作業の結果は issue に追記し、Secret の値は書かない。

## 進捗

`wrangler.toml` の `[env.production]` に D1、R2、Queues、Workers AI、Cron Triggers、vars を置き、レート制限の上限と窓も含めた。D1 の `database_id` は `2b764b0f-85ea-4582-932a-479d9c83d14f` とし、RP_ID と Origin は `warrior.phack.workers.dev`、Google のクライアント ID とリダイレクト URI は production のホスト名に合わせた。R2 は `warrior-ai-and-exports`、Queue は `warrior-ai-and-rest-push` とした。VAPID の公開鍵と `mailto:shoichi.furuya.0612@gmail.com` の subject も設定した。
`wrangler deploy --dry-run --env production` が成功することを確認した。
残りは R2 と Queues の作成確認、R2 のライフサイクルルール、Google OAuth のリダイレクト URI の登録、VAPID の秘密鍵と Google のクライアントシークレットの Secret、D1 のマイグレーションの適用である。

## 止める理由

Cloudflare のアカウント、サブドメイン、Google OAuth のクライアント、VAPID の鍵が利用者に依存する。
エージェントは実環境へ接続しない。
staging は作らない。

## 再開条件

利用者が Cloudflare のアカウントとサブドメインを用意し、production の資源を作る権限を渡す。
Google OAuth のクライアントと VAPID の鍵を用意する。

