# プッシュ通知の購読と Service Worker の通知表示を実装する

- Requirements: REQ-TMR-008, REQ-NFR-008
- Priority: High
- Created: 2026-11-12
- Completed: 2026-11-15
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-push-subscription

## 目的

利用者が許可した場合に、端末のプッシュ購読を作り、サーバーに保存する。
Service Worker がプッシュ通知を受け取り、レストの終了を表示する。
iOS でホーム画面に追加していない場合に、追加の手順を案内する。

## 優先度根拠

プッシュ通知は段階 1 で試作し、採用は staging の実機計測 (0038) の後に決める。
購読と表示の経路がないと、遅延の計測と採用の判断ができない。

## 現状

0020 で PWA の manifest と Service Worker のキャッシュ、0034 でレストタイマーを実装する。
しかし、プッシュの購読、購読の保存、Service Worker の通知の表示、通知を押したときの画面の移動がない。

## 設計方針

アプリは、認証済みの経路から VAPID の公開鍵を取得する。
利用者が設定でプッシュ通知を有効にしたときに、`PushManager.subscribe` を呼ぶ。
購読の条件は `userVisibleOnly: true` とし、`applicationServerKey` に VAPID の公開鍵を渡す。
画面の読み込み時に許可を求めない。

購読の情報は、endpoint、p256dh、auth、利用者 ID、端末トークンの ID とともにサーバーへ送る。
サーバーは次の条件を検証する。

- endpoint が HTTPS である。
- endpoint のホストが `web.push.apple.com` または `fcm.googleapis.com` である。
- 同じ endpoint の購読が同じ利用者のものとして 1 行になる。

条件を満たさない endpoint は 400 で拒否する。
許可するホストを追加するときは、ADR と許可リストを同じ変更で更新し、許可しないホストが 400 になるテストを追加する。

VAPID の公開鍵と subject は環境ごとの vars に置き、秘密鍵は Secret に置く。
アプリは秘密鍵を持たない。

Service Worker は `push` のイベントを受け取り、通知を表示する。
通知の本文には、種目名、重量、回数などの記録の内容を含めない。
`notificationclick` のイベントで、アプリの画面を開くか、既に開いている画面にフォーカスする。
通知の表示に失敗した場合は、通知を出さずに終了する。レストの音と画面表示は 0034 が別に担う。

iOS でホーム画面に追加していない場合、プッシュ通知が届かないことを説明し、追加の手順を表示する。
案内の文言は 0020 のホーム画面への追加の案内を使う。

ログアウトのときは、その端末の購読を削除する。
アカウントの削除では、利用者の全ての購読を削除する。

## 完了条件

- [ ] プッシュ通知を許可した端末で、画面を消灯した状態またはブラウザを閉じた状態でも、レストの終了がプッシュ通知で届く。これは REQ-TMR-008 の受入基準に対応する。実際の配信は 0067 の staging と 0038 の実機計測で確かめる。この issue の完了条件はローカルの購読と表示までとする。
- [ ] 通知が、そのトレーニングを開始した端末にだけ届くための端末の識別子を、購読とともに保存する。同じアカウントのほかの端末への配信の制御は 0066 が担う。
- [ ] iOS でホーム画面に追加していない場合は通知が届かないため、追加の手順が表示される。これは REQ-TMR-008 の受入基準に対応する。
- [ ] 通知の本文に、種目名、重量、回数などの記録の内容が含まれない。
- [ ] 通知を押すと、アプリの画面が開くか、既に開いている画面にフォーカスする。これは ADR-020 の決定に基づく E2E の対象であり、PRD に対応する要求 ID がない。
- [ ] ブラウザ通知を許可しない状態でも、レストタイマーが音と画面表示で動作する。これは REQ-NFR-008 の受入基準に対応する。
- [ ] 購読の endpoint が HTTPS で、ホストが `web.push.apple.com` または `fcm.googleapis.com` のときだけ受理する。HTTP の endpoint と、許可しないホストの endpoint を 400 で拒否する。
- [ ] 許可するホストを追加するときは、ADR と許可リストを同じ変更で更新する。
- [ ] ログアウトでその端末の購読が削除され、アカウントの削除で利用者の全ての購読が削除される。
- [ ] 結合テストで、endpoint の許可と拒否、同じ endpoint の重複、ログアウトとアカウントの削除を確かめる。
- [ ] E2E で、許可の状態、購読の作成、Service Worker の `push` の表示、通知を押したときの画面の移動を確かめる。`push` のイベントは CDP の `ServiceWorker.deliverPushMessage` で届ける。許可の状態と `push` の表示は `REQ-TMR-008` の受入基準「プッシュ通知を許可した端末で、画面を消灯した状態またはブラウザを閉じた状態でも、レストの終了がプッシュ通知で届く」に対応する。端末の限定は 0066 が担う。通知を押したときの画面の移動は ADR-020 の決定に基づく E2E の対象であり、PRD に対応する要求 ID がない。
- [ ] ローカルのテストは、許可リストと Service Worker の表示を対象にする。実際のプッシュサービスへの購読は 0067 の staging と 0038 の実機計測で確かめる。
- [ ] PBT で、endpoint の検証が、HTTPS で許可されたホストの endpoint だけを受理することを確かめる。

## 解決方法

`migrations/` に `push_subscriptions` の表を追加する。
`backend/warrior_core/src/push.rs` に、endpoint の検証と購読の型を置く。
`backend/warrior/src/push.rs` に、公開鍵の取得、購読の登録と削除を置く。
`backend/warrior/src/router.rs` に `GET /api/push/public-key`、`POST /api/push/subscriptions`、`DELETE /api/push/subscriptions` を追加する。
`service-worker/src/lib.rs` に `push` と `notificationclick` の処理を追加する。
`frontend/src/push.rs` に、購読の作成と解除、iOS の案内を置く。
`frontend/src/screens/mypage.rs` に、プッシュ通知の設定を置く。設定の保存は 0062 が担う。
`backend/warrior/tests/push_subscription.rs` に結合テストを追加する。
`backend/warrior/tests/e2e_push.rs` に E2E を追加する。
`backend/pbt/tests/push.rs` に endpoint の性質テストを置く。
`backend/warrior_core/src/account_deletion.rs` の表の一覧に `push_subscriptions` を追加する。

## 結果

プッシュ通知の購読と Service Worker の通知表示を実装した。`push_subscriptions` を追加し、endpoint は HTTPS かつ `web.push.apple.com` か `fcm.googleapis.com` のときだけ受理する。ログアウトで端末の購読を消し、アカウントの削除で利用者の購読を消す。実際の配信は 0067 と 0038 で確かめる。
