//! 同じオリジンの API への要求と、JSON の応答の復元。
//!
//! 画面は応答の型だけを持ち、要求の組み立てと失敗の分類はここに集める。

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::{JsCast, JsValue};
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
    /// 要求の本文を JSON にできない。
    InvalidRequest,
}

/// 同じオリジンへ要求を送り、応答の JSON を復元する。
pub async fn request_json<T: DeserializeOwned>(method: &str, url: &str) -> Result<T, HttpError> {
    read_json(send_ok(method, url, None).await?).await
}

/// 同じオリジンへ JSON の本文を送り、応答の JSON を復元する。
pub async fn request_json_with_body<T: DeserializeOwned, B: Serialize>(
    method: &str,
    url: &str,
    body: &B,
) -> Result<T, HttpError> {
    let body = serde_json::to_string(body).map_err(|_| HttpError::InvalidRequest)?;
    read_json(send_ok(method, url, Some(body)).await?).await
}

/// 同じオリジンへ JSON の本文を送り、応答の本文を読まない。
pub async fn send_json_without_response<B: Serialize>(
    method: &str,
    url: &str,
    body: &B,
) -> Result<(), HttpError> {
    let body = serde_json::to_string(body).map_err(|_| HttpError::InvalidRequest)?;
    send_ok(method, url, Some(body)).await.map(|_| ())
}

/// 同じオリジンへ要求を送り、応答の状態コードを返す。
pub async fn request_status(method: &str, url: &str) -> Result<u16, HttpError> {
    Ok(send(method, url, None, None).await?.status())
}

/// 同じオリジンへ JSON の本文を送り、応答の状態コードを返す。
pub async fn request_status_with_body<B: Serialize>(
    method: &str,
    url: &str,
    body: &B,
) -> Result<u16, HttpError> {
    let body = serde_json::to_string(body).map_err(|_| HttpError::InvalidRequest)?;
    Ok(send(method, url, Some(body), None).await?.status())
}

/// 操作 ID を添えて JSON の本文を送り、応答の状態コードを返す。
pub async fn request_status_with_operation<B: Serialize>(
    method: &str,
    url: &str,
    operation_id: &str,
    body: &B,
) -> Result<u16, HttpError> {
    let body = serde_json::to_string(body).map_err(|_| HttpError::InvalidRequest)?;
    Ok(send(method, url, Some(body), Some(operation_id))
        .await?
        .status())
}

/// 操作 ID を添えて本文の無い要求を送り、応答の状態コードを返す。
pub async fn request_status_for_operation(
    method: &str,
    url: &str,
    operation_id: &str,
) -> Result<u16, HttpError> {
    Ok(send(method, url, None, Some(operation_id)).await?.status())
}

/// 同じオリジンへ要求を送り、成功した応答だけを返す。
async fn send_ok(method: &str, url: &str, body: Option<String>) -> Result<Response, HttpError> {
    let response = send(method, url, body, None).await?;
    if !response.ok() {
        return Err(HttpError::Rejected);
    }
    Ok(response)
}

async fn send(
    method: &str,
    url: &str,
    body: Option<String>,
    operation_id: Option<&str>,
) -> Result<Response, HttpError> {
    let window = web_sys::window().ok_or(HttpError::Unavailable)?;
    let headers = Headers::new().map_err(|_| HttpError::Unavailable)?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| HttpError::Unavailable)?;
    if let Some(operation_id) = operation_id {
        headers
            .set("X-Operation-Id", operation_id)
            .map_err(|_| HttpError::Unavailable)?;
    }
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(&headers);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(&body));
    }
    let request = Request::new_with_str_and_init(url, &init).map_err(|_| HttpError::Unavailable)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| HttpError::Unavailable)?;
    response.dyn_into().map_err(|_| HttpError::Unavailable)
}

async fn read_json<T: DeserializeOwned>(response: Response) -> Result<T, HttpError> {
    let body = JsFuture::from(response.text().map_err(|_| HttpError::Unavailable)?)
        .await
        .map_err(|_| HttpError::Unavailable)?;
    let body: String = body.as_string().ok_or(HttpError::InvalidResponse)?;
    serde_json::from_str(&body).map_err(|_| HttpError::InvalidResponse)
}
