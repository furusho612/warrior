//! トレーニングの終了、サマリー、保存済みの記録の取得と引き継ぎの API。

use warrior_core::training_record::TrainingRecord;
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
    /// トレーニングが見つからない。
    NotFound,
    /// 編集権が無い、または記録の状態が変わった。
    Forbidden,
    /// 終了済みのトレーニングは引き継げない。
    Conflict,
    /// 応答の本文が期待する形に一致しない。
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

/// サーバーに保存済みのトレーニングの全体を返す。
pub async fn record(training_id: &str) -> Result<TrainingRecord, TrainingApiError> {
    request_record("GET", &format!("/api/trainings/{training_id}")).await
}

/// トレーニングの編集権をこの端末へ引き継ぎ、保存済みの記録を返す。
pub async fn takeover(training_id: &str) -> Result<TrainingRecord, TrainingApiError> {
    request_record("POST", &format!("/api/trainings/{training_id}/takeover")).await
}

async fn request(method: &str, url: &str) -> Result<TrainingSessionSummary, TrainingApiError> {
    let body = request_body(method, url).await?;
    serde_json::from_str(&body).map_err(|_| TrainingApiError::InvalidResponse)
}

async fn request_record(method: &str, url: &str) -> Result<TrainingRecord, TrainingApiError> {
    let body = request_body(method, url).await?;
    serde_json::from_str(&body).map_err(|_| TrainingApiError::InvalidResponse)
}

async fn request_body(method: &str, url: &str) -> Result<String, TrainingApiError> {
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
        return Err(status_error(response.status()));
    }
    let body = JsFuture::from(response.text().map_err(|_| TrainingApiError::Unavailable)?)
        .await
        .map_err(|_| TrainingApiError::Unavailable)?;
    body.as_string().ok_or(TrainingApiError::InvalidResponse)
}

fn status_error(status: u16) -> TrainingApiError {
    match status {
        403 => TrainingApiError::Forbidden,
        404 => TrainingApiError::NotFound,
        409 => TrainingApiError::Conflict,
        _ => TrainingApiError::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::{status_error, TrainingApiError};
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn the_status_codes_map_to_the_failures_the_screen_shows() {
        assert_eq!(status_error(403), TrainingApiError::Forbidden);
        assert_eq!(status_error(404), TrainingApiError::NotFound);
        assert_eq!(status_error(409), TrainingApiError::Conflict);
        assert_eq!(status_error(400), TrainingApiError::Rejected);
        assert_eq!(status_error(500), TrainingApiError::Rejected);
    }
}
