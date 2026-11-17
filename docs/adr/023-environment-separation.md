# ADR-023: 環境をローカルと production に分ける

- Status: Accepted

## Context

アプリは Google OAuth とパスキーを使う。
パスキーの Relying Party ID と Google OAuth のリダイレクト URI は、ホスト名で決まる。
本番のデータを壊さずに、デプロイを伴う変更を試す環境が望ましい。
参照する `../../../brewbook` は、`wrangler.toml` のトップレベルをローカル専用にし、`[env.production]` をリモートの環境として分けている。
利用者は staging を用意しないと決めた。
実機の確認とプッシュ通知の計測は production のテスト用アカウントで行う。

## Decision

環境をローカルと production の 2 つに分ける。
リモートの D1、R2、Queues、Workers AI、Cron Triggers のバインディングは production に置く。

`wrangler.toml` は 1 ファイルとし、トップレベルをローカル専用にする。
トップレベルの Worker の名前は `warrior-local` とし、`[env.production]` を `warrior` とする。
リモートの環境へデプロイするときは、必ず `--env production` を付ける。
`mise` のデプロイのタスクは、環境の名前を含め、`--env production` を付けたコマンドを持つ。

Relying Party ID と Origin は環境ごとの vars に持つ。
ローカルは `localhost:8787`、production は `warrior.phack.workers.dev` とする。
Google OAuth のクライアント ID とリダイレクト URI、AI のモデル名も環境ごとの vars に持つ。
VAPID の公開鍵と subject は production の vars に持つ。
Google のクライアントシークレットと VAPID の秘密鍵は、production の Secret に置く。
レート制限の上限値と時間窓も production の vars に持つ。
ローカルの `.dev.vars` には production の資格情報を置かない。

D1 のマイグレーションは、production に適用してからデプロイする。
R2 のバケットとライフサイクルルールは production に作る。
Queues も production に作る。

Cloudflare の Preview URLs は使わない。
Preview URLs は D1 と R2 を本番と分離せず、URL が毎回変わるため、Relying Party ID と Google OAuth のリダイレクト URI を固定できない。

## Consequences

production の初回の設定が必要になる。
D1、R2、Queues の作成、Workers AI のバインディングと Cron Triggers の設定、`database_id` の設定、Secret の設定、Google OAuth のリダイレクト URI の追加、R2 のライフサイクルルールの適用を行う。

ホスト名を変えると、登録済みのパスキーが無効になり、Google OAuth のリダイレクト URI も変わる。
ホスト名を固定する。

staging を持たないため、実機の確認とプッシュ通知の計測は production のテスト用アカウントで行う。
テスト用の記録と通知の予約は、計測の後に削除する。

トップレベルの Worker の名前を `warrior-local` にしているため、`--env production` を付け忘れた `wrangler deploy` は本番を更新しない。
一方で、ローカルの名前でリモートに Worker を作らないように、デプロイのタスクは環境の名前を必須にする。

環境を増やすときは、本 ADR を改訂し、`wrangler.toml`、`mise` のタスク、デプロイの手順を合わせて更新する。

## References

- `../../../brewbook/docs/adr/0015-environment-separation.md`
- `docs/prd/warrior.md` の「非機能要件」
- `docs/adr/016-dioxus-tailwind-same-origin.md`
- `docs/adr/019-google-oauth-and-passkeys.md`
- Cloudflare Workers の環境と Static Assets のドキュメント
