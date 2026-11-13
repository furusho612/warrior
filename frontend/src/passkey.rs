//! パスキーのブラウザ API を呼ぶ薄いラッパー。

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// `window.warriorPasskey.<name>` を呼び、返った JSON の文字列を返す。
pub async fn call(name: &str, argument: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or_else(|| "the window is missing".to_owned())?;
    let namespace = Reflect::get(&window, &JsValue::from_str("warriorPasskey"))
        .map_err(|error| format!("the passkey helper is missing: {error:?}"))?;
    let function = Reflect::get(&namespace, &JsValue::from_str(name))
        .map_err(|error| format!("the passkey function is missing: {error:?}"))?;
    let function: Function = function
        .dyn_into()
        .map_err(|_| "the passkey function is not callable".to_owned())?;
    let value = function
        .call1(&namespace, &JsValue::from_str(argument))
        .map_err(|error| format!("the passkey call failed: {error:?}"))?;
    let promise: Promise = value
        .dyn_into()
        .map_err(|_| "the passkey call did not return a promise".to_owned())?;
    let result = JsFuture::from(promise)
        .await
        .map_err(|error| format!("the passkey promise failed: {error:?}"))?;
    result
        .as_string()
        .ok_or_else(|| "the passkey result is not a string".to_owned())
}
