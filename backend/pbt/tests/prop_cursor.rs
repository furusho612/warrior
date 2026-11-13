//! カーソルの符号化と復号の性質テスト。
//!
//! 対応する要求はない。ADR-015 が定めるカーソル方式の内部条件を確かめる。

mod support;

use proptest::prelude::*;
use warrior_core::cursor::{decode_cursor, encode_cursor, CursorError};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn round_trips_key_and_id(key in ".+", id in support::uuid_v4_text()) {
        let encoded = encode_cursor(&key, &id).expect("encoded cursor");
        let decoded = decode_cursor(&encoded).expect("decoded cursor");

        prop_assert_eq!(decoded.key, key);
        prop_assert_eq!(decoded.id, id);
    }

    #[test]
    fn rejects_unknown_prefix(key in ".+", id in support::uuid_v4_text()) {
        let encoded = encode_cursor(&key, &id).expect("encoded cursor");
        let tampered = format!("c2_{}", &encoded[3..]);

        prop_assert_eq!(decode_cursor(&tampered), Err(CursorError::InvalidEncoding));
    }

    #[test]
    fn rejects_trailing_invalid_character(key in ".+", id in support::uuid_v4_text()) {
        let encoded = encode_cursor(&key, &id).expect("encoded cursor");
        let tampered = format!("{encoded}!");

        prop_assert_eq!(decode_cursor(&tampered), Err(CursorError::InvalidEncoding));
    }
}
