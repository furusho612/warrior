//! 端末で作る ID と、端末の時計から作る ISO 8601 の時刻。
//!
//! ID は UUID v4 とし、サーバーの操作の冪等性の鍵にする (ADR-002)。
//! 時刻は表示と端末内の行の更新にだけ使い、サーバーが受理した時刻の代わりにしない。

use warrior_core::datetime;
use warrior_core::ids::uuid_v4_from_bytes;

use crate::storage::StorageError;

/// ブラウザの暗号乱数から UUID v4 を作る。
pub fn new_uuid() -> Result<String, StorageError> {
    let window = web_sys::window()
        .ok_or_else(|| StorageError::Unavailable("the window is missing".to_owned()))?;
    let crypto = window
        .crypto()
        .map_err(|error| StorageError::Unavailable(format!("{error:?}")))?;
    let mut bytes = [0u8; 16];
    crypto
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|error| StorageError::Unavailable(format!("{error:?}")))?;
    Ok(uuid_v4_from_bytes(bytes))
}

/// 現在の時刻を ISO 8601 の UTC の文字列にする。
pub fn now_iso() -> String {
    let seconds = (js_sys::Date::now() / 1000.0) as i64;
    datetime::format_utc(seconds)
}
