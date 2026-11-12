# パスキーの登録とログインを実装する

- Requirements: REQ-NFR-013, REQ-NFR-014, REQ-MYP-006
- Priority: Medium
- Created: 2026-11-12
- Model: deepseek-v4.1-flash:1017
- Branch: feature/feature-passkeys

## 目的

認証済みのアカウントにパスキーを追加し、パスキーでログインできるようにする。
1 つのアカウントに複数のパスキーを登録でき、追加と削除ができるようにする。
パスキーを失っても Google アカウントでログインし、登録し直せるようにする。

## 優先度根拠

パスキーは段階 4 の機能であり、Google のログインが先に動く必要がある。
一方で、WebAuthn の検証の実装は認証の安全性に直結するため、画面より先に検証の形を固める。

## 現状

0015 で Google のログインとセッションを実装するが、パスキーの表、WebAuthn の検証、登録とログインの経路がない。
`p256`、`sha2`、`base64url`、CBOR、COSE の実装もない。

## 設計方針

パスキーは、認証済みのアカウントに追加するログイン手段とする。
パスワードは持たない。
パスキーの登録、追加、削除には、Google アカウントまたは既存のパスキーによる認証を必要とする。
登録、追加、削除の API は、セッションに加えて、再認証で発行した短命の elevation token を要求する。
elevation token は Google アカウントまたは既存のパスキーのアサーションで発行し、有効期限を 5 分、使用を 1 回とする。

WebAuthn の登録では、公開鍵のアルゴリズムを ES256 だけ受け付ける。
`residentKey` は `preferred`、`userVerification` は `required`、`attestation` は `none` を要求する。
attestation statement は検証しない。
ログインでも `userVerification` を `required` とする。
チャレンジは 32 バイトの乱数とし、D1 に保存し、有効期限は 5 分、使用は 1 回とする。
署名カウンタは W3C WebAuthn Level 3 の 7.2 節に従って検証する。

base64url、CBOR、COSE、WebAuthn の検証は、`../../../brewbook` の `base64url`、`cbor`、`cose`、`webauthn` の実装を移植して使う。
移植したコードには、仕様の該当節を引用したコメントを付ける。
`webauthn-rs` は追加しない。
`p256` と `sha2` だけを暗号のクレートとして使う。

登録したパスキーは、資格情報の ID、公開鍵、署名カウンタ、 transports、作成日時、最終使用日時、利用者 ID を持つ。
1 つのアカウントに複数のパスキーを登録できる。
パスキーの一覧は、マイページのアカウントの画面で表示する。

パスキーのログインは、検出可能な資格情報がある場合は、`allowCredentials` を空にして `navigator.credentials.get` を呼ぶ。
認証器が検出可能な資格情報を保存しない場合は、利用者が Google アカウントのメールアドレスを入力し、サーバーがその利用者のパスキーの ID を `allowCredentials` に渡す。
この経路により、登録したパスキーでログインできる。
メールアドレスの入力は、パスキーのログインの画面で行う。
メールアドレスの入力の経路でも、チャレンジ、`userVerification`、署名カウンタの検証は同じ規則を使う。

ログインに成功した後は、0015 と同じセッションを発行する。
パスキーをすべて失っても、Google アカウントが使えればログインして登録し直せる。
Google アカウントとすべてのパスキーを失った場合、アカウントは復旧できない。
この制約も、登録時に画面で示す。

## 完了条件

- [ ] 登録したパスキーでログインできる。検出可能な資格情報がない場合も、Google アカウントのメールアドレスの入力と `allowCredentials` でログインできる。これは REQ-NFR-013 の受入基準に対応する。
- [ ] 再認証なしのパスキーの登録、追加、削除が 403 になる。
- [ ] 1 つのアカウントに複数のパスキーを登録でき、追加と削除ができる。削除の後に一覧が変わる。これは REQ-NFR-013 と REQ-MYP-006 の受入基準に対応する。
- [ ] 登録したパスキーを使えない状態でも、Google アカウントでログインして新しいパスキーを登録できる。これは REQ-NFR-014 の受入基準に対応する。
- [ ] 登録では ES256 だけを受理し、他のアルゴリズムを拒否する。
- [ ] `residentKey` が `preferred`、`userVerification` が `required`、`attestation` が `none` である。
- [ ] チャレンジが 32 バイトの乱数で、5 分を過ぎたチャレンジと使用済みのチャレンジを拒否する。
- [ ] 署名カウンタが減少した認証を拒否する。
- [ ] パスキーのログインに成功すると、0015 と同じセッションが発行される。
- [ ] パスキーをすべて失っても Google アカウントでログインできる。Google アカウントとすべてのパスキーを失った場合は復旧できないことを、登録の画面に表示する。
- [ ] パスキーの一覧に、登録済みのパスキーが表示され、追加と削除の操作の後に一覧が変わる。
- [ ] テスト用の OIDC の提供者と Chrome の仮想認証器を使う E2E で、登録、ログイン、複数のパスキーの追加と削除、Google からの復旧、再認証なしの操作の拒否を確かめる。Safari の実機確認は 0067 の staging で行う。この issue の完了条件は Chrome の仮想認証器の E2E までとする。
- [ ] PBT で、base64url、CBOR、COSE のエンコードとデコードが往復することを確かめる。
- [ ] 例示テストで、W3C WebAuthn Level 3 のテストベクタ、署名カウンタの減少、期限切れのチャレンジ、使用済みのチャレンジを確かめる。
- [ ] Fuzzing の対象を 0065 で追加する。この issue ではパーサの型検査までを置く。

## 解決方法

`backend/warrior_core/src/base64url.rs`、`cbor.rs`、`cose.rs`、`webauthn.rs` を追加する。
`backend/warrior/src/auth/passkey.rs` に登録とログインの経路を置く。
`backend/warrior/src/router.rs` に `POST /api/auth/passkey/register/start`、`POST /api/auth/passkey/register/finish`、`POST /api/auth/passkey/login/start`、`POST /api/auth/passkey/login/finish`、`GET /api/me/passkeys`、`DELETE /api/me/passkeys/:id` を追加する。
`backend/warrior/src/auth/reauthenticate.rs` に、Google アカウントまたは既存のパスキーで再認証し、elevation token を発行する経路を置く。
`migrations/` に `passkeys` の表を追加する。
`frontend/src/screens/account.rs` にパスキーの一覧と登録、追加、削除の操作を置く。画面の仕上げは 0063 で行う。
`frontend/src/screens/login.rs` に、検出可能な資格情報がない場合のメールアドレスの入力を置く。
`backend/warrior/tests/e2e_passkey.rs` に E2E を追加する。
`backend/pbt/tests/webauthn.rs` に往復の性質テストを置く。
