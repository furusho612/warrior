//! W3C WebAuthn Level 3 の公開テストベクタによる登録の検証。
//!
//! 出典: <https://www.w3.org/TR/webauthn-3/#sctn-test-vectors> の 16.2。
//! attestation は none、alg は ES256、UV は登録では不要なので、このベクタを成功の経路に使う。

use warrior_core::base64url;
use warrior_core::webauthn::{verify_registration, RegistrationInput};

const CHALLENGE: &str = "3be5aacd03537142472340ab5969f240f1d87716e20b6807ac230655fa4b3b49";
const CLIENT_DATA_JSON: &str = "7b2274797065223a22776562617574686e2e637265617465222c226368616c6c656e6765223a224f2d57717a514e5463554a484930437257576e7951504859647862694332674872434d475666704c4f306b222c226f726967696e223a2268747470733a2f2f6578616d706c652e6f7267222c2263726f73734f726967696e223a747275652c22657874726144617461223a22636c69656e74446174614a534f4e206d617920626520657874656e6465642077697468206164646974696f6e616c206669656c647320696e20746865206675747572652c207375636820617320746869733a207a5a7175457444523944577170573574425754467567227d";
const ATTESTATION_OBJECT: &str = "a363666d74646e6f6e656761747453746d74a068617574684461746158a4bfabc37432958b063360d3ad6461c9c4735ae7f8edd46592a5e0f01452b2e4b54500000000883f4f6014f19c09d87aa38123be48d000206e1050c0d2ca2f07c755cb2c66a74c64fa43065c18f938354d9915db2bd5ce57a501020326200121582022200a473f90b11078851550d03b4e44a2279f8c4eca27b3153dedfe03e4e97d225820cbd0be95e746ad6f5a8191be11756e4c0420e72f65b466d39bc56b8b123a9c6e";

fn hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
        .collect()
}

#[test]
fn the_webauthn_level_3_none_attestation_vector_registers() {
    let challenge = base64url::encode(&hex(CHALLENGE));
    let client_data_json = base64url::encode(&hex(CLIENT_DATA_JSON));
    let attestation_object = base64url::encode(&hex(ATTESTATION_OBJECT));
    let registered = verify_registration(&RegistrationInput {
        rp_id: "example.org",
        origin: "https://example.org",
        expected_challenge: &challenge,
        client_data_json: &client_data_json,
        attestation_object: &attestation_object,
    })
    .expect("the published vector must verify");
    assert_eq!(
        registered.credential_id,
        hex("6e1050c0d2ca2f07c755cb2c66a74c64fa43065c18f938354d9915db2bd5ce57")
    );
    assert_eq!(registered.sign_count, 0);
    assert_eq!(
        registered.cose_public_key,
        hex("a501020326200121582022200a473f90b11078851550d03b4e44a2279f8c4eca27b3153dedfe03e4e97d225820cbd0be95e746ad6f5a8191be11756e4c0420e72f65b466d39bc56b8b123a9c6e")
    );
}
