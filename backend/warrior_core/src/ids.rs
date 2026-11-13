//! UUID v4 の文字列の検証。
//!
//! 主キーは UUID v4 の小文字、ハイフン付きの文字列で保存する (ADR-015)。

/// UUID の文字列表記の長さ (ハイフンを含む 36 文字)。
pub const UUID_V4_TEXT_LEN: usize = 36;

/// UUID v4 の形式を満たすかを判定する。
///
/// 小文字の 16 進、8-4-4-4-12 の区切り、バージョン 4、RFC 4122 の variant を要求する。
/// 大文字、区切りの欠落、長さの違う値を拒否する。
pub fn is_valid_uuid_v4(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != UUID_V4_TEXT_LEN {
        return false;
    }

    bytes.iter().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => *byte == b'-',
        14 => *byte == b'4',
        19 => matches!(byte, b'8' | b'9' | b'a' | b'b'),
        _ => matches!(byte, b'0'..=b'9' | b'a'..=b'f'),
    })
}

#[cfg(test)]
mod tests {
    use super::is_valid_uuid_v4;

    const VALID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    #[test]
    fn accepts_lowercase_version_4_uuid() {
        assert!(is_valid_uuid_v4(VALID));
    }

    #[test]
    fn rejects_uppercase_uuid() {
        assert!(!is_valid_uuid_v4("3F2504E0-4F89-41D3-9A0C-0305E82C3301"));
    }

    #[test]
    fn rejects_uuid_without_hyphens() {
        assert!(!is_valid_uuid_v4("3f2504e04f8941d39a0c0305e82c3301"));
    }

    #[test]
    fn rejects_uuid_with_wrong_length() {
        assert!(!is_valid_uuid_v4("3f2504e0-4f89-41d3-9a0c-0305e82c330"));
    }

    #[test]
    fn rejects_uuid_with_wrong_version() {
        assert!(!is_valid_uuid_v4("3f2504e0-4f89-31d3-9a0c-0305e82c3301"));
    }

    #[test]
    fn rejects_uuid_with_wrong_variant() {
        assert!(!is_valid_uuid_v4("3f2504e0-4f89-41d3-ca0c-0305e82c3301"));
    }

    #[test]
    fn rejects_empty_text() {
        assert!(!is_valid_uuid_v4(""));
    }
}
