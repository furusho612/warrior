# Cargo ワークスペースと mise のタスクを用意する

- Requirements: なし (ビルドと開発環境の基盤であり、PRD の要求 ID を持たない)
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-repository-foundation

## 目的

すべての後続 issue が同じツールの版と同じコマンドでビルド、テスト、デプロイできる状態を作る。
ADR-014、ADR-022、ADR-025 の構成をリポジトリの骨格に落とす。

## 優先度根拠

後続のすべての issue がこのワークスペースとタスクに依存する。
ここが動かないと、ほかの issue の変更を検証できない。

## 現状

リポジトリには `docs/` だけがあり、`Cargo.toml`、`mise.toml`、アプリのコード、CI の設定がない。
そのため、ビルド、テスト、デプロイのどのコマンドも実行できない。

## 設計方針

Cargo のワークスペースをリポジトリのルートに置き、次のクレートをメンバーにする。

```text
backend/warrior           Cloudflare Workers の入口とバインディング
backend/warrior_core      バインディングに依存しない純粋なロジック
backend/pbt               proptest による性質テスト
frontend                  Dioxus の Web アプリ
service-worker            Service Worker の wasm
```

Fuzzing のクレートは 0065 で追加する。
ADR-014 のとおり、Worker のクレートと純粋なロジックのクレートを分ける。
`backend/warrior_core` は `wasm32-unknown-unknown` とネイティブの両方でビルドできる状態を保つ。

`mise.toml` に ADR-022 のツールの版を固定する。
Rust は 1.99.0、`wasm32-unknown-unknown` のターゲット、clippy、rustfmt を含める。
`worker-build` は 0.8.6、Node.js は 24.21.0、`wrangler` は 4.135.0、Dioxus CLI は 0.7.10、`wasm-bindgen` は 0.2.129、ChromeDriver は 157.0.8103.15、JDK は Temurin 25、`cargo-fuzz` は 0.13.2、`ripgrep` は 15.2.0 とする。
版は `latest` にしない。

`mise` のタスクに、セットアップ、整形、lint、単体テスト、PBT、フロントエンドのビルド、Service Worker のビルド、ビルド、`check` を置く。
`frontend:build-service-worker` を `frontend:build` の依存に置く。
`check` は整形、lint、単体テスト、PBT、フロントエンドのビルド、Service Worker のビルドを順に実行する。
Fuzzing の型検査は 0065 で `check` に加える。
結合テスト、ブラウザのテスト、E2E、形式手法のタスクは、それぞれの issue で `check` に加える。
各 issue は、追加したテストを `mise run check` か、CI で実行するタスクに含める。

CI は GitHub Actions を前提にし、Linux の runner で `mise run check` を実行するジョブを置く。
macOS の runner を使う Safari の E2E は 0008 で加える。
CI の提供者が変わる場合は、同じ `mise run check` を呼ぶ設定に置き換える。

本番のビルドに含めるクレートは ADR-025 の表に限る。
`worker` は `d1` と `queue`、`dioxus` は `default-features = false` で `web`、`minimal`、`document`、`p256` は `default-features = false` で `ecdsa` だけを有効にする。
`web-sys` は使う API の feature だけを列挙する。
直接の依存を追加するときは、`cargo tree --edges normal` と `cargo tree --target wasm32-unknown-unknown` の差分を確認する。
この確認は自動テストにせず、レビューの観点にする。

振る舞いを持つコードはこの issue では作らない。
`backend/warrior_core` に health 用の定数を置き、ビルドとテストの対象を作る。
実際のロジックは 0011 以降で追加する。

## 完了条件

- [ ] ワークスペースの全クレートがネイティブと `wasm32-unknown-unknown` でビルドできる。これは PRD の要求 ID を持たない基盤の条件である。
- [ ] `mise install` の後に `mise run check` が成功する。
- [ ] `mise.toml` のツールの版が ADR-022 の値と一致する。
- [ ] `frontend:build` が `frontend:build-service-worker` に依存し、Service Worker の成果物をフロントエンドの配信物に含める。
- [ ] CI の Linux ジョブが `mise run check` を実行して成功する。
- [ ] 本番の直接の依存が ADR-025 の表に収まっている。
- [ ] テストは、ビルドとタスクの実行を確かめる。振る舞いを持つコードがまだないため、PBT と Fuzzing の対象はこの issue では作らない。

## 解決方法

リポジトリのルートに `Cargo.toml`、`rust-toolchain.toml`、`mise.toml`、`.gitignore`、`.github/workflows/ci.yml` を置く。
`Cargo.toml` の `[workspace]` に `backend/warrior`、`backend/warrior_core`、`backend/pbt`、`frontend`、`service-worker` を並べる。
`backend/warrior_core/src/lib.rs` に health 用の定数と、その値を確かめる単体テストを置く。
`backend/warrior/src/lib.rs` は空の Worker の入口を持つ。
`backend/pbt/tests/` には、0011 で初期の性質テストを追加する。
0001 の時点では、クレートのビルドと `mise run check` の完走だけを確かめる。
`frontend/src/main.rs` は 1 つの経路だけを持つ最小の Dioxus アプリにする。
`service-worker/src/lib.rs` は 1 つのイベントを受け取る最小の関数を持つ。
`mise.toml` の `[tasks]` に `setup`、`fmt`、`lint`、`test`、`test-pbt`、`build-frontend`、`build-service-worker`、`build`、`check` を定義する。
`.github/workflows/ci.yml` は `jdx/mise-action` でツールを入れ、`mise run check` を実行する。
`CHANGES.md` は、利用者に見える変更を出す issue が `## develop` に追記する。
最初の利用者向けの変更を出す 0015 が `CHANGES.md` を作り、担当者行は `@Shoichi` とする。
後続の issue は、利用者に見える変更を出す場合、完了条件に `CHANGES.md` の追記を含める。
