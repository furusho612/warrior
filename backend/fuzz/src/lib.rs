//! Fuzzing の対象と、対象のパーサへ任意の入力を渡す入口。
//!
//! Fuzzing は、任意のバイト列をパーサへ渡してパニックとタイムアウトが起きないことだけを
//! 確かめる (ADR-024)。値の正しさ、不変条件、往復の性質は PBT が担う。
//! `fuzz_targets/` の各ターゲットと、`backend/warrior/tests/fuzz_corpus.rs` の
//! corpus の例示テストが、この module の [`run`] を呼ぶ。

use std::sync::LazyLock;

use warrior_core::ai::{AiContext, AiOutput, AiProfile, TrainingExperience};
use warrior_core::nutrition::{Goal, Sex};
use warrior_core::{
    auth, base64url, cbor, cose, datetime, duration, ids, push, rest_push, rpe, sync, web_push,
    webauthn,
};

/// Fuzzing の対象。1 つの対象が、1 つの入力の境界に対応する。
///
/// 対象の数え方は、`docs/issues/0065-other-fuzzing-harness.md` の箇条書きの単位に合わせる。
/// WebAuthn の 5 つのパーサは 1 つの対象として扱う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// WebAuthn の CBOR、COSE、clientDataJSON、attestation object、authenticator data。
    WebAuthn,
    /// Google の ID トークンのクレームのパーサ。
    IdToken,
    /// AI の出力の JSON と、その中のテキスト。
    AiOutput,
    /// 同期の操作の JSON。
    SyncOperation,
    /// Web Push の購読の鍵。
    PushSubscription,
    /// base64url の文字列。
    Base64Url,
    /// ISO 8601 の日時。
    DateTime,
    /// UUID v4。
    Uuid,
    /// `mm:ss` と秒の時間。
    Duration,
    /// RPE の値。
    Rpe,
    /// Web Push の VAPID JWT と aes128gcm の本文。
    WebPush,
}

impl Target {
    /// すべての対象。corpus のディレクトリ名と、`cargo fuzz` のターゲット名に対応する。
    pub const ALL: &[Target] = &[
        Target::WebAuthn,
        Target::IdToken,
        Target::AiOutput,
        Target::SyncOperation,
        Target::PushSubscription,
        Target::Base64Url,
        Target::DateTime,
        Target::Uuid,
        Target::Duration,
        Target::Rpe,
        Target::WebPush,
    ];

    /// corpus のディレクトリ名と `cargo fuzz` のターゲット名。
    pub fn name(self) -> &'static str {
        match self {
            Target::WebAuthn => "webauthn",
            Target::IdToken => "id_token",
            Target::AiOutput => "ai_output",
            Target::SyncOperation => "sync_operation",
            Target::PushSubscription => "push_subscription",
            Target::Base64Url => "base64url",
            Target::DateTime => "datetime",
            Target::Uuid => "uuid",
            Target::Duration => "duration",
            Target::Rpe => "rpe",
            Target::WebPush => "web_push",
        }
    }
}

/// 対象のパーサに任意の入力を渡す。
///
/// 戻り値は返さない。Fuzzing が見るのは、パニックとタイムアウトが起きないことだけである。
pub fn run(target: Target, data: &[u8]) {
    match target {
        Target::WebAuthn => run_webauthn(data),
        Target::IdToken => run_id_token(data),
        Target::AiOutput => run_ai_output(data),
        Target::SyncOperation => run_sync_operation(data),
        Target::PushSubscription => run_push_subscription(data),
        Target::Base64Url => run_base64url(data),
        Target::DateTime => run_datetime(data),
        Target::Uuid => run_uuid(data),
        Target::Duration => run_duration(data),
        Target::Rpe => run_rpe(data),
        Target::WebPush => run_web_push(data),
    }
}

/// WebAuthn の検証の固定の条件。入力に依存しない値だけを使う。
const RP_ID: &str = "warrior.example.workers.dev";
const ORIGIN: &str = "https://warrior.example.workers.dev";
const CHALLENGE: &str = "fuzz-challenge";

/// WebAuthn の 5 つのパーサへ、同じ入力を別の境界として渡す。
fn run_webauthn(data: &[u8]) {
    let encoded = base64url::encode(data);
    // CBOR と COSE は任意のバイト列をそのまま読む。
    let _ = cbor::decode(data);
    let _ = cose::parse(data);
    // clientDataJSON は base64url で運ばれるため、任意のバイト列を復号して JSON として読む。
    let _ = webauthn::client_data_challenge(&encoded);
    // 登録とログインは、種別ごとに正当な clientDataJSON を固定し、attestation object と
    // authenticatorData のパーサへ任意のバイト列を届ける。
    let registration_client_data = encoded_client_data(webauthn::REGISTRATION_TYPE);
    let _ = webauthn::verify_registration(&webauthn::RegistrationInput {
        rp_id: RP_ID,
        origin: ORIGIN,
        expected_challenge: CHALLENGE,
        client_data_json: &registration_client_data,
        attestation_object: &encoded,
    });
    let authentication_client_data = encoded_client_data(webauthn::AUTHENTICATION_TYPE);
    let _ = webauthn::verify_authentication(&webauthn::AuthenticationInput {
        rp_id: RP_ID,
        origin: ORIGIN,
        expected_challenge: CHALLENGE,
        client_data_json: &authentication_client_data,
        authenticator_data: &encoded,
        signature: &encoded,
        cose_public_key: data,
    });
}

/// 検証の条件を満たす `clientDataJSON` を base64url にする。
fn encoded_client_data(data_type: &str) -> String {
    let client_data = format!(
        r#"{{"type":"{data_type}","challenge":"{CHALLENGE}","origin":"{ORIGIN}","crossOrigin":false}}"#
    );
    base64url::encode(client_data.as_bytes())
}

const ID_TOKEN_AUDIENCE: &str = "fuzz-client";
const ID_TOKEN_ISSUER: &str = "https://accounts.google.com";
const ID_TOKEN_NONCE: &str = "fuzz-nonce";
const ID_TOKEN_NOW: i64 = 1_800_000_000;

/// ID トークンの形のパーサと、payload のクレームのパーサへ入力を渡す。
fn run_id_token(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = auth::decode_id_token_payload(text);
        let _ = auth::validate_id_token(
            text,
            ID_TOKEN_AUDIENCE,
            ID_TOKEN_ISSUER,
            ID_TOKEN_NONCE,
            ID_TOKEN_NOW,
        );
    }
    // payload には base64url で運ばれる任意のバイト列を渡す。
    let token = format!("header.{}.signature", base64url::encode(data));
    let _ = auth::validate_id_token(
        &token,
        ID_TOKEN_AUDIENCE,
        ID_TOKEN_ISSUER,
        ID_TOKEN_NONCE,
        ID_TOKEN_NOW,
    );
}

/// AI の出力の検証が使うコンテキスト。出力のパーサに集中するため、記録は空にする。
static AI_CONTEXT: LazyLock<AiContext> = LazyLock::new(|| AiContext {
    goal: Goal::Hypertrophy,
    profile: AiProfile {
        height_cm: 175.0,
        weight_kg: 70.0,
        age: 36,
        sex: Sex::Male,
    },
    experience: TrainingExperience::Intermediate,
    training_days_per_week: 4,
    records: Vec::new(),
    goals: Vec::new(),
    part_analysis: Vec::new(),
    assisted_set_reference: Vec::new(),
    recovery: Vec::new(),
    routine: Vec::new(),
    exercises: Vec::new(),
});

/// AI の出力の JSON と、その中のテキストの検証へ入力を渡す。
fn run_ai_output(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = AiOutput::from_json(text, &AI_CONTEXT);
}

/// 同期の操作の JSON のパーサと、本文の検証へ入力を渡す。
fn run_sync_operation(data: &[u8]) {
    let Ok(operation) = serde_json::from_slice::<sync::Operation>(data) else {
        return;
    };
    let _ = sync::validate_operation(&operation);
}

/// Web Push の購読の endpoint と鍵のパーサへ入力を渡す。
fn run_push_subscription(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = push::https_host(text);
        let _ = push::is_allowed_endpoint(text);
    }
    let Ok(input) = serde_json::from_slice::<push::SubscriptionInput>(data) else {
        return;
    };
    let _ = push::Subscription::new(input);
}

/// base64url の復号と符号化へ入力を渡す。
fn run_base64url(data: &[u8]) {
    let encoded = base64url::encode(data);
    let _ = base64url::decode(&encoded);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = base64url::decode(text);
    }
}

/// ISO 8601 の日時のパーサと比較へ入力を渡す。
///
/// 比較の 2 つの値は、入力の中の NUL バイトで区切る。
fn run_datetime(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = datetime::parse_utc(text);
    let _ = datetime::japan_date_from_utc(text);
    if let Some((left, right)) = text.split_once('\0') {
        let _ = datetime::compare_utc(left, right);
    }
}

/// UUID v4 の検証と、16 バイトからの文字列の生成へ入力を渡す。
fn run_uuid(data: &[u8]) {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = ids::uuid_bytes(text);
    }
    let mut bytes = [0u8; 16];
    let copied = data.len().min(bytes.len());
    bytes[..copied].copy_from_slice(&data[..copied]);
    let uuid = ids::uuid_v4_from_bytes(bytes);
    let _ = ids::is_valid_uuid_v4(&uuid);
    let _ = ids::uuid_bytes(&uuid);
}

/// `mm:ss` と秒の時間のパーサへ入力を渡す。
fn run_duration(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = duration::parse(text);
}

/// RPE の値のパーサへ入力を渡す。
fn run_rpe(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = rpe::parse(text);
}

/// Web Push の VAPID JWT と aes128gcm の本文へ入力を渡す。
fn run_web_push(data: &[u8]) {
    let secret = p256::SecretKey::from_slice(&[1u8; 32]).expect("the fixed key is a scalar");
    let public_key =
        p256::elliptic_curve::sec1::ToSec1Point::to_sec1_point(&secret.public_key(), false);
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = web_push::verify_vapid(
            text,
            public_key.as_bytes(),
            "https://push.example.net",
            1_800_000_000,
        );
        let _ = endpoint_origin(text);
    }
    let _ = web_push::decrypt(data, &secret, &[0u8; 16]);
    let _ = rest_push::should_send(
        rest_push::ReservationState {
            active: true,
            version: 1,
            end_at: 0,
            sent_at: None,
        },
        1,
        i64::from(data.first().copied().unwrap_or(0)),
    );
}

fn endpoint_origin(text: &str) -> Result<String, web_push::Error> {
    web_push::endpoint_origin(text)
}
