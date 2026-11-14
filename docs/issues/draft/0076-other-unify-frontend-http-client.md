# フロントエンドの HTTP クライアントを 1 つに統合する

- Requirements: なし (挙動を変えないリファクタ)
- Priority: Low
- Created: 2026-11-14
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-unify-frontend-http-client

## 目的

画面ごとに分かれている同じオリジンへの HTTP 要求の組み立てを 1 つにまとめ、ステータスコードの分類を同じ規則で扱う。

## 優先度根拠

挙動は正しく、重複はリファクタで解消できる。記録とカレンダーの機能追加を止めないため、優先度を Low とする。

## 現状

`frontend/src/http.rs` はカレンダーなどが使う汎用の JSON クライアントで、通信不能、拒否、応答の不正だけを表す。
`frontend/src/training_session.rs` は 0023 で追加した 403、404、409 の分岐を持つ独自のクライアントを持つ。
同じ要求の組み立てが 2 か所にあり、ステータスコードの扱いを変えるときに両方を直す必要がある。

## 設計方針

`HttpError` に `Forbidden`、`NotFound`、`Conflict` を追加し、`http::status_error` で HTTP のステータスを分類する。
`TrainingApiError` は `HttpError` からの変換を持ち、画面の分岐は今の識別子を保つ。
`frontend/src/training_session.rs` の要求の組み立てを削除し、`http::request_json` を使う。
403 の応答の本文にあるエラーコードを読み、トレーニングの状態変更と Origin の拒否を区別する。状態変更は「記録の状態が変わりました。読み直してください。」、Origin の拒否は通信の失敗として扱う。
カレンダーの失敗の分岐は、新しい `HttpError` の値で今と同じ表示になることを保つ。

## 完了条件

- [ ] フロントエンドの HTTP 要求の組み立てが `frontend/src/http.rs` の 1 か所だけになる。これは受入基準に対応しない内部のリファクタである。
- [ ] 403、404、409 の分類が、記録画面の引き継ぎとサマリーの表示で今と同じになる。
- [ ] 403 の状態変更と Origin の拒否を、応答のエラーコードで区別する。
- [ ] カレンダーの失敗の表示が今と同じになる。
- [ ] `HttpError` の分類と `TrainingApiError` への変換の単体テストを追加する。
- [ ] 既存のブラウザテスト、E2E、結合テストが通過する。

## 解決方法

`frontend/src/http.rs` の `HttpError` と `request_json` を拡張する。
`frontend/src/training_session.rs` の `request`、`request_body`、`status_error` を削除し、`HttpError` から `TrainingApiError` への変換を置く。
`frontend/src/calendar.rs` と `frontend/src/screens/record/ownership.rs` の分岐を新しい値に合わせる。
`frontend/tests/` に分類のテストを追加する。
