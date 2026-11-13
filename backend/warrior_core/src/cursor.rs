//! 一覧のページングに使うカーソルの符号化と復号。
//!
//! カーソルは並び順のキーの値と ID を JSON にして base64url で包む。
//! 復号できない値、空の値、形式の合わない値は拒否する。

use serde::{Deserialize, Serialize};

use crate::{base64url, ids};

const CURSOR_PREFIX: &str = "c1_";
const CURSOR_VERSION: u8 = 1;

/// カーソルが運ぶ並び順の位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// 並び順のキーの値。
    pub key: String,
    /// キーと組になる ID。
    pub id: String,
}

/// カーソルの復号の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorError {
    /// 空の文字列である。
    Empty,
    /// 接頭辞または base64url の形式が違う。
    InvalidEncoding,
    /// JSON の形式が違う。
    InvalidPayload,
    /// 対応しない版である。
    UnsupportedVersion,
    /// キーが空である。
    EmptyKey,
    /// ID が UUID v4 の形式でない。
    InvalidId,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorPayload {
    version: u8,
    key: String,
    id: String,
}

/// 並び順のキーの値と ID からカーソルを作る。
///
/// 空のキーと UUID v4 の形式でない ID は受け付けない。
pub fn encode_cursor(key: &str, id: &str) -> Option<String> {
    if key.is_empty() || !ids::is_valid_uuid_v4(id) {
        return None;
    }

    let payload = CursorPayload {
        version: CURSOR_VERSION,
        key: key.to_owned(),
        id: id.to_owned(),
    };
    let json = serde_json::to_vec(&payload).ok()?;
    Some(format!("{CURSOR_PREFIX}{}", base64url::encode(&json)))
}

/// カーソルを復号する。
pub fn decode_cursor(encoded: &str) -> Result<Cursor, CursorError> {
    if encoded.is_empty() {
        return Err(CursorError::Empty);
    }

    let encoded_payload = encoded
        .strip_prefix(CURSOR_PREFIX)
        .ok_or(CursorError::InvalidEncoding)?;
    let json = base64url::decode(encoded_payload).ok_or(CursorError::InvalidEncoding)?;
    let payload: CursorPayload =
        serde_json::from_slice(&json).map_err(|_| CursorError::InvalidPayload)?;

    if payload.version != CURSOR_VERSION {
        return Err(CursorError::UnsupportedVersion);
    }
    if payload.key.is_empty() {
        return Err(CursorError::EmptyKey);
    }
    if !ids::is_valid_uuid_v4(&payload.id) {
        return Err(CursorError::InvalidId);
    }

    Ok(Cursor {
        key: payload.key,
        id: payload.id,
    })
}

#[cfg(test)]
mod tests {
    use super::{decode_cursor, encode_cursor, CursorError};
    use crate::base64url;

    const ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const OTHER_ID: &str = "00000000-0000-4000-8000-000000000000";

    #[test]
    fn round_trips_key_and_id() {
        let encoded = encode_cursor("2026-11-12T00:00:00Z", ID).unwrap();
        let decoded = decode_cursor(&encoded).unwrap();

        assert_eq!(decoded.key, "2026-11-12T00:00:00Z");
        assert_eq!(decoded.id, ID);
    }

    #[test]
    fn rejects_empty_cursor() {
        assert_eq!(decode_cursor(""), Err(CursorError::Empty));
    }

    #[test]
    fn rejects_unknown_prefix() {
        let encoded = encode_cursor("key", ID).unwrap().replacen("c1_", "c2_", 1);
        assert_eq!(decode_cursor(&encoded), Err(CursorError::InvalidEncoding));
    }

    #[test]
    fn rejects_non_base64url_payload() {
        assert_eq!(
            decode_cursor("c1_not base64"),
            Err(CursorError::InvalidEncoding)
        );
    }

    #[test]
    fn rejects_invalid_json_payload() {
        let encoded = format!("c1_{}", base64url::encode(b"not json"));
        assert_eq!(decode_cursor(&encoded), Err(CursorError::InvalidPayload));
    }

    #[test]
    fn rejects_unknown_fields_in_payload() {
        let encoded = format!(
            "c1_{}",
            base64url::encode(
                format!(r#"{{"version":1,"key":"key","id":"{ID}","extra":true}}"#).as_bytes()
            )
        );
        assert_eq!(decode_cursor(&encoded), Err(CursorError::InvalidPayload));
    }

    #[test]
    fn rejects_unsupported_version() {
        let encoded = format!(
            "c1_{}",
            base64url::encode(format!(r#"{{"version":2,"key":"key","id":"{ID}"}}"#).as_bytes())
        );
        assert_eq!(
            decode_cursor(&encoded),
            Err(CursorError::UnsupportedVersion)
        );
    }

    #[test]
    fn rejects_empty_key_in_payload() {
        let encoded = format!(
            "c1_{}",
            base64url::encode(format!(r#"{{"version":1,"key":"","id":"{ID}"}}"#).as_bytes())
        );
        assert_eq!(decode_cursor(&encoded), Err(CursorError::EmptyKey));
    }

    #[test]
    fn rejects_invalid_id_in_payload() {
        let encoded = format!(
            "c1_{}",
            base64url::encode(r#"{"version":1,"key":"key","id":"not-a-uuid"}"#.as_bytes())
        );
        assert_eq!(decode_cursor(&encoded), Err(CursorError::InvalidId));
    }

    #[test]
    fn rejects_encoding_empty_key() {
        assert_eq!(encode_cursor("", ID), None);
    }

    #[test]
    fn accepts_another_valid_uuid() {
        let encoded = encode_cursor("key", OTHER_ID).unwrap();
        assert_eq!(decode_cursor(&encoded).unwrap().id, OTHER_ID);
    }
}
