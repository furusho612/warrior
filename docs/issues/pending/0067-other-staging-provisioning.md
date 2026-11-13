# staging の資源を必要になった時点で用意する

- Requirements: REQ-NFR-001, REQ-NFR-003, REQ-NFR-004, REQ-NFR-005, REQ-NFR-009, REQ-NFR-013, REQ-NFR-014, REQ-NFR-017 (staging の用意は ADR-023)
- Priority: Low
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-staging-provisioning

## 目的

staging を追加する必要が生じたときに、ADR-023 の手順で staging の D1、R2、Queues、Workers AI、Cron Triggers を用意する。
実機の確認 (iPhone の Safari のパスキーとすべての画面、Android の Chrome のパスキー、iOS の PWA の Service Worker 自身の wasm と起動と表示の継続、Android の Chrome のすべての画面、iOS Safari と PC Safari と Android Chrome のオフライン記録、HTTPS と暗号化)、AI の品質評価、プッシュ通知の実機計測 (0038) を production から分離して行えるようにする。
レート制限の上限値と時間窓も staging の vars に置く。

## 優先度根拠

production を先に用意するため、staging は今は作らない。
実機の確認や AI の品質評価で staging が必要になったときに着手する。

## 現状

0002 と 0003 はローカルと production だけを用意する。
staging の設定と資源はない。

## 設計方針

ADR-023 の staging の定義に従う。
`[env.staging]` を `warrior-staging` とし、D1、R2、Queues、Workers AI、Cron Triggers を production と分ける。
Relying Party ID と Origin は staging のホスト名に固定する。
Google OAuth のクライアント ID、リダイレクト URI、Google のクライアントシークレット、AI のモデル名、VAPID の公開鍵と subject と秘密鍵、レート制限の上限値と時間窓を staging の vars と Secret に設定する。
R2 のバケットは staging 用に 1 つ用意し、`exports/` の接頭辞には CSV の書き出しを 7 日で削除するライフサイクルルールを設定する。
D1 のマイグレーションを staging に適用してからデプロイする。
`mise` に `deploy-staging` を追加する。
ローカルの `.dev.vars` には staging の資格情報を置き、production の資格情報を置かない。

ホスト名を変えるとパスキーが無効になり、OAuth のリダイレクト URI も変わるため、ホスト名を後から変えない。
staging で 0006 の HTTPS と暗号化、0016 の Safari と Android の Chrome のパスキー、0020 の iOS の PWA の Service Worker 自身の wasm の起動と PWA の起動と表示の継続、iPhone の Safari と Android の Chrome のすべての画面、iOS Safari と PC Safari と Android Chrome のオフライン記録を確認する。
staging を追加した後は、production と同じ `AI_MODEL` を staging に置く。

## 完了条件

- [ ] staging の D1、R2、Queues が作成され、staging の D1 の `database_id`、R2 のバケット名、Queues の名前が production と異なり、Workers AI のバインディングと Cron Triggers の設定が `[env.staging]` にあり、`wrangler.toml` の値が実物と一致する。これは ADR-023 の Decision と Consequences に対応する。
- [ ] staging の D1 に `migrations/` のすべてのマイグレーションが適用され、`wrangler d1 migrations list DB --remote --env staging` に未適用がない。適用済みの版は production と同じか、それより新しい。これは ADR-023 の Decision に対応する。
- [ ] staging の vars にある Relying Party ID と Origin が staging のホスト名である。これは ADR-023 の Decision に対応する。
- [ ] staging のレート制限の上限値と時間窓が staging の vars にある。これは `REQ-NFR-017` の受入基準「上限と時間窓は環境ごとの設定で変えられる」に対応する。
- [ ] staging の R2 バケットは 1 つで、`exports/` の接頭辞に、書き出しを 7 日で削除するライフサイクルルールが設定され、`wrangler r2 bucket lifecycle list <BUCKET>` が `exports/` の接頭辞の規則だけを返し、`users/` の接頭辞に規則がない。これは ADR-017 の Decision と ADR-023 の Decision と Consequences に対応する。
- [ ] Google OAuth のクライアントとリダイレクト URI が staging のホスト名に登録され、staging の vars にある Google のクライアント ID とリダイレクト URI が staging のホスト名に対応する。これは ADR-023 の Decision と Consequences に対応する。
- [ ] VAPID の公開鍵と subject が staging の vars に、Google のクライアントシークレットと VAPID の秘密鍵が staging の Secret に設定されている。これは ADR-023 の Decision に対応する。
- [ ] `wrangler deploy --dry-run --env staging` が成功する。これはデプロイ設定の検証の内部条件である。
- [ ] `mise` の `deploy-staging` が `--env staging` を付けて実行する。これは ADR-023 の Decision に対応する。
- [ ] 利用者が `mise run deploy-staging` を実行し、`warrior-staging.<アカウントのサブドメイン>.workers.dev` の `GET /api/health` が 200 を返す。これは staging の起動の内部条件である。
- [ ] ローカルの `.dev.vars` に staging の資格情報だけがあり、production の資格情報がない。これは ADR-023 の Decision に対応する。
- [ ] staging の `AI_MODEL` が production と同じである。これは 0061 と 0069 の判断に対応する。
- [ ] 0006 の HTTPS と実環境の暗号化 (REQ-NFR-009 の受入基準「すべての通信が暗号化された経路で行われる」) を staging で確かめる。
- [ ] 0016 の Safari のパスキー (REQ-NFR-013 の受入基準「登録、追加、削除の前に、Google アカウントまたは既存のパスキーによる再認証を求め、失敗した場合は操作を実行しない。1 つのアカウントに複数のパスキーを登録でき、追加と削除ができる」) を staging で確かめる。Safari での登録、ログイン、複数パスキーの追加と削除、Google からの復旧、再認証なしの拒否を確かめる。Google からの復旧は `REQ-NFR-014` の受入基準「登録したパスキーを使えない状態でも、Google アカウントでログインして新しいパスキーを登録できる」に対応する。
- [ ] Android の Chrome のパスキーを staging で確かめる。Android での登録、ログイン、複数パスキーの追加と削除、Google からの復旧、再認証なしの拒否を確かめる。これは `REQ-NFR-013` と `REQ-NFR-014` の受入基準に対応する。
- [ ] 0020 の iOS の Service Worker 自身の wasm の起動 (ADR-020 の Consequences の「iOS のホーム画面に追加した PWA で、Service Worker 自身の wasm が起動できることを確認する」) を staging で確かめる。これは内部条件であり、REQ-NFR-001 の受入基準の外側である。
- [ ] 0020 の iOS の PWA の起動と、読み込んだ後に通信を切っても画面の表示を続けられることを staging で確かめる。これは REQ-NFR-001 の受入基準「スマートフォンの Safari と Chrome、PC の Chrome と Safari の最新版で、すべての画面を操作できる」のうち、iOS の PWA を満たす。
- [ ] Android の Chrome ですべての画面を操作できることを staging で確かめる。これは REQ-NFR-001 の受入基準「スマートフォンの Safari と Chrome、PC の Chrome と Safari の最新版で、すべての画面を操作できる」のうち、Android の Chrome を満たす。PC の Chrome と Safari の画面は 0007 の経路の台帳の E2E で確認し、各機能の操作はその機能の E2E と staging で確認する。
- [ ] iPhone の Safari ですべての画面を操作できることを staging で確かめる。これは REQ-NFR-001 の受入基準「スマートフォンの Safari と Chrome、PC の Chrome と Safari の最新版で、すべての画面を操作できる」のうち、iPhone の Safari を満たす。
- [ ] iOS の Safari、PC の Safari、Android の Chrome で、通信を切って記録を続け、通信を戻して再読み込みし、復帰後の同期と未保存の表示を確かめる。これは `REQ-NFR-003`、`REQ-NFR-004`、`REQ-NFR-005` の受入基準に対応する。

## 解決方法

利用者が Cloudflare のダッシュボードまたは `wrangler` のコマンドで staging の資源を作る。
利用者が `wrangler d1 create <DB>`、`wrangler r2 bucket create <BUCKET>`、`wrangler queues create <QUEUE>` を使い、D1 は返った `database_id`、R2 と Queues は返った名前を `wrangler.toml` の `[env.staging]` に書く。`<DB>`、`<BUCKET>`、`<QUEUE>` は staging 用の別名にし、R2 のバケットは staging 用に 1 つだけ作る。
利用者が `wrangler.toml` の `[env.staging.vars]` に Relying Party ID、Origin、Google OAuth のクライアント ID、リダイレクト URI、AI のモデル名、VAPID の公開鍵と subject、レート制限の上限値と時間窓を含む staging の vars を置く。
利用者が `[env.staging.ai]` と `[env.staging.triggers]` を staging 用に設定する。
利用者が `wrangler d1 migrations apply DB --remote --env staging` でマイグレーションを適用する。
利用者が `wrangler secret put <GOOGLE_CLIENT_SECRET> --env staging` と `wrangler secret put <VAPID_PRIVATE_KEY> --env staging` で staging の Secret を設定する。
利用者が Google Cloud の OAuth の設定で、staging のリダイレクト URI を追加する。
利用者が R2 のライフサイクルルールを `wrangler r2 bucket lifecycle add <BUCKET> <NAME> exports/ --expire-days 7` で設定する。
エージェントが `mise.toml` に `deploy-staging` を追加する。
利用者が `mise run deploy-staging` で staging にデプロイする。
エージェントが `.dev.vars.example` に staging の資格情報の名前を追加する。利用者が `.dev.vars` に staging の資格情報を置く。
作業の結果は issue に追記し、Secret の値は書かない。
利用者が staging で iPhone の Safari のパスキーとすべての画面、Android の Chrome のパスキー、iOS の PWA の Service Worker 自身の wasm の起動と PWA の起動と表示の継続、Android の Chrome のすべての画面、iOS Safari と PC Safari と Android Chrome のオフライン記録、HTTPS と暗号化を確認し、結果を issue に追記する。

## 止める理由

今は production を先に用意するため、staging を作らない。
iPhone の Safari のパスキーとすべての画面、Android の Chrome のパスキー、iOS の PWA の Service Worker 自身の wasm と起動と表示の継続、Android の Chrome のすべての画面、iOS Safari と PC Safari と Android Chrome のオフライン記録、HTTPS の確認、AI の品質評価、プッシュ通知の実機計測 (0038) のうち、どの作業を先に始めるかは、その着手時に決まる。
エージェントは実環境へ接続しない。

## 再開条件

利用者が staging を必要とする作業 (iPhone の Safari のパスキーとすべての画面、Android の Chrome のパスキー、iOS の PWA の Service Worker 自身の wasm と起動と表示の継続、Android の Chrome のすべての画面、iOS Safari と PC Safari と Android Chrome のオフライン記録、HTTPS の確認、プッシュ通知の実機計測 (0038)、AI の品質評価 (0069) を含む) を始める。
0003 の production の資源ができている。
利用者が staging の資源を作る権限と、Google OAuth と VAPID の staging 用の資格情報を用意する。
