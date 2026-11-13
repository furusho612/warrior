# システム構成

## この文書の役割

この文書は、Warrior の構成を俯瞰する。
要求は `docs/prd/warrior.md`、判断は `docs/adr/`、実装の分担は `docs/issues/` が持つ。
この文書は、それらを読む前に全体の位置関係をつかむための入口であり、ADR の判断を置き換えない。

## 全体像

Warrior は、利用者の端末で動く PWA と、Cloudflare の Worker で動く API から成る。
端末は、記録の入力、画面の描画、端末内の保存、Service Worker のキャッシュを担う。
Worker は、認証、同期、マスターの配信、AI の呼び出し、書き出し、プッシュ通知の予約を担う。
永続化は D1 と R2、非同期処理は Queues と Cron Triggers、AI は Workers AI を使う。

```text
利用者の端末
  PWA (Dioxus + WebAssembly)
  IndexedDB
  Service Worker
        |
        | HTTPS
        v
Cloudflare の Worker
  Static Assets
  API
  Queues の producer と consumer
  Cron Triggers
        |
        +-- D1
        +-- R2
        +-- Workers AI
        +-- Web Push
```

## 端末と配信

画面は Dioxus で作り、WebAssembly として配信する。
スタイルは Tailwind を使い、画面と API を同じオリジンから配信する。
Worker の Static Assets が画面の成果物を返し、`/api/*` を Rust の処理に渡す。
Service Worker は、画面の成果物とアプリの wasm をキャッシュし、オフラインでも記録を続けられるようにする。
Service Worker 自身の wasm とアプリの wasm は別の成果物として扱う。

端末は、記録中の操作を IndexedDB に保存し、送信待ちの列に積む。
通信が復帰すると、列を順にサーバーへ送る。
記録中のトレーニングは、開始した端末が編集を所有し、ほかの端末は閲覧する。
開始した端末が使えない場合は、サーバーに保存済みの内容で記録を引き継ぐ。

## API と認証

認証は Google の OAuth とパスキーで行う。
ログインの後に、マイページからパスキーを登録、追加、削除できる。
パスキーの登録、追加、削除の前には、Google アカウントまたは既存のパスキーで再認証する。
再認証の結果は短命の elevation token として扱い、発行、期限、使い切りは認証の issue が所有する。

API は、認証済みの利用者の範囲だけを読み書きする。
レート制限の上限と時間窓は環境ごとの vars に置く。
アカウントの削除では、サーバーの D1 の行と R2 のオブジェクトを削除し、端末内のデータは端末側の削除処理が担う。

## データの境界

D1 は、利用者に属するデータ、マスター、一時的なジョブの状態を持つ。
利用者に属するデータは、利用者 ID で範囲を分ける。
種目、あらかじめ用意したプログラム、筋肉の単位などのマスターは、利用者に属さない。
トレーニング記録は、入力の順序を保つ表として持つ。
記録は、種目マスターの変更の影響を受けないように、表示に必要な値をスナップショットとして持つ。

R2 は、AI の入出力スナップショット、CSV の書き出しを持つ。
バケットは環境ごとに 1 つとし、公開しない。
AI のスナップショットは `users/<利用者 ID>/` の下に、CSV は `exports/<利用者 ID>/` の下に置く。
CSV は `exports/` の接頭辞に対するライフサイクルルールで 7 日後に削除する。
AI のスナップショットはアカウントの削除まで保持する。
どちらの取得も Worker の認証済みの経路を通し、署名付き URL と公開 URL を使わない。

端末の IndexedDB は、記録中の入力、送信待ちの列、画面のキャッシュを持つ。
端末内のデータは、アカウントの削除の応答の後に削除する。
キャッシュの再保存は Service Worker が担い、削除の処理と混ぜない。

## 記録と同期

記録画面は、セットの入力、レストタイマー、前回の記録、自己ベスト、メモを扱う。
セットの入力は端末内に保存し、通信の有無にかかわらず続けられる。
サーバーへの送信は、操作の順序を保つ送信待ちの列を通す。
同じ操作 ID の再送は、サーバーの状態を変えない。
親の操作が解決するまで、依存する操作は送らない。

時間の境界は日本時間で決める。
日付の切り替え、週の集計、カレンダーの表示は、同じ境界の規則を使う。

## AI

AI の評価はトレーニングの終了時と期間の終了時に生成し、提案はトレーニングの終了時と週と月の評価の後に生成する。
入力は、トレーニング記録、メモ、目標、身体情報のうち、ADR-010 が定める範囲に限る。
入出力スナップショットは R2 に保存し、D1 にはメタデータだけを置く。
AI の出力は、スキーマの検証を通ったものだけを画面に出す。
出力が不正な場合は再生成し、再試行の回数に上限を置く。
実モデルの品質評価は CI では行わず、staging と実機で行う。

## 非同期処理

Queues は、AI の生成ジョブと、レスト終了のプッシュ通知の予約に使う。
Cron Triggers は、週と月の評価の生成、期限切れのセッションとチャレンジの削除、lease の回復に使う。
ジョブは、状態と再試行の回数を持ち、同じ入力の再実行で状態が壊れないようにする。
プッシュ通知の予約は、レストの終了予定時刻を絶対時刻で持ち、延長、短縮、中止に追従する。

## 環境

production と staging は、D1、R2、Queues、Workers AI、Cron Triggers のバインディングを分ける。
ホスト名は環境ごとに固定し、パスキーの Relying Party ID と Google OAuth のリダイレクト URI をホスト名に合わせる。
Secret は環境ごとに分け、リポジトリと端末内の成果物に値を残さない。
ローカルは `wrangler dev` と `mise` のタスクで動かし、production の資格情報を置かない。
staging は、実機の確認、AI の品質評価、プッシュ通知の計測に使う。

## テストと観測

テストは、単体テスト、PBT、Fuzzing、形式手法、E2E の 5 層に分ける。結合テストとブラウザのテストは E2E の層に置く。
ブラウザの API に依存する部品は Chrome と Safari の実ブラウザで確かめる。
E2E は実物の Worker、D1、R2、Queues、ブラウザを組み合わせる。
Safari の E2E は macOS のジョブで実行し、Chrome の E2E は Linux のジョブで実行する。
staging の実機では、パスキー、プッシュ通知、オフラインの記録など、実ブラウザのエミュレーションで再現しにくい範囲を確かめる。

ログとエラーの記録は、利用者の記録の内容を含めない。
AI の入力と出力の保存、削除、再試行の結果は、利用者の操作と結び付けて追えるようにする。

## 参照

- `docs/prd/warrior.md`
- `docs/adr/001-web-app-as-pwa.md`
- `docs/adr/002-local-first-session-recording.md`
- `docs/adr/003-session-ownership-by-start-device.md`
- `docs/adr/004-ordered-training-record-structure.md`
- `docs/adr/005-japan-time-date-boundaries.md`
- `docs/adr/006-record-snapshots-independent-of-master.md`
- `docs/adr/007-rest-timer-absolute-end-time.md`
- `docs/adr/008-server-scheduled-rest-push.md`
- `docs/adr/009-ai-proposals-generated-at-session-end.md`
- `docs/adr/010-ai-context-data-boundary.md`
- `docs/adr/011-design-tokens-as-source-of-truth.md`
- `docs/adr/012-inline-svg-body-figure.md`
- `docs/adr/013-google-account-with-passkeys.md`
- `docs/adr/014-backend-rust-workers-rs.md`
- `docs/adr/015-d1-as-primary-store.md`
- `docs/adr/016-dioxus-tailwind-same-origin.md`
- `docs/adr/017-r2-for-ai-snapshots-and-exports.md`
- `docs/adr/018-workers-ai-with-validation.md`
- `docs/adr/019-google-oauth-and-passkeys.md`
- `docs/adr/020-pwa-offline-recording-and-service-worker.md`
- `docs/adr/021-queues-and-cron-for-jobs.md`
- `docs/adr/022-mise-toolchain.md`
- `docs/adr/023-environment-separation.md`
- `docs/adr/024-test-strategy.md`
- `docs/adr/025-minimal-dependencies.md`
