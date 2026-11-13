# デザイントークンから CSS と Rust の定数を生成する

- Requirements: REQ-NFR-012
- Priority: High
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/other-design-tokens

## 目的

`docs/design/tokens.json` を色、フォント、寸法、人体図の色、筋肉の単位の唯一の定義にし、アプリのコードに色のリテラル値を書かない仕組みを作る。
ADR-011 の生成の手順をビルドに組み込む。

## 優先度根拠

画面、人体図、グラフ、テーマの切り替えがすべてこの生成物に依存する。
色の定義がコードへ散ると、ライトとダークの両方の判別性を保てない。

## 現状

`docs/design/tokens.json` に配色、フォント、画面の基準寸法、人体図の色、筋肉の単位がある。
しかし、アプリが読む CSS カスタムプロパティと Rust の定数がなく、生成のタスクもない。
そのため、画面の実装ごとに色の値を写す状態になり、テーマの変更が全体へ波及する。

## 設計方針

ビルド用のクレート `tools/token-gen` をワークスペースに追加し、`docs/design/tokens.json` を原本として読んで次の 2 つを生成する。原本は `docs/design/` に置き、生成物だけを `frontend/` に置く。

- `frontend/src/generated/tokens.rs`: 色、フォント、寸法、筋肉の単位の識別子を型付きの定数にする。
- `frontend/public/generated/tokens.css`: ライトとダークの CSS カスタムプロパティを出力する。

生成物はリポジトリに含めない。
`mise run tokens` を `build-frontend` と `check` の依存に置き、生成に失敗したらビルドを失敗させる。
生成物は `.gitignore` に加える。

CSS のカスタムプロパティは、ルート要素のテーマ属性で切り替える。
初期値は端末の設定に合わせ、利用者が選んだテーマは 0062 で保存する。
部位の色は 8 つの部位区分のキーで引く。
人体図の色は人体図専用のキーで引く。
部位の色と人体図の色を同じものとして扱わない。

押せる要素の最小 44px と、文字の最小 11px を定数にし、画面の実装で検査する。
`mark` と `primary` の色を `surfaceRaised` の上の文字に使わない制約も、コメントではなく検査で守る。

Tailwind のテーマは生成した CSS のカスタムプロパティを参照する。
Tailwind の設定に色のリテラル値を書かない。

## 完了条件

- [ ] `mise run tokens` が `docs/design/tokens.json` から `frontend/src/generated/tokens.rs` と `frontend/public/generated/tokens.css` を生成する。これは PRD の要求 ID を持たない生成の条件である。
- [ ] `tokens.json` の全ての色、フォント、寸法、人体図の色、筋肉の単位が生成物に現れる。
- [ ] ライトとダークの両方の部位色とグラフの色が生成され、どちらのモードでも部位を判別できる組み合わせである。これは REQ-NFR-012 の受入基準「部位の色分けとグラフは、どちらのモードでも部位を判別できる配色になる」のうち、配色の定義を満たす。切り替えの操作は 0062 が担う。
- [ ] 生成した CSS に、ライトとダークのカスタムプロパティが揃っている。
- [ ] 生成した Rust の定数が、色、フォント、寸法、筋肉の単位を型付きで引ける。
- [ ] `frontend/src` の生成物の外に色のリテラル値がない。検査は `rg` を使う `mise` のタスクで行う。
- [ ] `tokens.json` の必須のキーが欠けたとき、`mise run tokens` が失敗する。
- [ ] 筋肉の単位の識別子が重複しないことを PBT ではなく例示テストで確かめる。入力が固定の定義ファイルであるため、PBT の対象にしない。
- [ ] 生成物を削除した状態から `mise run check` を実行すると、生成が走って成功する。

## 解決方法

`tools/token-gen/Cargo.toml` と `tools/token-gen/src/main.rs` を追加し、`serde_json` で `tokens.json` を読む。
`backend/warrior_core` ではなくビルド用のクレートに置く。本番の wasm に含めないためである。
`tools/token-gen/tests/` に、全てのキーの生成、必須キーの欠落、筋肉の単位の重複を確かめるテストを置く。
`frontend/tailwind.css` は生成した `tokens.css` を読み込み、`@theme` でカスタムプロパティを参照する。
`mise.toml` に `tokens` タスクを追加し、`build-frontend` と `check` の依存にする。
`frontend/src/theme.rs` に、テーマ属性を切り替える関数の置き場所を作る。実際の切り替えは 0062 で実装する。
