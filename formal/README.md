# 形式手法

認証、レスト通知、記録中のトレーニングの編集権、端末内の送信待ちの列の状態遷移を TLA+ で書き、TLC で不変条件を検査する (ADR-024)。
対象は `Auth.tla`、`RestNotification.tla`、`TrainingOwnership.tla`、`OfflineSync.tla` の 4 つである。

## 実行方法

- `mise run formal` で 4 つのモデルの TLC を実行する。`mise run check` にも含まれる。
- JDK は `mise.toml` の `[tools]` の `java` (Temurin 25) を使う。
- `tla2tools.jar` は mise のレジストリに無いため、タスクが版 (v1.7.4) と SHA-256 を固定して GitHub のリリースから `formal/.cache/` に取得する。jar はリポジトリに含めない (`.gitignore` の対象)。
- TLC のメタディレクトリは `formal/.cache/states` に置き、成功時は `-cleanup` で消える。
- チェックサムが一致しない場合はタスクが失敗する。`formal/.cache/tla2tools.jar` を削除して再実行する。
- チェックサムの確認に使う `shasum` が無い場合は、その旨のメッセージで失敗する。

## 定数

状態空間を小さくし、TLC が短時間で終わるようにする。

| モデル | 定数 |
| --- | --- |
| `Auth` | 利用者 2、チャレンジ 2、OAuth の状態 2、セッション 2、パスキー 2 |
| `RestNotification` | 端末 2、予約 2、メッセージ 2、時刻 2 |
| `TrainingOwnership` | 端末 2、操作 2 |
| `OfflineSync` | 端末 2、操作 2 |

## 認証

`Auth.tla` は、パスキーのチャレンジ、Google OAuth の状態、セッション、パスキーの追加と削除をモデル化する。

| 不変条件 | 内容 | 対応する要求 |
| --- | --- | --- |
| `ChallengeAtMostOnce` | 1 つのチャレンジは最大 1 回しか結果を生まない | REQ-NFR-006、REQ-NFR-013 |
| `OAuthAtMostOnce` | 1 つの OAuth の状態は最大 1 回しか結果を生まない | REQ-NFR-006 |
| `ExpiredChallengesRejected` | 期限切れのチャレンジは結果を生まない | REQ-NFR-013 |
| `ExpiredOAuthRejected` | 期限切れの OAuth の状態は結果を生まない | REQ-NFR-006 |
| `NoInvalidUse` | ログアウトしたセッションは要求を通さない | REQ-NFR-006 |
| `SessionsAreNotReused` | 有効なセッションと終了したセッションは重ならない | REQ-NFR-006 |
| `GoogleLoginPreserved` | パスキーを削除しても Google アカウントでのログインを失わない | REQ-NFR-014 |

`ConsumeChallenge` と `ConsumeOAuth` は結果の回数を増やす。ガードを外すと回数が 2 以上になる。
`Logout` はセッションを終了し、`UseSession` は終了したセッションでは通さない。
`DeletePasskey` はパスキーだけを削除し、Google アカウントの集合を変えない。

## レスト通知

`RestNotification.tla` は、レストの予約、Queue の遅延、取り消し、再予約、古いメッセージ、通知の送信をモデル化する。

| 不変条件 | 内容 | 対応する要求 |
| --- | --- | --- |
| `AtMostOneNotification` | 1 つの予約につき通知は最大 1 回である | REQ-TMR-009 |
| `NoInvalidSend` | 取り消し、終了、期限切れ、古い終了予定時刻の通知を送らない | REQ-TMR-009、REQ-TMR-010、REQ-TMR-011 |
| `LatestEndUsed` | 送信した通知は、送信時点の最新の終了予定時刻を使う | REQ-TMR-010 |

`Send` は、重複、取り消し、終了、期限切れ、古い終了予定時刻の 5 つのガードを通る。
`sentWhen` は送信時点の終了予定時刻を記録し、`invalidSends` はガードを外して送った通知を記録する。

## 編集権と引き継ぎ

`TrainingOwnership.tla` は、記録中のトレーニングの編集権、開始した端末、サーバーに保存済みの操作、未同期の操作、引き継ぎ、終了後の編集をモデル化する。

| 不変条件 | 内容 | 対応する要求 |
| --- | --- | --- |
| `AtMostOneEditor` | 記録中に編集できる端末は 1 台以下である | REQ-NFR-015 |
| `HandoffFromSaved` | 引き継ぎの元はサーバーに保存済みの操作だけである | REQ-NFR-015 |
| `LastWriteWins` | サーバーの内容と最後に受理した書き込みが一致する | REQ-NFR-015 |
| `AfterFinishAllCanEdit` | 終了後は同じアカウントのどの端末も編集できる | REQ-NFR-015 |
| `AcceptedWritesSurvive` | 受理済みの操作は、同じ操作の後勝ちの上書き以外では残る | REQ-NFR-015 |

## オフライン同期

`OfflineSync.tla` は、端末内の送信待ちの列、送信、受理、再送、重複、サーバーへの適用をモデル化する。

| 不変条件 | 内容 | 対応する要求 |
| --- | --- | --- |
| `NoDuplicateSets` | 同じ操作を複数回適用しても、サーバーの適用回数は 1 回以下である | REQ-NFR-003、REQ-NFR-005 |
| `AcceptedApplied` | 受理済みの操作はサーバーに適用されている | REQ-NFR-005 |
| `QueuedIsLocal` | 送信待ちの操作は端末内の記録に含まれる | REQ-NFR-003 |
| `UnsyncedNotOverwritten` | 未同期の操作をサーバーの内容で上書きしない | REQ-NFR-003、REQ-NFR-005 |

## 検査が機能していることの確認

ガードを外したコピーをリポジトリの外に置き、TLC が違反を検出することを確かめた。

| 変更 | TLC の結果 |
| --- | --- |
| `Handoff` で編集権を置き換えずに追加する | `Invariant Inv is violated`（`editors = {d1, d2}`） |
| `Handoff` で未同期の操作も引き継ぐ | `Invariant Inv is violated`（`handoffSource = {o1}`、`accepted = {}`） |
| `Send` の重複ガードを外す (OfflineSync) | `Invariant Inv is violated`（`counts[o1] = 2`） |
| `ConsumeChallenge` の消費済みガードを外す | `Invariant Inv is violated`（`challengeResults[c1] = 2`） |
| `ConsumeOAuth` の消費済みガードを外す | `Invariant Inv is violated` |
| `UseSession` のログアウト済みガードを外す | `Invariant Inv is violated` |
| `DeletePasskey` で Google アカウントのログインを失わせる | `Invariant Inv is violated` |
| `Send` の取り消しガードを外す | `Invariant Inv is violated` |
| `Send` の古い終了予定時刻のガードを外す | `Invariant Inv is violated` |

通常の 4 つの仕様は `mise run formal` で成功する。
