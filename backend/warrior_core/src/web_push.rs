//! Web Push の VAPID の JWT と RFC 8291 の aes128gcm の暗号化。
//!
//! 送信の要求は `https://` の endpoint、購読の `p256dh` と `auth`、本文、`TTL`、
//! VAPID の鍵と subject、そして暗号に使う salt と一時鍵の組を受け取る。
//! 乱数は呼び出し側が Web Crypto から取り、このモジュールは状態を持たない。
//! `Urgency` は常に `high` とし、`TTL` は呼び出し側の値を使う (REQ-TMR-009、ADR-008)。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::base64url;

/// 送信の要求の `Urgency`。
pub const URGENCY: &str = "high";

/// 本文の `Content-Encoding`。
pub const CONTENT_ENCODING: &str = "aes128gcm";

/// AES-128-GCM の鍵の長さ。
const CEK_LEN: usize = 16;

/// AES-128-GCM の nonce の長さ。
const NONCE_LEN: usize = 12;

/// HKDF で導出する中間の鍵の長さ。
const IKM_LEN: usize = 32;

/// 1 レコードの最大の長さ。
pub const RECORD_SIZE: u32 = 4_096;

/// salt の長さ。
pub const SALT_LEN: usize = 16;

/// 認証の秘密の長さ。
pub const AUTH_LEN: usize = 16;

/// VAPID の JWT の有効期限の上限 (秒)。
pub const VAPID_MAX_LIFETIME_SECONDS: i64 = 24 * 60 * 60;

/// 送信の要求に使う VAPID の JWT の有効期間 (秒)。
pub const VAPID_LIFETIME_SECONDS: i64 = 12 * 60 * 60;

/// Web Push の暗号と送信の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// endpoint が HTTPS の URL ではない。
    InvalidEndpoint,
    /// 購読の公開鍵 (`p256dh`) が P-256 の点ではない。
    InvalidPublicKey,
    /// 購読の認証の秘密 (`auth`) の長さが 16 バイトではない。
    InvalidAuthSecret,
    /// VAPID の秘密鍵が P-256 のスカラーではない。
    InvalidVapidPrivateKey,
    /// VAPID の公開鍵が P-256 の点ではない、または秘密鍵と一致しない。
    InvalidVapidPublicKey,
    /// JWT の形式が正しくない。
    InvalidJwt,
    /// JWT の署名が正しくない。
    InvalidSignature,
    /// JWT の有効期限が現在時刻より後でない、または上限を超えている。
    InvalidExpiration,
    /// JWT の `aud` が endpoint の origin と一致しない。
    InvalidAudience,
    /// 暗号化に失敗した。
    Encryption,
    /// 復号に失敗した。
    Decryption,
    /// 本文が aes128gcm の形式ではない。
    InvalidBody,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidEndpoint => "the endpoint must be an HTTPS URL",
            Self::InvalidPublicKey => "the p256dh key must be a P-256 public key",
            Self::InvalidAuthSecret => "the auth secret must be 16 bytes",
            Self::InvalidVapidPrivateKey => "the VAPID private key must be a P-256 scalar",
            Self::InvalidVapidPublicKey => {
                "the VAPID public key must be a P-256 point that matches the private key"
            }
            Self::InvalidJwt => "the VAPID token is not a JWT",
            Self::InvalidSignature => "the VAPID token signature is invalid",
            Self::InvalidExpiration => "the VAPID token expiration is outside the allowed window",
            Self::InvalidAudience => "the VAPID token audience does not match the endpoint",
            Self::Encryption => "the payload could not be encrypted",
            Self::Decryption => "the payload could not be decrypted",
            Self::InvalidBody => "the body is not a single aes128gcm record",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for Error {}

/// VAPID の鍵と subject。
#[derive(Debug, Clone)]
pub struct VapidKeys {
    private_key: SecretKey,
    public_key: PublicKey,
    subject: String,
}

impl VapidKeys {
    /// 秘密鍵と公開鍵のバイト列から作る。公開鍵は秘密鍵から導いた値と一致する必要がある。
    pub fn new(private_key: &[u8], public_key: &[u8], subject: &str) -> Result<Self, Error> {
        let private_key =
            SecretKey::from_slice(private_key).map_err(|_| Error::InvalidVapidPrivateKey)?;
        let public_key =
            PublicKey::from_sec1_bytes(public_key).map_err(|_| Error::InvalidVapidPublicKey)?;
        if private_key.public_key() != public_key {
            return Err(Error::InvalidVapidPublicKey);
        }
        if subject.trim().is_empty() {
            return Err(Error::InvalidJwt);
        }
        Ok(Self {
            private_key,
            public_key,
            subject: subject.to_owned(),
        })
    }

    /// 公開鍵の base64url。
    pub fn public_key(&self) -> String {
        base64url::encode(self.public_key.to_sec1_point(false).as_bytes())
    }

    /// subject。
    pub fn subject(&self) -> &str {
        &self.subject
    }
}

/// 送信の要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRequest {
    /// 購読の endpoint。
    pub endpoint: String,
    /// 購読の公開鍵 (base64url)。
    pub p256dh: String,
    /// 購読の認証の秘密 (base64url)。
    pub auth: String,
    /// 通知の本文。
    pub payload: Vec<u8>,
    /// `TTL` (秒)。
    pub ttl: u32,
}

/// 暗号に使う salt と一時鍵。
#[derive(Debug, Clone)]
pub struct PushRandomness {
    salt: [u8; SALT_LEN],
    ephemeral_secret: SecretKey,
}

impl PushRandomness {
    /// salt と一時鍵のバイト列から作る。
    pub fn new(salt: [u8; SALT_LEN], ephemeral_secret: &[u8]) -> Result<Self, Error> {
        let ephemeral_secret =
            SecretKey::from_slice(ephemeral_secret).map_err(|_| Error::InvalidVapidPrivateKey)?;
        Ok(Self {
            salt,
            ephemeral_secret,
        })
    }
}

/// 送信する HTTP の要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedPush {
    /// HTTP のメソッド。
    pub method: &'static str,
    /// 送信先の URL。
    pub url: String,
    /// 要求のヘッダ。
    pub headers: Vec<(String, String)>,
    /// 要求の本文。
    pub body: Vec<u8>,
}

/// VAPID の JWT のクレーム。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VapidClaims {
    /// プッシュサービスの origin。
    pub aud: String,
    /// 有効期限の Unix 時刻 (秒)。
    pub exp: i64,
    /// subject。
    pub sub: String,
}

/// endpoint の origin を返す。
pub fn endpoint_origin(endpoint: &str) -> Result<String, Error> {
    let rest = endpoint
        .strip_prefix("https://")
        .ok_or(Error::InvalidEndpoint)?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or(Error::InvalidEndpoint)?;
    if authority.contains('@') {
        return Err(Error::InvalidEndpoint);
    }
    Ok(format!("https://{authority}"))
}

/// VAPID の JWT を組み立てる。`exp` は `now` より後で、24 時間以内でなければならない。
pub fn vapid_jwt(vapid: &VapidKeys, audience: &str, exp: i64, now: i64) -> Result<String, Error> {
    if exp <= now || exp > now.saturating_add(VAPID_MAX_LIFETIME_SECONDS) {
        return Err(Error::InvalidExpiration);
    }
    let header = serde_json::json!({
        "typ": "JWT",
        "alg": "ES256",
        "kid": vapid.public_key(),
    });
    let claims = VapidClaims {
        aud: audience.to_owned(),
        exp,
        sub: vapid.subject.clone(),
    };
    let header = serde_json::to_vec(&header).map_err(|_| Error::InvalidJwt)?;
    let claims = serde_json::to_vec(&claims).map_err(|_| Error::InvalidJwt)?;
    let signing_input = format!(
        "{}.{}",
        base64url::encode(&header),
        base64url::encode(&claims)
    );
    let signing_key = SigningKey::from(&vapid.private_key);
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    Ok(format!(
        "{signing_input}.{}",
        base64url::encode(signature.to_bytes().as_ref())
    ))
}

/// VAPID の JWT を検証し、クレームを返す。
pub fn verify_vapid(
    token: &str,
    public_key: &[u8],
    expected_audience: &str,
    now: i64,
) -> Result<VapidClaims, Error> {
    let mut parts = token.split('.');
    let header_part = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or(Error::InvalidJwt)?;
    let claims_part = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or(Error::InvalidJwt)?;
    let signature_part = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or(Error::InvalidJwt)?;
    if parts.next().is_some() {
        return Err(Error::InvalidJwt);
    }
    let header: serde_json::Value =
        serde_json::from_slice(&base64url::decode(header_part).ok_or(Error::InvalidJwt)?)
            .map_err(|_| Error::InvalidJwt)?;
    if header.get("typ").and_then(serde_json::Value::as_str) != Some("JWT")
        || header.get("alg").and_then(serde_json::Value::as_str) != Some("ES256")
        || header.get("kid").and_then(serde_json::Value::as_str)
            != Some(base64url::encode(public_key).as_str())
    {
        return Err(Error::InvalidJwt);
    }
    let claims: VapidClaims =
        serde_json::from_slice(&base64url::decode(claims_part).ok_or(Error::InvalidJwt)?)
            .map_err(|_| Error::InvalidJwt)?;
    let signature =
        Signature::from_slice(&base64url::decode(signature_part).ok_or(Error::InvalidJwt)?)
            .map_err(|_| Error::InvalidSignature)?;
    let verifying_key =
        VerifyingKey::from_sec1_bytes(public_key).map_err(|_| Error::InvalidVapidPublicKey)?;
    let signing_input = format!("{header_part}.{claims_part}");
    verifying_key
        .verify(signing_input.as_bytes(), &signature)
        .map_err(|_| Error::InvalidSignature)?;
    if claims.aud != expected_audience {
        return Err(Error::InvalidAudience);
    }
    if claims.exp <= now || claims.exp > now.saturating_add(VAPID_MAX_LIFETIME_SECONDS) {
        return Err(Error::InvalidExpiration);
    }
    Ok(claims)
}

/// 通知を暗号化した HTTP の要求を作る。
pub fn prepare(
    request: &PushRequest,
    vapid: &VapidKeys,
    now: i64,
    randomness: &PushRandomness,
) -> Result<PreparedPush, Error> {
    let origin = endpoint_origin(&request.endpoint)?;
    let p256dh = decode_public_key(&request.p256dh)?;
    let auth = decode_auth(&request.auth)?;
    let body = encrypt(
        &request.payload,
        &p256dh,
        &auth,
        &randomness.salt,
        &randomness.ephemeral_secret,
    )?;
    let exp = now.saturating_add(VAPID_LIFETIME_SECONDS);
    let token = vapid_jwt(vapid, &origin, exp, now)?;
    let authorization = format!("vapid t={token}, k={}", vapid.public_key());
    Ok(PreparedPush {
        method: "POST",
        url: request.endpoint.clone(),
        headers: vec![
            ("TTL".to_owned(), request.ttl.to_string()),
            ("Urgency".to_owned(), URGENCY.to_owned()),
            ("Content-Encoding".to_owned(), CONTENT_ENCODING.to_owned()),
            (
                "Content-Type".to_owned(),
                "application/octet-stream".to_owned(),
            ),
            ("Authorization".to_owned(), authorization),
        ],
        body,
    })
}

/// RFC 8291 の aes128gcm で本文を暗号化する。
pub fn encrypt(
    payload: &[u8],
    p256dh: &PublicKey,
    auth: &[u8; AUTH_LEN],
    salt: &[u8; SALT_LEN],
    ephemeral_secret: &SecretKey,
) -> Result<Vec<u8>, Error> {
    let shared = diffie_hellman(ephemeral_secret.to_nonzero_scalar(), p256dh.as_affine());
    let ua_public = p256dh.to_sec1_point(false);
    let as_public = ephemeral_secret.public_key().to_sec1_point(false);
    let mut auth_info = Vec::with_capacity(13 + ua_public.len() + as_public.len());
    auth_info.extend_from_slice(b"WebPush: info\0");
    auth_info.extend_from_slice(ua_public.as_bytes());
    auth_info.extend_from_slice(as_public.as_bytes());
    let (cek, nonce) = derive_keys(shared.raw_secret_bytes().as_slice(), auth, salt, &auth_info)?;
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| Error::Encryption)?;
    let nonce = Nonce::from(nonce);
    let mut plaintext = Vec::with_capacity(payload.len() + 1);
    plaintext.extend_from_slice(payload);
    plaintext.push(0x02);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|_| Error::Encryption)?;
    let mut body = Vec::with_capacity(SALT_LEN + 4 + 1 + as_public.len() + ciphertext.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(u8::try_from(as_public.len()).map_err(|_| Error::Encryption)?);
    body.extend_from_slice(as_public.as_bytes());
    body.extend_from_slice(&ciphertext);
    Ok(body)
}

/// RFC 8291 の aes128gcm の本文を復号する。
pub fn decrypt(
    body: &[u8],
    ua_secret: &SecretKey,
    auth: &[u8; AUTH_LEN],
) -> Result<Vec<u8>, Error> {
    if body.len() <= SALT_LEN + 4 + 1 {
        return Err(Error::InvalidBody);
    }
    let salt: [u8; SALT_LEN] = body[..SALT_LEN]
        .try_into()
        .map_err(|_| Error::InvalidBody)?;
    let record_size = u32::from_be_bytes(
        body[SALT_LEN..SALT_LEN + 4]
            .try_into()
            .map_err(|_| Error::InvalidBody)?,
    );
    if record_size != RECORD_SIZE {
        return Err(Error::InvalidBody);
    }
    let key_id_len = usize::from(body[SALT_LEN + 4]);
    let key_id_start = SALT_LEN + 5;
    let key_id_end = key_id_start
        .checked_add(key_id_len)
        .ok_or(Error::InvalidBody)?;
    if key_id_len == 0 || body.len() <= key_id_end {
        return Err(Error::InvalidBody);
    }
    let as_public = PublicKey::from_sec1_bytes(&body[key_id_start..key_id_end])
        .map_err(|_| Error::InvalidBody)?;
    let shared = diffie_hellman(ua_secret.to_nonzero_scalar(), as_public.as_affine());
    let ua_public = ua_secret.public_key().to_sec1_point(false);
    let as_encoded = as_public.to_sec1_point(false);
    let mut auth_info = Vec::with_capacity(13 + ua_public.len() + as_encoded.len());
    auth_info.extend_from_slice(b"WebPush: info\0");
    auth_info.extend_from_slice(ua_public.as_bytes());
    auth_info.extend_from_slice(as_encoded.as_bytes());
    let (cek, nonce) = derive_keys(
        shared.raw_secret_bytes().as_slice(),
        auth,
        &salt,
        &auth_info,
    )?;
    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| Error::Decryption)?;
    let nonce = Nonce::from(nonce);
    let plaintext = cipher
        .decrypt(&nonce, &body[key_id_end..])
        .map_err(|_| Error::Decryption)?;
    let end = plaintext
        .iter()
        .rposition(|byte| *byte != 0)
        .ok_or(Error::Decryption)?;
    if plaintext[end] != 0x02 {
        return Err(Error::Decryption);
    }
    Ok(plaintext[..end].to_vec())
}

fn derive_keys(
    ecdh_secret: &[u8],
    auth: &[u8; AUTH_LEN],
    salt: &[u8; SALT_LEN],
    auth_info: &[u8],
) -> Result<([u8; CEK_LEN], [u8; NONCE_LEN]), Error> {
    let hkdf = Hkdf::<Sha256>::new(Some(auth), ecdh_secret);
    let mut ikm = [0u8; IKM_LEN];
    hkdf.expand(auth_info, &mut ikm)
        .map_err(|_| Error::Encryption)?;
    let hkdf = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0u8; CEK_LEN];
    hkdf.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| Error::Encryption)?;
    let mut nonce = [0u8; NONCE_LEN];
    hkdf.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| Error::Encryption)?;
    Ok((cek, nonce))
}

fn decode_public_key(value: &str) -> Result<PublicKey, Error> {
    let bytes = base64url::decode(value).ok_or(Error::InvalidPublicKey)?;
    PublicKey::from_sec1_bytes(&bytes).map_err(|_| Error::InvalidPublicKey)
}

fn decode_auth(value: &str) -> Result<[u8; AUTH_LEN], Error> {
    let bytes = base64url::decode(value).ok_or(Error::InvalidAuthSecret)?;
    bytes.try_into().map_err(|_| Error::InvalidAuthSecret)
}

#[cfg(test)]
mod tests {
    use super::{
        decrypt, encrypt, endpoint_origin, prepare, vapid_jwt, verify_vapid, Error, PushRandomness,
        PushRequest, VapidKeys, AUTH_LEN, SALT_LEN,
    };
    use p256::elliptic_curve::sec1::ToSec1Point;

    fn decode(value: &str) -> Vec<u8> {
        crate::base64url::decode(value).expect("the test vector must be base64url")
    }

    #[test]
    fn req_tmr_009_the_rfc_8291_vector_decrypts_to_the_plaintext() {
        let ua_private = decode("q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94");
        let auth = decode("BTBZMqHH6r4Tts7J_aSIgg");
        let body = decode(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
        );
        let ua_secret =
            p256::SecretKey::from_slice(&ua_private).expect("the receiver key is a scalar");
        let auth: [u8; AUTH_LEN] = auth.try_into().expect("the auth secret is 16 bytes");
        let plaintext = decrypt(&body, &ua_secret, &auth).expect("the vector must decrypt");
        assert_eq!(plaintext, b"When I grow up, I want to be a watermelon");
    }

    #[test]
    fn req_tmr_009_the_rfc_8291_vector_encrypts_to_the_same_body() {
        let ua_public =
            decode("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4");
        let auth = decode("BTBZMqHH6r4Tts7J_aSIgg");
        let as_private = decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw");
        let salt = decode("DGv6ra1nlYgDCS1FRnbzlw");
        let body = decode(
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN",
        );
        let public =
            p256::PublicKey::from_sec1_bytes(&ua_public).expect("the receiver key is a point");
        let secret = p256::SecretKey::from_slice(&as_private).expect("the sender key is a scalar");
        let auth: [u8; AUTH_LEN] = auth.try_into().expect("the auth secret is 16 bytes");
        let salt: [u8; SALT_LEN] = salt.try_into().expect("the salt is 16 bytes");
        assert_eq!(
            encrypt(
                b"When I grow up, I want to be a watermelon",
                &public,
                &auth,
                &salt,
                &secret
            )
            .expect("the vector must encrypt"),
            body
        );
    }

    #[test]
    fn req_tmr_009_the_vapid_token_verifies_and_carries_the_audience() {
        let secret = p256::SecretKey::from_slice(&[1u8; 32]).expect("the test key is a scalar");
        let public = secret.public_key();
        let vapid = VapidKeys::new(
            secret.to_bytes().as_slice(),
            public.to_sec1_point(false).as_bytes(),
            "mailto:owner@example.com",
        )
        .expect("the VAPID keys must be valid");
        let token = vapid_jwt(
            &vapid,
            "https://push.example.net",
            1_700_000_100,
            1_700_000_000,
        )
        .expect("the token must be signed");
        let claims = verify_vapid(
            &token,
            public.to_sec1_point(false).as_bytes(),
            "https://push.example.net",
            1_700_000_000,
        )
        .expect("the token must verify");
        assert_eq!(claims.aud, "https://push.example.net");
        assert_eq!(claims.sub, "mailto:owner@example.com");
        assert_eq!(claims.exp, 1_700_000_100);
        assert_eq!(
            verify_vapid(
                &token,
                public.to_sec1_point(false).as_bytes(),
                "https://other.example.net",
                1_700_000_000
            ),
            Err(Error::InvalidAudience)
        );
    }

    #[test]
    fn req_tmr_009_the_expiration_boundary_is_enforced() {
        let secret = p256::SecretKey::from_slice(&[1u8; 32]).expect("the test key is a scalar");
        let public = secret.public_key();
        let vapid = VapidKeys::new(
            secret.to_bytes().as_slice(),
            public.to_sec1_point(false).as_bytes(),
            "mailto:owner@example.com",
        )
        .expect("the VAPID keys must be valid");
        assert_eq!(
            vapid_jwt(
                &vapid,
                "https://push.example.net",
                1_700_000_000,
                1_700_000_000
            ),
            Err(Error::InvalidExpiration)
        );
        assert_eq!(
            vapid_jwt(
                &vapid,
                "https://push.example.net",
                1_700_000_000 + super::VAPID_MAX_LIFETIME_SECONDS + 1,
                1_700_000_000
            ),
            Err(Error::InvalidExpiration)
        );
        assert!(vapid_jwt(
            &vapid,
            "https://push.example.net",
            1_700_000_000 + super::VAPID_MAX_LIFETIME_SECONDS,
            1_700_000_000
        )
        .is_ok());
    }

    #[test]
    fn req_tmr_009_the_prepared_request_uses_high_urgency_and_the_given_ttl() {
        let receiver =
            p256::SecretKey::from_slice(&[2u8; 32]).expect("the receiver key is a scalar");
        let sender = p256::SecretKey::from_slice(&[3u8; 32]).expect("the sender key is a scalar");
        let receiver_public =
            crate::base64url::encode(receiver.public_key().to_sec1_point(false).as_bytes());
        let auth = crate::base64url::encode(&[7u8; AUTH_LEN]);
        let request = PushRequest {
            endpoint: "https://push.example.net/send/abc".to_owned(),
            p256dh: receiver_public,
            auth,
            payload: b"rest finished".to_vec(),
            ttl: 30,
        };
        let vapid_secret =
            p256::SecretKey::from_slice(&[4u8; 32]).expect("the VAPID key is a scalar");
        let vapid_public = vapid_secret.public_key();
        let vapid = VapidKeys::new(
            vapid_secret.to_bytes().as_slice(),
            vapid_public.to_sec1_point(false).as_bytes(),
            "mailto:owner@example.com",
        )
        .expect("the VAPID keys must be valid");
        let randomness = PushRandomness::new([9u8; SALT_LEN], sender.to_bytes().as_slice())
            .expect("the randomness must be valid");
        let prepared = prepare(&request, &vapid, 1_700_000_000, &randomness)
            .expect("the request must be prepared");
        assert_eq!(prepared.method, "POST");
        assert_eq!(prepared.url, request.endpoint);
        assert!(prepared
            .headers
            .iter()
            .any(|(name, value)| name == "Urgency" && value == "high"));
        assert!(prepared
            .headers
            .iter()
            .any(|(name, value)| name == "TTL" && value == "30"));
        assert!(prepared
            .headers
            .iter()
            .any(|(name, value)| name == "Content-Encoding" && value == "aes128gcm"));
        let auth: [u8; AUTH_LEN] = [7u8; AUTH_LEN];
        assert_eq!(
            decrypt(&prepared.body, &receiver, &auth).expect("the body must decrypt"),
            request.payload
        );
    }

    #[test]
    fn req_tmr_009_the_endpoint_origin_is_the_audience() {
        assert_eq!(
            endpoint_origin("https://push.example.net/send/abc?x=1"),
            Ok("https://push.example.net".to_owned())
        );
        assert_eq!(
            endpoint_origin("http://push.example.net/send/abc"),
            Err(Error::InvalidEndpoint)
        );
        assert_eq!(
            endpoint_origin("https://push.example.net:8443/send/abc"),
            Ok("https://push.example.net:8443".to_owned())
        );
        assert_eq!(
            endpoint_origin("https://user@push.example.net/send/abc"),
            Err(Error::InvalidEndpoint)
        );
    }
}
