//! 認証のセッションと UUID の性質テスト。
//!
//! 対応する要求: `REQ-NFR-006`、`REQ-NFR-013`。
//! セッションのハッシュ、Cookie の属性、所有者の比較、UUID の生成を確かめる。

use proptest::prelude::*;
use warrior_core::auth::{
    clear_session_cookie, hash_session_token, is_owner, oauth_state_cookie, session_cookie,
    sha256_base64url, OAUTH_STATE_COOKIE, SESSION_COOKIE,
};
use warrior_core::base64url;
use warrior_core::ids::{is_valid_uuid_v4, uuid_v4_from_bytes};

proptest! {
    #[test]
    fn generated_session_tokens_are_valid_and_hash_to_a_fixed_length(
        bytes in proptest::collection::vec(any::<u8>(), 32),
    ) {
        let token = base64url::encode(&bytes);
        prop_assert_eq!(token.len(), 43);
        let hash = hash_session_token(&token);
        prop_assert_eq!(hash.len(), 43);
        prop_assert_eq!(&hash, &sha256_base64url(token.as_bytes()));
        prop_assert!(!hash.contains('='));
    }

    #[test]
    fn session_cookies_always_carry_the_expected_attributes(token in ".{0,64}") {
        let cookie = session_cookie(&token);
        let expected_prefix = format!("{SESSION_COOKIE}={token};");
        prop_assert!(cookie.starts_with(&expected_prefix));
        prop_assert!(cookie.contains("HttpOnly"));
        prop_assert!(cookie.contains("Secure"));
        prop_assert!(cookie.contains("SameSite=Lax"));
        prop_assert!(cookie.contains("Max-Age=2592000"));
        prop_assert!(clear_session_cookie().contains("Max-Age=0"));
        let state_prefix = format!("{OAUTH_STATE_COOKIE}={token};");
        prop_assert!(oauth_state_cookie(&token).starts_with(&state_prefix));
    }

    #[test]
    fn ownership_is_reflexive_only_for_non_empty_identifiers(
        left in ".{0,12}",
        right in ".{0,12}",
    ) {
        prop_assert_eq!(is_owner(&left, &right), !left.is_empty() && left == right);
    }

    #[test]
    fn uuid_v4_generation_always_produces_the_canonical_shape(bytes in any::<[u8; 16]>()) {
        let text = uuid_v4_from_bytes(bytes);
        prop_assert!(is_valid_uuid_v4(&text));
        prop_assert_eq!(&text[14..15], "4");
        prop_assert!(matches!(&text[19..20], "8" | "9" | "a" | "b"));
        prop_assert_eq!(text.len(), 36);
    }
}
