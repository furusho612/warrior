# 形式手法

記録中のトレーニングの編集権と、端末内の送信待ちの列の状態遷移を TLA+ で書き、TLC で不変条件を検査する (ADR-024)。
対象は `TrainingOwnership.tla` と `OfflineSync.tla` の 2 つである。

## 実行方法

- `mise run formal` で TLC を実行する。`mise run check` にも含まれる。
- JDK は `mise.toml` の `[tools]` の `java` (Temurin 25) を使う。
- `tla2tools.jar` は mise のレジストリに無いため、タスクが版 (v1.7.4) と SHA-256 を固定して GitHub のリリースから `formal/.cache/` に取得する。jar はリポジトリに含めない (`.gitignore` の対象)。
- TLC のメタディレクトリは `formal/.cache/states` に置き、成功時は `-cleanup` で消える。
- チェックサムが一致しない場合はタスクが失敗する。`formal/.cache/tla2tools.jar` を削除して再実行する。
- チェックサムの確認に使う `shasum` が無い場合は、その旨のメッセージで失敗する。

## 定数

状態空間を小さくし、TLC が短時間で終わるようにする。

| モデル | 定数 |
| --- | --- |
| `TrainingOwnership` | 端末 2 台 (`d1`、`d2`)、操作 2 つ (`o1`、`o2`) |
| `OfflineSync` | 端末 2 台、操作 2 つ |

## 編集権と引き継ぎ

`TrainingOwnership.tla` は、記録中のトレーニングの編集権、開始した端末、サーバーに保存済みの操作、未同期の操作、引き継ぎ、終了後の編集をモデル化する。

| 不変条件 | 内容 | 対応する要求 |
| --- | --- | --- |
| `AtMostOneEditor` | 記録中に編集できる端末は 1 台以下である | REQ-NFR-015 |
| `HandoffFromSaved` | 引き継ぎの元はサーバーに保存済みの操作だけである | REQ-NFR-015 |
| `LastWriteWins` | サーバーの内容と最後に受理した書き込みが一致する | REQ-NFR-015 |
| `AfterFinishAllCanEdit` | 終了後は同じアカウントのどの端末も編集できる | REQ-NFR-015 |
| `AcceptedWritesSurvive` | 受理済みの操作は、後から同じ操作を上書きしない限りサーバーに残る | REQ-NFR-015 |

`Write` は端末内の未同期の列に操作を置き、`Sync` が編集権を持つ端末だけからサーバーへ適用する。
`Handoff` は編集権を移し、未同期の操作を引き継がない。
`Finish` はすべての端末を編集可能にし、`EditAfterFinish` は後勝ちでサーバーの内容を更新する。

## オフライン同期

`OfflineSync.tla` は、端末内の送信待ちの列、送信、受理、再送、重複、サーバーへの適用をモデル化する。

| 不変条件 | 内容 | 対応する要求 |
| --- | --- | --- |
| `NoDuplicateSets` | 同じ操作を複数回適用しても、サーバーの適用回数は 1 回以下である | REQ-NFR-003、REQ-NFR-005 |
| `AcceptedApplied` | 受理済みの操作はサーバーに適用されている | REQ-NFR-005 |
| `QueuedIsLocal` | 送信待ちの操作は端末内の記録に含まれる | REQ-NFR-003 |
| `UnsyncedNotOverwritten` | 未同期の操作をサーバーの内容で上書きしない | REQ-NFR-003、REQ-NFR-005 |

`Send` は受理とサーバーへの適用を同じ操作で行い、`Ack` が端末内の列から操作を外す。
`Retry` は列に残る操作を再送する。`Send` の重複ガードを外すと `NoDuplicateSets` が違反する。

## 検査が機能していることの確認

ガードを外したコピーをリポジトリの外に置き、TLC が違反を検出することを確かめた。

| 変更 | TLC の結果 |
| --- | --- |
| `Handoff` で編集権を置き換えずに追加する | `Invariant Inv is violated`（`editors = {d1, d2}`） |
| `Handoff` で未同期の操作も引き継ぐ | `Invariant Inv is violated`（`handoffSource = {o1}`、`accepted = {}`） |
| `Send` の重複ガードを外す | `Invariant Inv is violated`（`counts[o1] = 2`） |

通常の仕様は `mise run formal` で成功する。
