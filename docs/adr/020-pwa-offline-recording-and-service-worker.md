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
`importScripts` は Service Worker の fetch を経由せず、ブラウザの HTTP キャッシュを使う。
生成された JS は版付きの URL で配信し、`Cache-Control` でキャッシュする。
登録用の `sw.js` は `Cache-Control: no-cache` で配信し、更新を検出できるようにする。
手書きの JavaScript にアプリのロジックを置かない。
Service Worker の `install`、`activate`、`fetch`、`push`、`notificationclick` のイベントは、Rust で処理する。
キャッシュとプッシュの処理には、`web-sys` の API を使う。

Service Worker のクレートを Cargo のワークスペースに置く。
Service Worker の wasm と JS を、アプリのビルドの前にビルドして配信物に含める。
Service Worker 自身の wasm と JS の版を揃え、キャッシュの版に使う。
登録用の `sw.js` と生成された JS は、ブラウザの HTTP キャッシュに任せる。
Service Worker 自身の wasm は install のときに Cache API に保存し、ローダーは起動時にキャッシュから読む。wasm がキャッシュにない場合、ローダーはネットワークから取得して Cache API に保存する。
ネットワークからの取得に失敗しても、キャッシュから起動できるようにする。

記録中の入力の端末内への保存、IndexedDB のスキーマの移行、送信待ちの列、再読み込み後の復元は ADR-002 に従う。
この ADR は、Service Worker のキャッシュとプッシュ通知を扱う。

Service Worker は、アプリの静的なファイル、wasm、CSS、アイコンをキャッシュする。
生成された JS は HTTP キャッシュに任せ、Cache API には入れない。
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

iOS のホーム画面に追加した PWA で、Service Worker の wasm が起動できることを確認する。
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
