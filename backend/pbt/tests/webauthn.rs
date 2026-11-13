//! WebAuthn のパーサと署名カウンタの性質テスト。
//!
//! 対応する要求: `REQ-NFR-013`、`REQ-NFR-014`。
//! base64url と CBOR と COSE の往復、署名カウンタの規則を確かめる。

use proptest::prelude::*;
use warrior_core::base64url;
use warrior_core::cbor;
use warrior_core::cose;
use warrior_core::webauthn::{check_sign_count, Error, SignCounter};

fn cbor_header(major: u8, length: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    if length < 24 {
        bytes.push((major << 5) | length as u8);
    } else if length < 256 {
        bytes.push((major << 5) | 24);
        bytes.push(length as u8);
    } else {
        bytes.push((major << 5) | 25);
        bytes.extend_from_slice(&(length as u16).to_be_bytes());
    }
    bytes
}

fn cbor_array_integer_bytes_text(integer: u8, bytes: &[u8], text: &str) -> Vec<u8> {
    let mut encoded = cbor_header(4, 3);
    encoded.extend_from_slice(&cbor_header(0, usize::from(integer)));
    encoded.extend_from_slice(&cbor_header(2, bytes.len()));
    encoded.extend_from_slice(bytes);
    encoded.extend_from_slice(&cbor_header(3, text.len()));
    encoded.extend_from_slice(text.as_bytes());
    encoded
}

fn es256_key(x: [u8; 32], y: [u8; 32]) -> Vec<u8> {
    let mut encoded = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20];
    encoded.extend_from_slice(&x);
    encoded.push(0x22);
    encoded.push(0x58);
    encoded.push(0x20);
    encoded.extend_from_slice(&y);
    encoded
}

proptest! {
    #[test]
    fn base64url_round_trips_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let encoded = base64url::encode(&bytes);
        prop_assert_eq!(base64url::decode(&encoded), Some(bytes));
    }

    #[test]
    fn cbor_round_trips_an_array(integer in any::<u8>(), bytes in proptest::collection::vec(any::<u8>(), 0..64), text in ".{0,64}") {
        let encoded = cbor_array_integer_bytes_text(integer, &bytes, &text);
        let decoded = cbor::decode(&encoded).expect("the encoded CBOR must decode");
        let values = decoded.as_array().expect("an array");
        prop_assert_eq!(values[0].as_integer(), Some(i64::from(integer)));
        prop_assert_eq!(values[1].as_bytes(), Some(bytes.as_slice()));
        prop_assert_eq!(values[2].as_text(), Some(text.as_str()));
    }

    #[test]
    fn cose_round_trips_arbitrary_p256_coordinates(x in any::<[u8; 32]>(), y in any::<[u8; 32]>()) {
        let parsed = cose::parse(&es256_key(x, y)).expect("the encoded COSE key must parse");
        prop_assert_eq!(parsed.x, x);
        prop_assert_eq!(parsed.y, y);
        prop_assert_eq!(parsed.to_sec1_bytes()[0], 0x04);
        prop_assert_eq!(&parsed.to_sec1_bytes()[1..33], &x);
        prop_assert_eq!(&parsed.to_sec1_bytes()[33..], &y);
    }

    #[test]
    fn sign_counter_only_accepts_an_increase(stored in any::<u32>(), current in any::<u32>()) {
        let result = check_sign_count(stored, current);
        if stored == 0 && current == 0 {
            prop_assert_eq!(result, Ok(SignCounter::Skipped));
        } else if current <= stored {
            prop_assert_eq!(result, Err(Error::SignCounterRegressed));
        } else {
            prop_assert_eq!(result, Ok(SignCounter::Updated(current)));
        }
    }
}

#[test]
fn a_non_es256_cose_key_is_rejected() {
    // W3C WebAuthn Level 3 の 16.8 の ES384 の鍵。alg が -35 なので ES256 ではない。
    let text = "a5010203382220022158304866bd8b01da789e9eb806e5eab05ae5a638542296ab057a2f1bbce9b58f8a08b9171390b58a37ac7fffc2c5f45857da2258302a0b024c7f4b72072a1f96bd30a7261aae9571dd39870eb29e55c0941c6b08e89629a1ea1216aa64ce57c2807bf3901a";
    let bytes = (0..text.len() / 2)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
        .collect::<Vec<u8>>();
    assert_eq!(cose::parse(&bytes), Err(cose::Error::UnsupportedAlgorithm));
}
