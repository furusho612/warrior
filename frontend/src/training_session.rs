//! トレーニングの終了と、終了したサマリーの取得の API。

use warrior_core::training_session::TrainingSessionSummary;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response};

/// トレーニングの API の失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingApiError {
    /// 通信できない、または応答が返らない。
    Unavailable,
    /// サーバーが要求を受け付けない。
    Rejected,
    /// 応答の本文がサマリーの形に一致しない。
    InvalidResponse,
}

/// トレーニングを終了し、サーバーが保存したサマリーを返す。
pub async fn finish(training_id: &str) -> Result<TrainingSessionSummary, TrainingApiError> {
    request("POST", &format!("/api/trainings/{training_id}/finish")).await
}

/// 終了したトレーニングのサマリーを返す。
pub async fn summary(training_id: &str) -> Result<TrainingSessionSummary, TrainingApiError> {
    request("GET", &format!("/api/trainings/{training_id}/summary")).await
}

async fn request(method: &str, url: &str) -> Result<TrainingSessionSummary, TrainingApiError> {
    let window = web_sys::window().ok_or(TrainingApiError::Unavailable)?;
    let headers = Headers::new().map_err(|_| TrainingApiError::Unavailable)?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|_| TrainingApiError::Unavailable)?;
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(&headers);
    let request =
        Request::new_with_str_and_init(url, &init).map_err(|_| TrainingApiError::Unavailable)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| TrainingApiError::Unavailable)?;
    let response: Response = response
        .dyn_into()
        .map_err(|_| TrainingApiError::Unavailable)?;
    if !response.ok() {
        return Err(TrainingApiError::Rejected);
    }
    let body = JsFuture::from(response.text().map_err(|_| TrainingApiError::Unavailable)?)
        .await
        .map_err(|_| TrainingApiError::Unavailable)?;
    let body: String = body.as_string().ok_or(TrainingApiError::InvalidResponse)?;
    serde_json::from_str(&body).map_err(|_| TrainingApiError::InvalidResponse)
}
