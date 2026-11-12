# E2E とブラウザ API のテスト基盤を用意する

- Requirements: なし (テスト基盤は ADR-024 の判断であり、PRD の要求 ID を持たない)
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-e2e-harness

## 目的

実際の Worker、D1、R2、Queues、ブラウザを組み合わせる E2E と、ブラウザの API を直接確かめるテストの基盤を作る。
テスト用の OIDC の提供者を別のプロセスで起動し、本物の Google のアカウントに接続しない。

## 優先度根拠

認証、記録、同期、通知、画面の受入基準は、部品のテストだけでは確かめられない。
E2E の基盤がないと、後続の issue が受入基準を検証できない。

## 現状

0001 の `mise run check` は単体テストとビルドだけを実行する。
`wrangler dev` を起動する結合テスト、ブラウザを操作する E2E、ブラウザの API を直接動かすテストの基盤がない。

## 設計方針

E2E は `backend/warrior/tests/` に置き、`thirtyfour` で Chrome と Safari を操作する。
`wrangler dev` でローカルの D1、R2、Queues、Cron Triggers を起動し、`migrations/` を適用し、Dioxus のビルドを Static Assets として配信する。
起動の完了は `GET /api/health` の応答を条件に待つ。固定の `sleep` を書かない。

ブラウザの API を直接使う部品のテストは `wasm-bindgen-test` で書き、Chrome と Safari の実ブラウザで実行する。
対象は IndexedDB、Service Worker、Push、WebAuthn、通知である。

テスト用の OIDC の提供者は `tools/test-oidc` の別のプロセスで起動する。
提供者は Google のトークンエンドポイントと UserInfo エンドポイントと同じ形の応答を返す。
`/authorize`、`/token`、`/userinfo`、`/test/user` を持つ。
`/test/user` はテストが利用者を切り替えるための経路であり、`sub` を Cookie に保存する。
`/authorize` は Cookie の `sub` を使い、`nonce` をそのまま ID トークンの payload に入れる。
ID トークンの署名はダミーとし、サーバーは payload の `aud`、`iss`、`exp`、`nonce` を検証する。
本物の Google のアカウントには接続しない。
テスト専用の提供者と、その URL を指す vars は本番のビルドに含めない。

E2E は実行ごとに一意な利用者を作り、D1 の行と R2 の `users/<利用者 ID>/` と `exports/<利用者 ID>/` の接頭辞を分ける。
テストが作った資源は teardown で削除する。
D1 は `wrangler d1 execute DB --local --command "<削除する SQL>"` で削除し、R2 はローカルの保存先の `users/<利用者 ID>/` と `exports/<利用者 ID>/` の接頭辞だけを削除する。
削除の対象はテストが作った資源に限る。

待つのは条件であり、時間ではない。
サーバーは health の応答、画面は DOM の状態、同期は D1 の行と IndexedDB の状態、通知は Service Worker の状態を条件にする。
待機を含むテストは 10 回以上連続で実行し、安定して通ることを確かめる。

Chrome の E2E では ChromeDriver の WebAuthn の拡張コマンドで仮想認証器を付ける。
プッシュ通知は CDP の `ServiceWorker.deliverPushMessage` で実際の Service Worker にイベントを届ける。
オフラインは CDP のネットワークのエミュレーションで通信を切る。
Safari の WebDriver は WebAuthn の仮想認証器と CDP を持たないため、Safari の E2E は画面、API、IndexedDB、Service Worker のキャッシュを対象にする。

この issue では、テスト用の提供者でログインし、ホームの画面に到達する 1 本の E2E を置く。
各機能の E2E は、その機能の issue で追加する。

## 完了条件

- [ ] `mise run e2e` が `wrangler dev` とテスト用の OIDC の提供者を起動し、Chrome でログインからホームの表示までを実行して成功する。これは PRD の要求 ID を持たない基盤の条件である。
- [ ] `mise run e2e-safari` が macOS の runner で画面、API、IndexedDB、Service Worker のキャッシュの E2E を実行して成功する。
- [ ] テスト用の OIDC の提供者が `/authorize`、`/token`、`/userinfo`、`/test/user` を持ち、Google の応答と同じ形を返す。
- [ ] 本物の Google のアカウントと外部のサービスに接続しない。
- [ ] テスト専用の提供者と、その URL を指す vars が本番のビルドに含まれない。
- [ ] E2E の実行ごとに一意な利用者を作り、D1 の行と R2 の `users/<利用者 ID>/` と `exports/<利用者 ID>/` の接頭辞を分け、teardown で削除する。ハーネスのテストで `exports/<利用者 ID>/` にダミーのオブジェクトを置き、teardown で消えることを確かめる。
- [ ] 待機が条件で書かれ、固定の `sleep` がない。
- [ ] 待機を含むテストを 10 回連続で実行し、すべて成功する。
- [ ] Chrome の WebAuthn の仮想認証器、CDP のプッシュとオフラインの操作が、後続の issue から使える関数として用意されている。
- [ ] CI の Linux のジョブで Chrome の E2E が走り、macOS のジョブで Safari の E2E が走る。
- [ ] テスト基盤そのものに PBT は使わない。PBT は 0011 以降の純粋なロジックに置く。

## 解決方法

`backend/warrior/tests/support/mod.rs` に、`wrangler dev` の起動、health の待機、マイグレーションの適用、テスト用の提供者の起動、利用者の作成、teardown を置く。
`backend/warrior/tests/e2e_auth_smoke.rs` に、ログインからホームまでの E2E を置く。
`tools/test-oidc/Cargo.toml` と `tools/test-oidc/src/main.rs` に、テスト用の提供者を置く。ワークスペースに追加し、本番の wasm に含めない。HTTP のサーバーは標準ライブラリの `TcpListener` で実装し、本番の依存を増やさない。
`frontend/tests/browser.rs` に `wasm-bindgen-test` の入口を置く。
`mise.toml` に `e2e`、`e2e-safari`、`test-browser` を追加し、`check` から Chrome の E2E とブラウザ API のテストを呼ぶ。
`.github/workflows/ci.yml` に Linux と macOS のジョブを追加する。
