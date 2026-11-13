# PWA の manifest と Service Worker のキャッシュを実装する

- Requirements: REQ-NFR-001, REQ-TMR-008
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-pwa-service-worker

## 目的

Web アプリをインストール可能な PWA として提供し、Service Worker で静的なファイルをキャッシュする。
新しい版の Service Worker が有効になったら古いキャッシュを削除し、更新を利用者に届ける。
iOS でプッシュ通知を受けるために、ホーム画面への追加の手順を案内する。

## 優先度根拠

PWA の manifest と Service Worker は、レスト通知の受信とアプリの起動に必要である。
キャッシュの版の管理を後から足すと、古い画面が動き続けて更新が届かない。

## 現状

0007 で Dioxus のアプリシェルを作るが、manifest、Service Worker、キャッシュ、更新の検出、ホーム画面への追加の案内がない。

## 設計方針

PWA の manifest を JSON で `frontend/public/` に置く。
ADR-001 は、単一の Web アプリを PWA として提供し、Service Worker がプッシュ通知の受信と静的なファイルの配信を担うと決めている。この issue は Service Worker の器とキャッシュを置き、プッシュ通知の受信と表示、通知のクリックの処理は 0036 が加える。
ADR-020 は、Service Worker のロジックを Rust で書き、Service Worker 自身の wasm と JS の版を揃えてキャッシュの版に使うと決めている。
この issue はこれらの決定に従う。
`name`、`short_name`、`start_url`、`display`、`background_color`、`theme_color`、アイコンを含める。
表示は `standalone` とし、ホーム画面から起動した場合はブラウザの画面を隠す。

Service Worker のロジックは Rust で書き、`wasm-bindgen` の `--target no-modules` で wasm にビルドする。
ブラウザが要求する登録用の `sw.js` は、生成された JS を `importScripts` で読み込み、Service Worker 自身の wasm を初期化するだけの最小のローダーとする。
手書きの JavaScript にアプリのロジックを置かない。

Service Worker は、アプリの静的なファイル、アプリの wasm、CSS、アイコンをキャッシュする。
生成された JS は HTTP キャッシュに任せ、Cache API には入れない。
キャッシュは版ごとに分け、新しい版の Service Worker が有効になったら古いキャッシュを削除する。
Service Worker 自身の wasm は `install` のときに Cache API に保存し、ローダーは起動時にキャッシュから読む。ローダーは Service Worker 自身の wasm がキャッシュにない場合、ネットワークから取得して Cache API に保存する。
キャッシュの対象 (アプリの静的なファイル、アプリの wasm、CSS、アイコン) の要求のうち、キャッシュにないものは、ネットワークから取得して Cache API に保存する。生成された JS は対象外とする。
ネットワークからの取得に失敗しても、キャッシュから起動できるようにする。

登録用の `sw.js` は `Cache-Control: no-cache` で配信し、更新を検出できるようにする。
生成された JS は版付きの URL で配信し、`Cache-Control` でキャッシュする。
Service Worker 自身の wasm と JS の版を揃え、キャッシュの版に使う。

アプリは Service Worker を登録し、新しい版が待機していることを検出したら、再読み込みの操作を画面に出す。
利用者が操作したときに待機中の Service Worker を有効にし、画面を再読み込みする。

iOS の Safari で、ホーム画面に追加していない状態を検出したら、共有メニューからホーム画面に追加する手順を表示する。
この案内は、プッシュ通知の購読の画面でも使う。

未接続でのアプリの新規起動と再読み込みは対象外とする。
アプリが読み込まれた後は、未接続でも記録中のトレーニングを続けられる。この保存は 0021 が担う。

## 完了条件

- [ ] 対象のブラウザの最新版で、アプリをホーム画面に追加して起動できる。これは REQ-NFR-001 の受入基準「スマートフォンの Safari と Chrome、PC の Chrome と Safari の最新版で、すべての画面を操作できる」のうち、PWA としての起動を満たす。
- [ ] manifest に `name`、`short_name`、`start_url`、`display`、`background_color`、`theme_color`、アイコンがある。
- [ ] ホーム画面から起動した場合、ブラウザの画面を隠して起動する。
- [ ] Service Worker がアプリの静的なファイル、アプリの wasm、CSS、アイコンをキャッシュする。
- [ ] 生成された JS を Cache API に入れず、HTTP キャッシュに任せる。
- [ ] キャッシュが版ごとに分かれ、新しい版が有効になったら古いキャッシュが削除される。
- [ ] キャッシュの対象の要求のうち、キャッシュにないものをネットワークから取得し、Cache API に保存する。生成された JS は対象外とする。これはキャッシュの可用性の内部条件であり、`REQ-NFR-001` の受入基準の外側である。
- [ ] 新しい版が待機しているとき、画面に再読み込みの操作が出る。操作の後に新しい版が有効になる。
- [ ] アプリが読み込まれた後にネットワークを切った場合も、画面の表示を続けられる。未接続での新規起動と再読み込みは対象外である。
- [ ] iOS でホーム画面に追加していない場合、追加の手順が表示される。これは REQ-TMR-008 の受入基準「iOS でホーム画面に追加していない場合は通知が届かないため、追加の手順を案内する」のうち、案内の部分を満たす。通知の購読は 0036 が担う。
- [ ] ブラウザのテストで、manifest の読み込み、Service Worker の登録、キャッシュの対象、キャッシュにない対象の要求の取得と保存、Service Worker 自身の wasm の cache miss の取得と保存、古いキャッシュの削除、更新の操作を確かめる。
- [ ] E2E で、ホーム画面に追加した状態の起動と、読み込まれた画面の表示を続けられることを確かめる。iOS の実機確認は 0067 の staging で行う。この issue の完了条件は Chrome と Safari のローカル E2E までとする。
- [ ] Service Worker のキャッシュのキーの決定を純粋な関数に分け、単体テストで確かめる。
- [ ] PBT は、キャッシュのキーが版と経路から一意に決まることを確かめる。

## 解決方法

`frontend/public/manifest.webmanifest` とアイコンを追加する。
`service-worker/Cargo.toml` と `service-worker/src/lib.rs` に、`install`、`activate`、`fetch` の処理を置く。`push` と `notificationclick` の処理は 0036 が追加する。
`frontend/public/sw.js` に、生成された JS を読み込む最小のローダーを置く。
`frontend/src/pwa.rs` に、Service Worker の登録、更新の検出、再読み込み、iOS の追加の案内を置く。
`frontend/src/text.rs` に、更新とホーム画面への追加の文言を追加する。
`backend/warrior/src/security.rs` の CSP に、Service Worker 自身の wasm の実行に必要な設定を加える。
`frontend/tests/pwa.rs` にブラウザのテストを追加する。
`backend/warrior/tests/e2e_pwa.rs` に E2E を追加する。
`backend/pbt/tests/pwa.rs` にキャッシュのキーの性質テストを置く。
