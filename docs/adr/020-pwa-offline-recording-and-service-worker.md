# ADR-020: PWA のオフライン記録と Service Worker を Rust で実装する

- Status: Accepted

## Context

記録中のトレーニングは、入力のたびに端末内へ保存し、通信が切れても入力を続けられる (REQ-NFR-003)。
再読み込みの後や、タブを閉じて開き直した後も、記録の続きから再開できる (REQ-NFR-004)。
通信が復帰したら、端末内の記録を自動でサーバーに保存する (REQ-NFR-005)。
未接続でのアプリの起動と、過去の記録の未接続での閲覧は対象外である。
画面を消灯した状態やブラウザを閉じた状態でも、レストの終了をプッシュ通知で知らせる (REQ-TMR-008)。
参照する `../../../brewbook` は PWA の manifest を持つが、Service Worker と IndexedDB を使っていない。
ブラウザは Service Worker の登録に JavaScript のファイルを要求する。
フロントエンドのアプリ本体は Rust の Dioxus で作り、wasm にビルドする。
Service Worker のロジックも Rust で書き、wasm にビルドする。

## Decision

アプリ本体は Rust と Dioxus で書き、wasm にビルドする。
Service Worker のロジックも Rust で書き、`wasm-bindgen` の `--target no-modules` で wasm にビルドする。
Service Worker は、アプリの静的なファイルのキャッシュと、プッシュ通知の受信と表示を担う。
画面の描画、状態管理、API の呼び出し、IndexedDB の操作は、アプリ本体の Rust から行う。
PWA の manifest は JSON で置く。

ブラウザが要求する登録用の `sw.js` は、生成された JS を `importScripts` で読み込み、wasm を初期化するだけの最小のローダーとする。
手書きの JavaScript にアプリのロジックを置かない。
Service Worker の `install`、`activate`、`fetch`、`push`、`notificationclick` のイベントは、Rust で処理する。
キャッシュとプッシュの処理には、`web-sys` の API を使う。

Service Worker のクレートを Cargo のワークスペースに置く。
`frontend:build-service-worker` のタスクで wasm と JS をビルドし、`frontend/public/service-worker/` に出力する。
このタスクを `dx build` の前に実行する。
Service Worker の wasm と JS の版を揃え、キャッシュの版に使う。
Service Worker が読み込む JS と wasm も、install のときにキャッシュする。
Service Worker のローダーは、起動時にキャッシュから wasm を読み、ネットワークからの取得に失敗しても起動できるようにする。

記録中の入力は、Rust から `web-sys` の IndexedDB の API を使って端末内に保存する。
IndexedDB のデータベースに版を持ち、スキーマを変えるときは移行する。
移行に失敗した場合は、未同期の記録を保持したまま再試行できるようにする。
IndexedDB の汎用クレートは追加しない。
保存してから画面へ反映し、サーバーへの送信は入力の完了条件にしない。
端末内の記録には、アプリが生成した UUID v4 の ID を付ける。
通信が復帰したら、端末内の送信待ちの列から API へ送る。
サーバーが受理した時点で、未保存の表示を消す。

再読み込みの後は、IndexedDB の記録から状態を復元する。
サーバーに保存済みの内容で、端末内の記録を上書きしない。
過去の記録を未接続で閲覧するためのキャッシュは持たない。

Service Worker は、アプリの静的なファイル、wasm、CSS、アイコンをキャッシュする。
キャッシュは版ごとに分け、新しい版の Service Worker が有効になったら古いキャッシュを削除する。
プッシュ通知を受け取ったら、通知を表示する。
通知を押したら、アプリの画面を開くか、既に開いている画面にフォーカスする。

Rust のアプリは、Service Worker を登録し、`PushManager` でプッシュの購読を作り、購読の情報を API に送る。
iOS ではホーム画面に追加した場合だけプッシュ通知が届くため、追加の手順を画面で案内する。

## Consequences

手書きの JavaScript は、Service Worker の登録用のローダーの数行だけになる。
`wasm-bindgen` の生成物は Rust のビルド成果物であり、手書きの JavaScript ではない。

Service Worker の wasm は `wasm-bindgen-test` で直接テストしにくい。
プッシュの payload の解析と、キャッシュのキーの決定を純粋な関数に分けて native のテストで検証し、イベントの処理は E2E のブラウザテストで検証する。

Service Worker の wasm のサイズを計測する。
キャッシュの対象を増やすと、初回のダウンロードと更新の量が増える。

Service Worker のキャッシュが古いままだと、更新が利用者に届かない。
wasm と JS の版を揃え、新しい版が有効になったら古いキャッシュを削除する。

iOS のホーム画面に追加した PWA で、Service Worker の wasm が起動できることを試作で確認する。
起動できない場合は、通知の受信と表示だけを JavaScript に残す例外を認め、この ADR を改訂する。
Service Worker の wasm が起動できない場合に備えて、通知の表示のフォールバックを用意する。

ブラウザは Service Worker の登録に JavaScript のファイルを要求するため、ローダーの JavaScript は残る。
Service Worker を完全にゼロ JavaScript にすることはできない。

未接続でアプリを起動することは対象外である。
アプリが読み込まれた後は、未接続でも記録中のトレーニングを続けられる。

プッシュ通知の到達は、端末の通知の許可と、iOS のホーム画面への追加に依存する。
音と画面表示を常に代替として持つ (ADR-008)。

## References

- `docs/prd/warrior.md` の「トレーニング記録」「非機能要件」
- `docs/adr/002-local-first-session-recording.md`
- `docs/adr/008-server-scheduled-rest-push.md`
- `docs/design/README.md` の「画面のきまり」
- MDN の Service Worker と IndexedDB のドキュメント
- `wasm-bindgen` の `--target no-modules` のドキュメント
- `wasm-bindgen-service-worker` の実装例
