# 経路ごとに利用者または IP で数えるレート制限を実装する

- Requirements: REQ-NFR-017
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-rate-limiting
- Completed: 2026-11-17

## 目的

認証、パスキー、AI、プッシュ購読の経路に、経路ごとに利用者ごとまたは IP ごとの上限を設ける。
上限を超えた要求を 429 で拒否し、D1 のカウンタを Cron Triggers で削除する。

## 優先度根拠

認証と AI の経路は、短時間の大量の要求でコストと安全性が崩れる。
上限の仕組みを後から足すと、経路ごとに数え方がばらける。

## 現状

0006 の HTTP 基盤は Origin の検証とセキュリティヘッダを持つが、レート制限がない。
D1 のカウンタの表も、Cron Triggers の期限切れの削除もない。

## 設計方針

レート制限は固定の時間窓のカウンタで行う。
窓の開始は、現在時刻を窓の秒数で割った商に窓の秒数を掛けた値とする。
カウンタは D1 の表に置き、`INSERT ... ON CONFLICT DO UPDATE` と `SELECT` を `batch()` でまとめ、現在の回数を得る。
読み取りと更新を別の要求に分けず、1 つの取引の中で現在の回数を得る。

表は 2 つに分ける。

- `user_rate_limits`: 認証済みの利用者のカウンタ。`user_id`、`kind`、`window_start`、`count` を持つ。
- `ip_rate_limits`: 認証の開始とコールバック、パスキーの登録とログインのカウンタ。`ip`、`kind`、`window_start`、`count` を持つ。

認証の開始とコールバック、パスキーの登録とログインは IP ごとに数える。
AI とプッシュ購読は利用者ごとに数える。
IP は `CF-Connecting-IP` ヘッダから取る。
`X-Forwarded-For` は使わない。
`CF-Connecting-IP` がない要求は、レート制限の対象の経路では 400 で拒否する。

上限と窓の秒数は環境ごとの vars に置く。
初期値は次のとおりとし、個人利用の実測で調整する。

| 対象 | キー | 上限 | 窓 |
| --- | --- | --- | --- |
| 認証の開始とコールバック | `RATE_LIMIT_AUTH_IP` | 10 回 | 60 秒 |
| パスキーの登録とログイン | `RATE_LIMIT_PASSKEY_IP` | 20 回 | 60 秒 |
| AI の生成の要求 | `RATE_LIMIT_AI_USER` | 20 回 | 86400 秒 |
| プッシュ購読の登録と解除 | `RATE_LIMIT_PUSH_USER` | 30 回 | 60 秒 |

上限を超えた場合は 429 を返し、`Retry-After` に次の窓までの秒数を付ける。
応答は 0006 のエラーの形に合わせ、コードを `rate_limited` とする。

この issue で、Cron Triggers の `scheduled` の入口と、期限切れの行を削除するジョブを作る。
削除の対象は、期限切れのセッション、チャレンジ、OAuth の状態、利用者ごとと IP ごとのレート制限のカウンタとする。
週と月の評価の生成と、Queue の lease の回復は、後続の issue がこの入口にジョブを追加する。

## 完了条件

- [ ] 認証の開始とコールバックが IP ごとに数えられ、上限を超えると 429 になる。これは REQ-NFR-017 の受入基準を支える安全性の条件である。
- [ ] パスキーの登録とログインが IP ごとに数えられ、上限を超えると 429 になる。
- [ ] 認証済みの AI の要求が利用者ごとに数えられ、上限を超えると 429 になる。
- [ ] プッシュ購読の登録と解除が利用者ごとに数えられ、上限を超えると 429 になる。
- [ ] 429 の応答に `Retry-After` が付き、次の窓までの秒数を示す。
- [ ] 窓が変わるとカウンタが 0 から始まる。
- [ ] 同じ利用者の別の端末からの要求が、同じ利用者のカウンタを共有する。
- [ ] 別の利用者のカウンタが混ざらない。
- [ ] `CF-Connecting-IP` がない要求が 400 になる。
- [ ] `X-Forwarded-For` の値で IP を決めない。
- [ ] Cron Triggers の `scheduled` が期限切れのセッション、チャレンジ、OAuth の状態、レート制限のカウンタを削除する。
- [ ] 上限と窓の秒数が環境ごとの vars から読まれ、コードにリテラル値がない。
- [ ] 単体テストで、窓の開始の計算、上限の境界、`Retry-After` の値を確かめる。
- [ ] 結合テストで、上限の直前と直後、窓の切り替わり、利用者と IP の分離を確かめる。
- [ ] PBT で、任意の時刻に対して窓の開始が窓の秒数の倍数になり、同じ窓の時刻が同じ開始になることを確かめる。
- [ ] 利用者ごとのレート制限の表の行が、アカウントの削除の対象の一覧に含まれる。IP ごとのレート制限の表の行は、利用者に紐づかないため、アカウントの削除の対象に含まれない。

## 解決方法

`backend/warrior/src/rate_limit.rs` に、窓の計算、カウンタの更新、429 の応答を置く。
`backend/warrior/src/router.rs` の認証、パスキー、AI、プッシュの経路にレート制限を適用する。
`backend/warrior/src/scheduled.rs` に、Cron Triggers の入口と期限切れの削除のジョブを置く。
`migrations/` に `user_rate_limits` と `ip_rate_limits` の表を追加する。
`backend/warrior/tests/rate_limit.rs` に結合テストを追加する。
`backend/pbt/tests/rate_limit.rs` に窓の性質テストを置く。
`wrangler.toml` の vars に上限と窓の秒数を追加する。
`backend/warrior_core/src/account_deletion.rs` の表の一覧に `user_rate_limits` を追加する。`ip_rate_limits` は利用者に紐づかないため、アカウントの削除の対象にしない。

## 結果

認証とパスキーの IP ごとの制限に加え、プッシュ購読の登録と解除に利用者ごとの制限を追加した。
`Kind::PushUser` を `backend/warrior/src/push.rs` の作成と解除の入口で確認する。
結合テスト `the_push_subscription_requests_are_rate_limited_per_user` で 429 と `Retry-After` を確認した。
既存の `rate_limit` の結合テストと PBT も通過している。
