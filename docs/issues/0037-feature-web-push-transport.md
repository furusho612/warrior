# Web Push の送信を実装する

- Requirements: REQ-TMR-009
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-web-push-transport

## 目的

レスト終了のプッシュ通知を送るため、VAPID の ES256 の JWT と aes128gcm の暗号化を実装する。
ブラウザのプッシュサービスに通知を送れるようにする。

## 優先度根拠

プッシュ通知の予約は 0066 で行うが、送信の仕組みがないと通知を届けられない。
ADR-025 は Web Push を自前で実装すると決めている。

## 現状

0036 でプッシュ購読と Service Worker の通知表示を実装するが、購読へ通知を送る処理がない。
Web Push の VAPID の JWT と aes128gcm の暗号化の実装もない。

## 設計方針

Web Push の送信を、`backend/warrior_core/src/web_push.rs` に純粋なロジックとして置く。
送信は、endpoint、p256dh、auth、payload、Urgency、TTL、VAPID の鍵を受け取る。
キューとデータベースには触れない。
予約と送信のタイミングは 0066 が決める。

VAPID は ES256 の JWT とする。
ヘッダは `typ` を `JWT`、`alg` を `ES256` とし、`kid` に VAPID の公開鍵を置く。
payload は `aud` にプッシュサービスの origin、`exp` に有効期限、`sub` に subject を置く。
JWT は VAPID の秘密鍵で署名し、公開鍵で検証できることを確かめる。

本文は RFC 8291 の aes128gcm で暗号化する。
p256 の ECDH で共有鍵を作り、HKDF-SHA256 で鍵と nonce を導出し、AES-128-GCM で暗号化する。`p256` の `ecdh` feature を使う。
`p256`、`sha2`、`hkdf`、`aes-gcm` を使い、汎用の Web Push のクレートは追加しない。

`Urgency` は `high` とする。
`TTL` は呼び出し側から受け取る。
REQ-TMR-009 の「緊急度を最高にして送る」「有効期限を短くする」を、送信の要求の値として満たす。

## 完了条件

- [ ] VAPID の JWT が公開鍵で検証でき、ヘッダと payload が一致する。これは REQ-TMR-009 の受入基準のうち、緊急度を最高にして送るための送信の要求を満たす。
- [ ] 送信の要求の `Urgency` が `high` であり、`TTL` が呼び出し側の値になる。
- [ ] aes128gcm が RFC 8291 のテストベクタで復号して一致する。
- [ ] 鍵の導出と暗号化が、p256、sha2、hkdf、aes-gcm だけで実装されている。
- [ ] 汎用の Web Push のクレートを追加しない。
- [ ] PBT で、VAPID の JWT の payload が往復し、任意の payload と鍵に対して暗号化と復号が一致することを確かめる。
- [ ] 例示テストで、RFC 8291 のテストベクタ、`exp` の境界、`aud` の不一致、`Urgency` と `TTL` の値を確かめる。
- [ ] Fuzzing の対象を 0065 で追加する。この issue では暗号の入力の型検査までを置く。

## 解決方法

`backend/warrior_core/src/web_push.rs` に、VAPID の JWT の組み立てと検証、aes128gcm の暗号化と復号を置く。
`backend/warrior_core/src/lib.rs` から `pub mod web_push;` で公開する。
`backend/pbt/tests/web_push.rs` に性質テストを置く。
`backend/warrior_core/src/web_push.rs` の `#[cfg(test)]` に RFC 8291 のテストベクタと境界の例示テストを置く。
0066 は、このモジュールの送信の関数を呼ぶ。
