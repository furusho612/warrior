# ADR-016: フロントエンドを Dioxus と Tailwind で書き、同じ Worker から同一オリジンで配信する

- Status: Accepted

## Context

利用者はスマートフォンと PC のブラウザからアプリを使う。
画面は 11 種類あり、記録画面はスマートフォンでの片手操作を前提とする。
参照する `../../../brewbook` は、フロントエンドを Dioxus 0.7.10 で書き、Tailwind CSS を使い、`dx` でビルドしている。
フロントエンドの成果物は、利用者向けの Worker の Static Assets として同じホスト名から配信している。
`/api/*` へのリクエストだけを Rust の処理に渡し、それ以外は静的ファイルから返す。
同じオリジンにすると、CORS の設定が不要になり、HttpOnly Cookie を使える。
パスキーの Relying Party ID と Google OAuth のリダイレクト URI はホスト名に依存する。

## Decision

フロントエンドは Rust の Dioxus 0.7.10 で書き、Web だけを対象にする。
画面の経路は `dioxus-router` で扱い、経路の台帳を 1 か所に置く。
ビルドは Dioxus CLI の `dx build --release --platform web` を使い、Trunk は使わない。
スタイルは Tailwind CSS を使い、`dx` が Tailwind CLI を起動して `public/tailwind.css` を出力する。

フロントエンドの成果物を、利用者向けの Worker の Static Assets として同梱する。
`wrangler.toml` の `[assets]` で成果物のディレクトリを指定し、`run_worker_first` を `["/api/*"]` にし、`not_found_handling` を `single-page-application` にする。
`/api/*` は Rust の処理に渡し、それ以外の経路は静的ファイルから返す。
アプリは CORS を許可しない。
状態を変更するリクエストは `Origin` ヘッダを検証し、同一オリジン以外を拒否する。
認証、AI、プッシュ購読の経路に、D1 のカウンタで利用者ごとと IP ごとの上限を設ける。
上限値は環境ごとの vars に持つ。
認証の前の経路は IP ごと、認証後の経路は利用者ごとに数える。

CSP、HSTS、X-Content-Type-Options、Referrer-Policy、Permissions-Policy を付ける。
CSP は、同じオリジンのスクリプトと wasm の実行に必要な `'wasm-unsafe-eval'`、フォントの配信元だけを許可し、外部スクリプトを許可しない。
フォントはデザインと同じ Noto Sans JP と Barlow Condensed を Google Fonts から読み込む。
CSP では `fonts.googleapis.com` と `fonts.gstatic.com` だけを許可する。
自前配信に変える場合は、フォントのサブセット化とライセンスを確認する。

セッションは HttpOnly、Secure、SameSite=Lax の Cookie で渡す。
UI の文言は型付きのキーと日本語の表で持ち、画面のコードに文字列を直接書かない。
状態管理、CSS-in-Rust、i18n の汎用クレートは追加しない。
色、余白、書体、部品の値は `docs/design/tokens.json` と `docs/design/` を原本とし、Tailwind のテーマと CSS 変数に写す。
グラフはフロントエンドで描画し、集計はバックエンドの API が行う。
PWA の manifest とアイコンは `frontend/public/` に置く。

## Consequences

1 回のデプロイでフロントエンドとバックエンドの両方を更新する。
CORS の設定と、別オリジン間のトークン管理が不要になる。

Relying Party ID は Worker のホスト名になる。
ホスト名を変えると、登録済みのパスキーが無効になり、Google OAuth のリダイレクト URI も変わる。
公開前に最終のホスト名を決める。

ローカル開発では `dx serve` を使わない。
`dx` の開発サーバーは別オリジンになり、同一オリジンの前提と `Origin` の検証に反するためである。
フロントエンドをビルドしてから `wrangler dev` を起動し、同じオリジンから配信する。

wasm の初回ロードのサイズを計測する。
`dx` のリリースビルドと `wasm-opt` を使い、サイズが問題になる場合は画面の分割を検討する。

Dioxus 0.7 の開発は Dioxus Labs が Cognition の下で続いている。
版は `mise.toml` と `Cargo.lock` で固定し、更新は別の issue で行う。

レート制限の上限値は、個人利用の実測で調整する。
Google Fonts は外部へのリクエストになる。
自前配信に変える場合は、フォントのサブセットとライセンスを確認する。

## References

- `../../../brewbook/docs/adr/0005-same-origin-deployment.md`
- `../../../brewbook/docs/adr/0017-frontend-rust-dioxus-web-only.md`
- `docs/design/README.md` の「画面のきまり」「配色」「フォント」
- `docs/prd/warrior.md` の「非機能要件」
- Cloudflare Workers の Static Assets のドキュメント
