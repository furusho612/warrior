# Worker の HTTP 基盤とセキュリティヘッダを用意する

- Requirements: REQ-NFR-009
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-server-foundation

## 目的

Worker の経路、API のエラーの形、リクエストのログ、セキュリティヘッダ、同一オリジンの検証を 1 か所に揃える。
後続の API が同じ形で実装できるようにする。

## 優先度根拠

認証、記録、AI、プッシュのすべての API がこの基盤の上に載る。
ヘッダと Origin の検証を後から足すと、経路ごとに漏れが出る。

## 現状

`wrangler.toml` と D1 の基盤は 0002 と 0004 で用意するが、Worker の経路、エラーの形、ログ、セキュリティヘッダ、Origin の検証がない。
そのため、後続の API がそれぞれ別の形で応答とログを実装する状態になる。

## 設計方針

`worker` クレートの `Router` を使い、`/api/*` の経路を 1 か所で登録する。
この issue では `GET /api/health` だけを実装する。
未知の経路は 404、許可しないメソッドは 405 を返す。

API のエラーは、コードと英語のメッセージを持つ JSON に統一する。

```json
{"error":{"code":"unauthorized","message":"Authentication is required."}}
```

画面に出す文言は、アプリの日本語のキーの表でコードから引く。
サーバーのログとエラーメッセージは英語にする。

リクエストごとに、経路名、メソッド、状態コード、処理時間、エラーコード、リクエスト ID を JSON で記録する。
身体情報、メールアドレス、メモ、AI の入出力、トークン、プッシュ購読の情報は記録しない。
リクエスト ID はリクエストごとに生成し、応答のヘッダにも付ける。

セキュリティヘッダを Worker の応答と静的ファイルの応答の両方に付ける。
CSP は同じオリジンのスクリプトと wasm の実行に必要な `'wasm-unsafe-eval'`、フォントの配信元の `fonts.googleapis.com` と `fonts.gstatic.com` だけを許可し、外部スクリプトを許可しない。
HSTS、X-Content-Type-Options、Referrer-Policy、Permissions-Policy を付ける。
CORS のヘッダは返さない。

状態を変更するリクエストは `Origin` ヘッダを検証し、環境ごとの vars の Origin と一致しない要求を 403 で拒否する。
`GET` と `HEAD` は Origin の検証をしない。
認証とレート制限は後続の issue で加える。

## 完了条件

- [ ] `GET /api/health` が 200 と JSON を返す。これは PRD の要求 ID を持たない基盤の条件である。
- [ ] 未知の経路が 404、許可しないメソッドが 405 を返し、どちらも同じ形のエラー JSON になる。
- [ ] API の応答と静的ファイルの応答に、CSP、HSTS、X-Content-Type-Options、Referrer-Policy、Permissions-Policy が付く。
- [ ] CSP が `'wasm-unsafe-eval'` と Google Fonts の 2 つのホストだけを許可し、外部スクリプトを許可しない。
- [ ] 状態を変更するリクエストで、`Origin` が vars の値と一致しないとき 403 を返す。一致するときは通過する。これは REQ-NFR-009 の受入基準「すべての通信が暗号化された経路で行われる」を支える同一オリジンの検証である。この issue では同一オリジンの検証とヘッダをローカルで確かめる。HTTPS と実環境の暗号化は 0067 の staging で確かめる。
- [ ] CORS のヘッダを返さない。
- [ ] リクエストのログが JSON で、経路名、メソッド、状態コード、処理時間、エラーコード、リクエスト ID を含む。
- [ ] ログに身体情報、メールアドレス、メモ、AI の入出力、トークン、プッシュ購読の情報が出ない。
- [ ] ヘッダの組み立てと Origin の検証を単体テストで確かめる。経路の応答は `wrangler dev` の結合テストで確かめる。
- [ ] 状態を持たないヘッダと経路の処理に PBT は使わない。入力の組合せが固定であるため、例示テストと結合テストで足りる。

## 解決方法

`backend/warrior/src/lib.rs` に `fetch` の入口を置く。
`backend/warrior/src/router.rs` に `Router` の登録と `/api/health` を置く。
`backend/warrior/src/error.rs` にエラーのコードと JSON の変換を置く。
`backend/warrior/src/log.rs` に JSON のログの組み立てを置く。
`backend/warrior/src/security.rs` にセキュリティヘッダと Origin の検証を置く。
`frontend/public/_headers` に静的ファイル用のヘッダを置く。`wrangler dev` で反映を確かめる。
`backend/warrior/tests/` に `wrangler dev` を起動する結合テストを置く。
`backend/pbt/tests/` にはこの issue の対象を置かない。
