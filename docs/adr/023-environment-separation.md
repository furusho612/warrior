# ADR-023: 環境をローカル、staging、production に分ける

- Status: Accepted

## Context

アプリは Google OAuth とパスキーを使う。
パスキーの Relying Party ID と Google OAuth のリダイレクト URI は、ホスト名で決まる。
本番のデータを壊さずに、デプロイを伴う変更を試す環境が必要である。
参照する `../../../brewbook` は、`wrangler.toml` のトップレベルをローカル専用にし、`[env.staging]` と `[env.production]` をリモートの環境として分けている。
トップレベルの Worker の名前を `brewbook-local` にして、`--env` を付け忘れた `wrangler deploy` が本番を更新しないようにしている。

## Decision

環境をローカル、staging、production の 3 つに分ける。
リモートの D1、R2、Queues、Workers AI、Cron Triggers のバインディングは環境ごとに分ける。

`wrangler.toml` は 1 ファイルとし、トップレベルをローカル専用にする。
トップレベルの Worker の名前は `warrior-local` とし、`[env.staging]` を `warrior-staging`、`[env.production]` を `warrior` とする。
リモートの環境へデプロイするときは、必ず `--env` を付ける。
`mise` のデプロイのタスクは、環境の名前を含め、`--env` を付けたコマンドを持つ。

Relying Party ID と Origin は環境ごとの vars に持つ。
ローカルは `localhost:8787`、staging は `warrior-staging.<アカウントのサブドメイン>.workers.dev`、production は `warrior.<アカウントのサブドメイン>.workers.dev` とする。
Google OAuth のクライアント ID とリダイレクト URI、AI のモデル名も環境ごとの vars に持つ。
VAPID の公開鍵と subject は環境ごとの vars に持つ。
Google のクライアントシークレットと VAPID の秘密鍵は、環境ごとの Secret に置く。
レート制限の上限値も環境ごとの vars に持つ。
ローカルの `.dev.vars` には staging の資格情報だけを置き、本番の資格情報をローカルに置かない。

D1 のマイグレーションは、対象の環境に適用してからデプロイする。
R2 のバケットとライフサイクルルールは環境ごとに作る。
Queues も環境ごとに作る。

Cloudflare の Preview URLs は使わない。
Preview URLs は D1 と R2 を本番と分離せず、URL が毎回変わるため、Relying Party ID と Google OAuth のリダイレクト URI を固定できない。

## Consequences

staging の初回の設定が必要になる。
D1、R2、Queues、Workers AI、Cron Triggers の作成、`database_id` の設定、Secret の設定、Google OAuth のリダイレクト URI の追加、R2 のライフサイクルルールの適用を行う。

ホスト名を変えると、登録済みのパスキーが無効になり、Google OAuth のリダイレクト URI も変わる。
ホスト名を固定する。

トップレベルの Worker の名前を `warrior-local` にしているため、`--env` を付け忘れた `wrangler deploy` は本番を更新しない。
一方で、ローカルの名前でリモートに Worker を作らないように、デプロイのタスクは環境の名前を必須にする。

環境ごとに無料枠を消費する。
staging と production の D1、R2、Queues、Workers AI、Cron Triggers の利用量を分けて確認する。

環境を増やすときは、本 ADR を改訂し、`wrangler.toml`、`mise` のタスク、デプロイの手順を合わせて更新する。

## References

- `../../../brewbook/docs/adr/0015-environment-separation.md`
- `docs/prd/warrior.md` の「非機能要件」
- `docs/adr/016-dioxus-tailwind-same-origin.md`
- `docs/adr/019-google-oauth-and-passkeys.md`
- Cloudflare Workers の環境と Static Assets のドキュメント
