# ADR 一覧

設計判断の一覧。
Status が Accepted になった ADR を実装の前提にする。

`../../../brewbook` を参照する ADR は、著者の環境の隣接リポジトリを指す。
参照は `brewbook` の commit `f7e936c8fe344ff0d0ab28c6659f7fcbe5e2e06d` の時点である。

| ID | 判断 | Status |
| --- | --- | --- |
| [ADR-001](001-web-app-as-pwa.md) | Web アプリをインストール可能な PWA として提供する | Accepted |
| [ADR-002](002-local-first-session-recording.md) | 記録は端末内へ先に保存し、サーバーへ非同期に同期する | Accepted |
| [ADR-003](003-session-ownership-by-start-device.md) | 記録中のトレーニングは開始した端末だけが編集する | Accepted |
| [ADR-004](004-ordered-training-record-structure.md) | 記録はトレーニング、種目、セットの順序付き構造で持つ | Accepted |
| [ADR-005](005-japan-time-date-boundaries.md) | トレーニングの日付と週の境界は日本時間で決める | Accepted |
| [ADR-006](006-record-snapshots-independent-of-master.md) | 過去の記録は種目と身体情報のスナップショットを持つ | Accepted |
| [ADR-007](007-rest-timer-absolute-end-time.md) | レストタイマーは終了予定時刻から残り時間を計算する | Accepted |
| [ADR-008](008-server-scheduled-rest-push.md) | レスト終了のプッシュ通知はサーバーが予約する | Accepted |
| [ADR-009](009-ai-proposals-generated-at-session-end.md) | AI の評価と次回セットの提案はトレーニングの終了時に生成して保存する | Accepted |
| [ADR-010](010-ai-context-data-boundary.md) | AI に渡す情報はサーバーで組み立て、必要な項目に限る | Accepted |
| [ADR-011](011-design-tokens-as-source-of-truth.md) | デザイントークンを色と寸法の唯一の定義にする | Accepted |
| [ADR-012](012-inline-svg-body-figure.md) | 人体図はインライン SVG と筋肉単位の属性で描く | Accepted |
| [ADR-013](013-google-account-with-passkeys.md) | Google アカウントを本人確認の起点にし、パスキーを追加のログイン手段にする | Accepted |
| [ADR-014](014-backend-rust-workers-rs.md) | バックエンドを Rust と workers-rs で Cloudflare Workers に実装する | Accepted |
| [ADR-015](015-d1-as-primary-store.md) | データベースに Cloudflare D1 を使い、直接 SQL で扱う | Accepted |
| [ADR-016](016-dioxus-tailwind-same-origin.md) | フロントエンドを Dioxus と Tailwind で書き、同じ Worker から同一オリジンで配信する | Accepted |
| [ADR-017](017-r2-for-ai-snapshots-and-exports.md) | R2 を AI の入出力スナップショットと CSV の書き出しに使う | Accepted |
| [ADR-018](018-workers-ai-with-validation.md) | Workers AI を AI バインディングで使い、出力を検証する | Accepted |
| [ADR-019](019-google-oauth-and-passkeys.md) | 認証を Google OAuth とパスキーで行い、WebAuthn の検証を自前で実装する | Accepted |
| [ADR-020](020-pwa-offline-recording-and-service-worker.md) | PWA のオフライン記録と Service Worker を Rust で実装する | Accepted |
| [ADR-021](021-queues-and-cron-for-jobs.md) | Cloudflare Queues と Cron Triggers で AI 生成とレスト通知を実行する | Accepted |
| [ADR-022](022-mise-toolchain.md) | ツールチェーンを mise で固定する | Accepted |
| [ADR-023](023-environment-separation.md) | 環境をローカル、staging、production に分ける | Accepted |
| [ADR-024](024-test-strategy.md) | テストを E2E、PBT、Fuzzing、形式手法、単体の 5 層で整備する | Accepted |
| [ADR-025](025-minimal-dependencies.md) | 依存を最小限にし、汎用クレートの一部だけを使う場合は自前で実装する | Accepted |
