//! base64url の符号化と復号 (RFC 4648 §5)。
//!
//! パディングは付けず、`+` と `/` の代わりに `-` と `_` を使う。
//! WebAuthn、Web Push、カーソルが同じ文字集合を共有する。

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// バイト列を base64url の文字列にする。
pub fn encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = u32::from(chunk.get(1).copied().unwrap_or(0));
        let third = u32::from(chunk.get(2).copied().unwrap_or(0));
        let bits = (first << 16) | (second << 8) | third;

        encoded.push(char::from(ALPHABET[((bits >> 18) & 0x3f) as usize]));
        encoded.push(char::from(ALPHABET[((bits >> 12) & 0x3f) as usize]));
        if chunk.len() > 1 {
            encoded.push(char::from(ALPHABET[((bits >> 6) & 0x3f) as usize]));
        }
        if chunk.len() > 2 {
            encoded.push(char::from(ALPHABET[(bits & 0x3f) as usize]));
        }
    }
    encoded
}

/// base64url の文字列をバイト列に戻す。
///
/// パディング、空白、文字集合の外の文字、正準でない末尾のビットを拒否する。
pub fn decode(encoded: &str) -> Option<Vec<u8>> {
    if encoded.is_empty() {
        return Some(Vec::new());
    }

    let bytes = encoded.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }

    let mut decoded = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut buffered_bits = 0u32;
    for byte in bytes {
        buffer = (buffer << 6) | u32::from(decode_character(*byte)?);
        buffered_bits += 6;
        while buffered_bits >= 8 {
            buffered_bits -= 8;
            decoded.push(((buffer >> buffered_bits) & 0xff) as u8);
        }
    }

    let trailing_mask = (1u32 << buffered_bits) - 1;
    if buffer & trailing_mask != 0 {
        return None;
    }
    Some(decoded)
}

/// base64url の 1 文字を 6 ビットの値にする。
fn decode_character(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn encodes_rfc_4648_examples_without_padding() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg");
        assert_eq!(encode(b"fo"), "Zm8");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg");
        assert_eq!(encode(b"fooba"), "Zm9vYmE");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn decodes_rfc_4648_examples() {
        assert_eq!(decode("Zg"), Some(b"f".to_vec()));
        assert_eq!(decode("Zm9vYmFy"), Some(b"foobar".to_vec()));
    }

    #[test]
    fn rejects_padding() {
        assert_eq!(decode("Zg=="), None);
    }

    #[test]
    fn rejects_non_canonical_trailing_bits() {
        assert_eq!(decode("Zh"), None);
    }

    #[test]
    fn rejects_invalid_length() {
        assert_eq!(decode("Z"), None);
    }

    #[test]
    fn rejects_characters_outside_the_alphabet() {
        assert_eq!(decode("Z!"), None);
        assert_eq!(decode("Z+"), None);
        assert_eq!(decode("Z/"), None);
    }
}
