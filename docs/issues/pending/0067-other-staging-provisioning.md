# staging の資源を必要になった時点で用意する

- Requirements: REQ-NFR-001, REQ-NFR-009, REQ-NFR-013 (staging の用意は ADR-023)
- Priority: Low
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-staging-provisioning

## 目的

staging を追加する必要が生じたときに、ADR-023 の手順で staging の D1、R2、Queues、Workers AI、Cron Triggers を用意する。
実機の確認 (iPhone の Safari のパスキー、iOS の PWA の Service Worker 自身の wasm、HTTPS と暗号化)、AI の品質評価を production から分離して行えるようにする。

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
Google OAuth のクライアント ID とリダイレクト URI、VAPID の鍵、レート制限の上限値を staging の vars と Secret に設定する。
R2 には CSV の書き出しを 7 日で削除するライフサイクルルールを設定する。
D1 のマイグレーションを staging に適用してからデプロイする。
`mise` に `deploy-staging` を追加する。
ローカルの `.dev.vars` には staging の資格情報を置き、production の資格情報を置かない。

ホスト名を変えるとパスキーが無効になり、OAuth のリダイレクト URI も変わるため、ホスト名を後から変えない。
staging で 0006 の HTTPS と暗号化、0016 の Safari のパスキー、0020 の iOS の PWA の Service Worker 自身の wasm の起動を確認する。
staging を追加した後は、production と同じ `AI_MODEL` を staging に置く。

## 完了条件

- [ ] staging の D1、R2、Queues、Workers AI、Cron Triggers が作成され、`wrangler.toml` の `[env.staging]` の値が実物と一致する。
- [ ] staging の D1 にマイグレーションが適用され、適用済みの版が production と同じである。
- [ ] staging の Relying Party ID と Origin が staging のホスト名である。これは ADR-023 の Decision に対応する。
- [ ] staging のレート制限の上限値が staging の vars にある。これは ADR-023 の Decision に対応する。
- [ ] staging の R2 にライフサイクルルールが設定されている。
- [ ] Google OAuth のクライアントとリダイレクト URI が staging のホスト名に登録されている。
- [ ] VAPID の公開鍵と subject が staging の vars に、秘密鍵が staging の Secret に設定されている。
- [ ] `wrangler deploy --dry-run --env staging` が成功する。
- [ ] `mise` の `deploy-staging` が `--env staging` を付けて実行する。
- [ ] ローカルの `.dev.vars` に staging の資格情報だけがあり、production の資格情報がない。
- [ ] staging の `AI_MODEL` が production と同じである。
- [ ] 0006 の HTTPS と実環境の暗号化 (REQ-NFR-009 の受入基準「すべての通信が暗号化された経路で行われる」) を staging で確かめる。
- [ ] 0016 の Safari のパスキー (REQ-NFR-013 の受入基準「ログインの後にマイページからパスキーを登録でき、登録の後はパスキー認証でログインできる」) を staging で確かめる。
- [ ] 0020 の iOS の Service Worker 自身の wasm の起動 (ADR-020 の Consequences の「iOS のホーム画面に追加した PWA で、Service Worker 自身の wasm が起動できることを確認する」) を staging で確かめる。これは内部条件であり、REQ-NFR-001 の受入基準の外側である。
- [ ] 0020 の iOS の PWA の起動と、読み込まれた画面の表示を続けられることを staging で確かめる。これは REQ-NFR-001 の受入基準「スマートフォンの Safari と Chrome、PC の Chrome と Safari の最新版で、すべての画面を操作できる」に対応する。

## 解決方法

Cloudflare のダッシュボードまたは `wrangler` のコマンドで staging の資源を作る。
`wrangler d1 create`、`wrangler r2 bucket create`、`wrangler queues create` を使い、返った ID を `wrangler.toml` の `[env.staging]` に書く。
`wrangler secret put --env staging` で staging の Secret を設定する。
Google Cloud の OAuth の設定で、staging のリダイレクト URI を追加する。
R2 のライフサイクルルールは `wrangler r2 bucket lifecycle add <BUCKET> <NAME> <PREFIX> --expire-days 7` で設定する。
`mise.toml` に `deploy-staging` を追加する。
`.dev.vars.example` と `.dev.vars` に staging の資格情報の名前を追加する。
作業の結果は issue に追記し、Secret の値は書かない。
利用者が staging で iPhone の Safari のパスキーの経路、iOS の PWA の Service Worker 自身の wasm の起動、HTTPS と暗号化を確認し、結果を issue に追記する。

## 止める理由

今は production を先に用意するため、staging を作らない。
iPhone の Safari のパスキー、iOS の PWA の Service Worker 自身の wasm、HTTPS の確認、AI の品質評価のうち、どの作業を先に始めるかは、その着手時に決まる。
エージェントは実環境へ接続しない。

## 再開条件

利用者が staging を必要とする作業 (iPhone の Safari のパスキー、iOS の PWA の Service Worker 自身の wasm、HTTPS の確認を含む) を始める。
0003 の production の資源ができている。
利用者が staging の資源を作る権限と、Google OAuth と VAPID の staging 用の資格情報を用意する。
