//! 同じオリジンの API への要求と、JSON の応答の復元。
//!
//! 画面は応答の型だけを持ち、要求の組み立てと失敗の分類はここに集める。

use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

/// API の要求の失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpError {
    /// 通信できない、または応答が返らない。
    Unavailable,
    /// サーバーが要求を受け付けない。
    Rejected,
    /// 応答の本文が期待した形に一致しない。
    InvalidResponse,
}

/// 同じオリジンへ要求を送り、応答の JSON を復元する。
pub async fn request_json<T: DeserializeOwned>(method: &str, url: &str) -> Result<T, HttpError> {
    let window = web_sys::window().ok_or(HttpError::Unavailable)?;
    let headers = Headers::new().map_err(|_| HttpError::Unavailable)?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| HttpError::Unavailable)?;
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(&headers);
    let request = Request::new_with_str_and_init(url, &init).map_err(|_| HttpError::Unavailable)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| HttpError::Unavailable)?;
    let response: Response = response.dyn_into().map_err(|_| HttpError::Unavailable)?;
    if !response.ok() {
        return Err(HttpError::Rejected);
    }
    let body = JsFuture::from(response.text().map_err(|_| HttpError::Unavailable)?)
        .await
        .map_err(|_| HttpError::Unavailable)?;
    let body: String = body.as_string().ok_or(HttpError::InvalidResponse)?;
    serde_json::from_str(&body).map_err(|_| HttpError::InvalidResponse)
}
