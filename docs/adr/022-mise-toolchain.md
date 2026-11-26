# ADR-022: ツールチェーンを mise で固定する

- Status: Accepted

## Context

手元と CI で同じツールの版と同じコマンドラインを使う必要がある。
参照する `../../../brewbook` は、Rust、`worker-build`、Node.js、`wrangler`、Dioxus CLI、`wasm-bindgen`、ChromeDriver、JDK の版を `mise.toml` で固定し、ビルド、テスト、デプロイのコマンドラインを `mise` のタスクに置いている。
バックエンドは Rust と `worker-build` で wasm にビルドし、フロントエンドは Dioxus CLI の `dx` でビルドする。
テストと E2E は Chrome と Safari の WebDriver、PBT は `proptest`、Fuzzing は `cargo-fuzz`、形式手法は TLA+ と TLC を使う。
TLC の実行には JDK が要る。

## Decision

リポジトリのルートに `mise.toml` を置き、ツールの版とコマンドラインを管理する。
Rust は 1.99.0、`wasm32-unknown-unknown` のターゲット、clippy と rustfmt を含める。
`worker-build` は 0.8.6、Node.js は 24.21.0、`wrangler` は 4.135.0、Dioxus CLI は 0.7.10、`wasm-bindgen` はテストランナー用に 0.2.129、ChromeDriver は 157.0.8103.15、JDK は Temurin 25 とする。
ビルド用の `wasm-bindgen` は Dioxus CLI が取得する版に従う。
Safari と `safaridriver` は macOS に付属するものを使い、mise では固定しない。
`cargo-fuzz` は 0.13.2 とし、`cargo:cargo-fuzz` で入れる。
`ripgrep` は lint の絵文字と禁止語の検査に使い、15.2.0 とする。
版は `latest` にせず、更新は ADR で記録する。
Cloudflare のビルド環境が持つグローバル設定のツールを読み込まないよう、`auto_install` を無効にし、ビルドに必要なツールはビルドコマンドで明示的にインストールする。

`mise` のタスクに、セットアップ、整形、lint、単体テスト、PBT、Fuzzing の型検査、結合テスト、ブラウザのテスト、E2E、形式手法、ビルド、Service Worker のビルド、マイグレーション、デプロイを置く。
`frontend:build-service-worker` を `frontend:build` の依存に置く。
CI も同じ `mise.toml` でツールを入れ、同じタスクを実行する。
Safari の E2E は macOS の runner で実行する。

タスクの内容は Rust と Dioxus の標準のコマンドの組み合わせに留める。
ロジックが必要な場合は Rust のコードにして、タスクからはそれを呼ぶだけにする。
依存の追加は、ADR または issue に理由を記録する。

## Consequences

手元と CI のツールの版が 1 か所で決まり、ビルドを再現できる。
開発者は `mise` を入れることが前提になる。
Safari の E2E には macOS の runner が要る。
Chrome の E2E は Linux の runner で実行できる。

`wrangler` は npm のパッケージであるため、mise の `[tools]` に Node.js を置く。
Node.js は `wrangler` のインストールにだけ使い、アプリのコードには使わない。

Dioxus CLI は Tailwind CLI、esbuild、`wasm-bindgen`、`wasm-opt` を自身の固定した版で取得する。
これらの版は Dioxus CLI の版が決めるため、mise では個別に固定しない。
`wasm-bindgen-test` のランナーは、`frontend/Cargo.lock` の `wasm-bindgen` と版を揃える。
Safari のブラウザのテストは `safaridriver` を起動して実行する。

ツールの版を更新するときは、`mise.toml` の変更をコミットし、`mise run check` が通ることを確認してからマージする。

## References

- `../../../brewbook/docs/adr/0009-mise-toolchain-and-tasks.md`
- `../../../brewbook/mise.toml`
- `docs/prd/warrior.md` の「非機能要件」
