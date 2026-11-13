//! UUID v4 の形式の性質テスト。
//!
//! 対応する要求はない。ADR-015 が定める主キーの形式の内部条件を確かめる。

mod support;

use proptest::prelude::*;
use warrior_core::ids::is_valid_uuid_v4;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn accepts_generated_uuid_v4(text in support::uuid_v4_text()) {
        prop_assert!(is_valid_uuid_v4(&text));
    }

    #[test]
    fn rejects_uppercase_uuid_v4(text in support::uuid_v4_text()) {
        prop_assert!(!is_valid_uuid_v4(&text.to_ascii_uppercase()));
    }

    #[test]
    fn rejects_uuid_v4_without_hyphens(text in support::uuid_v4_text()) {
        prop_assert!(!is_valid_uuid_v4(&text.replace('-', "")));
    }

    #[test]
    fn rejects_uuid_v4_with_changed_length(text in support::uuid_v4_text()) {
        prop_assert!(!is_valid_uuid_v4(&text[..35]));
    }
}
