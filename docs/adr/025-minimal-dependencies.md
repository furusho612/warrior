# ADR-025: 依存を最小限にし、汎用クレートの一部だけを使う場合は自前で実装する

- Status: Accepted

## Context

バックエンドは Cloudflare Workers の `wasm32-unknown-unknown` で動く。
フロントエンドは Dioxus の wasm で動く。
wasm のビルドでは、OpenSSL や `ring` のように wasm32 でビルドできないクレートがある。
依存が増えると、ビルドの失敗、wasm のサイズの増加、監査の負担が増える。
参照する `../../../brewbook` は、乱数、base64url、ISO 8601、UUID、WebAuthn の CBOR と COSE を自前で実装し、`uuid`、`rand`、`getrandom`、`chrono`、`time`、`base64`、`webauthn-rs` を追加していない。
`../../../brewbook` は、D1 に ORM を使わず、R2 の操作に S3 のクレートを使わず、HTTP のルーティングに axum を使わない。
一方で、暗号の検証には `p256` と `sha2` を使う。
プロジェクトの規約は、依存を最小限にし、新規の依存は著名な組織が保守するライブラリに限り、採否を利用者が決めることを求める。
利用者は、汎用性の高いクレートの一部だけを使う場合に、その部分を自前で実装することを求めた。

## Decision

本番のビルドに含めるクレートを、次の表のものに限る。
表にないクレートを追加するときは、目的、標準ライブラリやプラットフォームの API で代替できない理由、推移的な依存の数、wasm のビルドの結果、wasm のサイズへの影響を、ADR または issue に記録する。

### 本番の依存

| クレート | 用途 | 理由 |
| --- | --- | --- |
| `worker` | Cloudflare Workers の SDK | HTTP、D1、R2、Workers AI、Queues、Cron、Fetch、Date、ログをまとめて扱う。公式の SDK であり、代替がない |
| `serde`、`serde_json` | JSON の符号化と復号 | API と D1 の値を扱う。`worker` のバインディングも Serde を使う |
| `wasm-bindgen`、`js-sys`、`wasm-bindgen-futures` | wasm と JavaScript の相互運用 | ブラウザと Workers の API を呼ぶために必要である |
| `web-sys` | ブラウザの API | fetch、IndexedDB、WebAuthn、Push、Service Worker、DOM を使う。使う API の feature だけを有効にする |
| `dioxus`、`dioxus-router` | 画面と経路 | フロントエンドのフレームワークとルーターである。状態管理や CSS の汎用クレートは追加しない |
| `p256` | ES256 の署名の検証と生成 | WebAuthn と Web Push の VAPID に必要である。暗号は自前で実装しない |
| `sha2` | SHA-256 | WebAuthn、セッションのハッシュ、Web Push に必要である。暗号は自前で実装しない |
| `hkdf` | HKDF-SHA256 | Web Push の鍵の導出に必要である。暗号は自前で実装しない |
| `aes-gcm` | AES-128-GCM | Web Push の本文の暗号化に必要である。暗号は自前で実装しない |

`worker`、`dioxus`、`wasm-bindgen`、`web-sys` の推移的な依存は、これらの選択に含まれる。
直接の依存だけを管理し、推移的な依存はフレームワークと SDK の更新で追う。

### 自前で実装するもの

汎用クレートの一部だけを使う場合は、その部分を自前で実装する。
実装した部分には、単体テスト、PBT、Fuzzing を付ける。

| 機能 | 自前で実装する理由 | 参照する実装 |
| --- | --- | --- |
| base64url | WebAuthn と Web Push が使う文字集合とパディングの規則だけが必要である。汎用の base64 クレートは不要である | `../../../brewbook/backend/brew_book_core/src/base64url.rs` |
| ISO 8601 の日時 | 固定長の UTC の文字列の整形と比較だけが必要である。`chrono` と `time` は不要である | `../../../brewbook/backend/brew_book_core/src/datetime.rs` |
| UUID v4 | 16 バイトの乱数から文字列を作るだけである。`uuid` は不要である | `../../../brewbook/backend/brew_book_core/src/ids.rs` |
| 乱数 | Web Crypto の `crypto.getRandomValues` を呼ぶだけである。`rand` と `getrandom` は不要である | `../../../brewbook/backend/brew_book/src/random.rs` |
| CBOR と COSE | WebAuthn が使う map、bytes、text、integer、array と ES256 の鍵だけを扱う。汎用の CBOR クレートは不要である | `../../../brewbook/backend/brew_book_core/src/cbor.rs`、`cose.rs` |
| WebAuthn の検証 | 登録とログインの検証の範囲だけを扱う。`webauthn-rs` は wasm32 でビルドできない見込みである | `../../../brewbook/backend/brew_book_core/src/webauthn.rs` |
| CSV の書き出し | 1 セット 1 行の固定の列と、セルのエスケープだけを扱う。`csv` は不要である | 実装時に作る |
| Web Push | VAPID の ES256 の JWT と、aes128gcm の暗号化だけを扱う。汎用の `web-push` クレートは不要である | Rust の Web Push の実装例 |
| グラフ | 画面の SVG を Dioxus で描く。グラフのクレートは不要である | 実装時に作る |
| 画面の文言 | 日本語だけの型付きのキーの表を持つ。i18n のクレートは不要である | `../../../brewbook/frontend/src/i18n/` |
| IndexedDB | `web-sys` の API を直接使う。`rexie` と `idb` は不要である | 実装時に作る |

### 追加しないクレート

次のクレートは本番のビルドに追加しない。
代替の手段を使う。

| 追加しないクレート | 代替 |
| --- | --- |
| `axum`、`hyper`、`http` の feature | `worker` の `Router` と `Request`、`Response` |
| `sqlx`、`diesel`、`sea-query` | D1 の API に直接 SQL を渡す |
| `reqwest`、`gloo-net` | `worker::Fetch` と `web-sys` の fetch |
| `chrono`、`time` | `worker::Date` と自前の ISO 8601 |
| `uuid` | 自前の UUID v4 |
| `rand`、`getrandom` | Web Crypto の `crypto.getRandomValues` |
| `base64` | 自前の base64url |
| `csv` | 自前の CSV の書き出し |
| `plotters`、グラフのクレート | Dioxus の SVG |
| `fluent`、`rust-i18n` | 型付きの日本語のキーの表 |
| `stylance`、CSS-in-Rust のクレート | Tailwind CSS と CSS 変数 |
| `webauthn-rs` | 自前の WebAuthn の検証 |
| `jsonwebtoken`、`josekit`、`rsa` | Google OAuth の認可コードフローと UserInfo |
| `web-push` | RustCrypto による VAPID と aes128gcm |
| `shiguredo_s3` | R2 のバインディング。署名付き URL は使わない |
| `rexie`、`idb` | `web-sys` の IndexedDB |
| `tracing`、`log` | `worker` のログと自前の JSON のログ |
| `thiserror`、`anyhow` | 自前のエラーの型 |
| `regex` | 手書きのパーサと `worker::Url` |
| `url` | `worker::Url` |
| `serde-wasm-bindgen` | `web-sys` の型と `serde_json` |
| `console_error_panic_hook` | `worker` のパニックのフック、または最小の自前のフック |
| `once_cell`、`lazy_static` | 標準ライブラリの `OnceLock` と `LazyLock` |
| `itertools` | 標準ライブラリのイテレータ |
| `bytes` | `Vec<u8>` と `&[u8]` |
| `mime` | 固定の文字列の定数 |
| `ring`、`aws-lc-rs`、`openssl` | wasm32 でビルドできる RustCrypto のクレート |

### 例外

暗号、TLS、証明書の検証、パスワードのハッシュ、定数時間の比較のように、安全性を自前の実装で担保できないものは、この規則の例外とする。
著名な組織が保守する、監査されたクレートを使う。
本プロジェクトでは、`p256`、`sha2`、`hkdf`、`aes-gcm` をこの例外として使う。

テスト専用のクレートは、開発の依存に限って追加する。
本番の wasm に含めない。
対象は、`proptest`、`libfuzzer-sys`、`wasm-bindgen-test`、`thirtyfour` である。
テストのために本番のコードへ分岐を追加しない。
テスト専用のサービスは別のプロセスに置く。

### 依存の追加と見直し

依存を追加する変更では、`cargo tree --edges normal` と `cargo tree --target wasm32-unknown-unknown` の差分をレビューに含める。
`web-sys` の feature、`worker` の feature、`dioxus` の feature は、使うものだけを列挙する。
`worker` は `d1` と `queue` だけを有効にする。
`dioxus` は `default-features = false` とし、`web`、`minimal`、`document` だけを有効にする。
`p256` は `default-features = false` とし、`ecdsa` だけを有効にする。
使わなくなった依存は削除する。
自前で実装した部分が小さな範囲を超えて大きくなった場合は、焦点を絞ったクレートの採用を、別の ADR で検討する。

## Consequences

wasm が小さくなり、ビルドの失敗が減り、依存の監査が簡単になる。
一方で、自前で実装するコードが増え、そのコードのテストと保守の責任を負う。

暗号とセッションのハッシュは自前で実装しない。
`p256`、`sha2`、`hkdf`、`aes-gcm` の更新と脆弱性の情報は追う。

自前で実装した base64url、ISO 8601、UUID、CBOR、COSE、Web Push は、PBT と Fuzzing の対象にする。
WebAuthn の検証は、仕様の該当節を引用したコメントと、公開されているテストベクタで確かめる。

汎用のクレートを追加しないぶん、実装の初速は落ちる。
そのぶん、wasm のサイズとビルドの再現性が上がる。

テスト専用のクレートは本番の wasm に含めない。
テストのための分岐を本番のコードに置かない。

## References

- `../../../brewbook/docs/adr/0001-backend-rust-on-cloudflare-workers.md`
- `../../../brewbook/docs/adr/0004-passkey-only-authentication.md`
- `../../../brewbook/backend/brew_book_core/src/base64url.rs`
- `../../../brewbook/backend/brew_book_core/src/datetime.rs`
- `../../../brewbook/backend/brew_book_core/src/ids.rs`
- `../../../brewbook/backend/brew_book/src/random.rs`
- `../../../brewbook/backend/brew_book_core/src/cbor.rs`
- `../../../brewbook/backend/brew_book_core/src/cose.rs`
- `../../../brewbook/backend/brew_book_core/src/webauthn.rs`
- `docs/prd/warrior.md` の「非機能要件」
- `~/.pi/agent/AGENTS.md` の「原則」
