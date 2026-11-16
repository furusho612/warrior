//! Web Push の VAPID と aes128gcm の性質テスト。
//!
//! 対応する要求: `REQ-TMR-009`。

use p256::elliptic_curve::sec1::ToSec1Point;
use p256::SecretKey;
use proptest::prelude::*;
use warrior_core::base64url;
use warrior_core::web_push::{
    decrypt, encrypt, vapid_jwt, verify_vapid, VapidClaims, VapidKeys, AUTH_LEN, SALT_LEN,
    VAPID_MAX_LIFETIME_SECONDS,
};

const NOW: i64 = 1_700_000_000;

fn scalar() -> impl Strategy<Value = SecretKey> {
    any::<[u8; 32]>().prop_filter_map("the bytes must be a P-256 scalar", |bytes| {
        SecretKey::from_slice(&bytes).ok()
    })
}

fn subject() -> impl Strategy<Value = String> {
    "[a-z]{1,20}@example\\.test".prop_map(|value| format!("mailto:{value}"))
}

fn audience() -> impl Strategy<Value = String> {
    "[a-z]{1,20}".prop_map(|value| format!("https://{value}.example.net"))
}

proptest! {
    #[test]
    fn req_tmr_009_the_vapid_claims_round_trip(
        secret in scalar(),
        subject in subject(),
        audience in audience(),
        lifetime in 1i64..=VAPID_MAX_LIFETIME_SECONDS,
    ) {
        let public = secret.public_key();
        let vapid = VapidKeys::new(
            secret.to_bytes().as_slice(),
            public.to_sec1_point(false).as_bytes(),
            &subject,
        )
        .expect("the generated keys must be valid");
        let token = vapid_jwt(&vapid, &audience, NOW + lifetime, NOW)
            .expect("the token must be signed");
        let claims = verify_vapid(&token, public.to_sec1_point(false).as_bytes(), &audience, NOW)
            .expect("the token must verify");
        let expected = VapidClaims {
            aud: audience,
            exp: NOW + lifetime,
            sub: subject,
        };
        prop_assert_eq!(claims, expected);
        let mut parts = token.split('.');
        let _header = parts.next().expect("a header");
        let payload = parts.next().expect("a payload");
        let decoded: VapidClaims = serde_json::from_slice(
            &base64url::decode(payload).expect("the payload must be base64url"),
        )
        .expect("the payload must be JSON");
        prop_assert_eq!(decoded.exp, NOW + lifetime);
    }

    #[test]
    fn req_tmr_009_the_aes128gcm_body_round_trips_for_any_payload_and_keys(
        payload in prop::collection::vec(any::<u8>(), 0..1024),
        receiver_secret in scalar(),
        sender_secret in scalar(),
        auth in any::<[u8; AUTH_LEN]>(),
        salt in any::<[u8; SALT_LEN]>(),
    ) {
        let receiver = receiver_secret.public_key();
        let body = encrypt(&payload, &receiver, &auth, &salt, &sender_secret)
            .expect("the payload must encrypt");
        let decrypted = decrypt(&body, &receiver_secret, &auth)
            .expect("the payload must decrypt");
        prop_assert_eq!(decrypted, payload);
    }
}
