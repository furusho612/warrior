//! Web Crypto の乱数。
//!
//! 暗号学的な乱数は自前で実装せず、Workers の `crypto.getRandomValues` を呼ぶ (ADR-025)。

use js_sys::{Function, Reflect, Uint8Array};
use warrior_core::base64url;
use wasm_bindgen::{JsCast, JsValue};
use worker::Result;

/// 指定した長さの乱数を作る。
pub fn random_bytes(length: usize) -> Result<Vec<u8>> {
    let global = js_sys::global();
    let crypto = Reflect::get(&global, &JsValue::from_str("crypto"))?;
    let function = Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))?;
    let function: Function = function.dyn_into()?;
    let values = Uint8Array::new_with_length(length as u32);
    function.call1(&crypto, &values)?;
    Ok(values.to_vec())
}

/// 32 バイトの乱数を base64url にした値。
pub fn random_token() -> Result<String> {
    Ok(base64url::encode(&random_bytes(32)?))
}

/// 16 バイトの乱数から UUID v4 を作る。
pub fn random_uuid() -> Result<String> {
    let bytes: [u8; 16] = random_bytes(16)?
        .try_into()
        .map_err(|_| worker::Error::RustError("random bytes had an unexpected length".into()))?;
    Ok(warrior_core::ids::uuid_v4_from_bytes(bytes))
}
