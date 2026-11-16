//! トレーニングの終了、サマリー、保存済みの記録の取得と引き継ぎの API。
//!
//! 要求の組み立ては `crate::http` に任せ、ここでは応答の型と画面の分岐だけを持つ。

use warrior_core::training_record::TrainingRecord;
use warrior_core::training_session::TrainingSessionSummary;

use crate::http::{self, HttpError};

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

impl From<HttpError> for TrainingApiError {
    fn from(error: HttpError) -> Self {
        match error {
            HttpError::Unavailable | HttpError::OriginRejected => Self::Unavailable,
            HttpError::Rejected | HttpError::InvalidRequest => Self::Rejected,
            HttpError::Forbidden => Self::Forbidden,
            HttpError::NotFound => Self::NotFound,
            HttpError::Conflict => Self::Conflict,
            HttpError::InvalidResponse => Self::InvalidResponse,
        }
    }
}

/// トレーニングを終了し、サーバーが保存したサマリーを返す。
pub async fn finish(training_id: &str) -> Result<TrainingSessionSummary, TrainingApiError> {
    http::request_json("POST", &format!("/api/trainings/{training_id}/finish"))
        .await
        .map_err(TrainingApiError::from)
}

/// 終了したトレーニングのサマリーを返す。
pub async fn summary(training_id: &str) -> Result<TrainingSessionSummary, TrainingApiError> {
    http::request_json("GET", &format!("/api/trainings/{training_id}/summary"))
        .await
        .map_err(TrainingApiError::from)
}

/// サーバーに保存済みのトレーニングの全体を返す。
pub async fn record(training_id: &str) -> Result<TrainingRecord, TrainingApiError> {
    http::request_json("GET", &format!("/api/trainings/{training_id}"))
        .await
        .map_err(TrainingApiError::from)
}

/// トレーニングの編集権をこの端末へ引き継ぎ、保存済みの記録を返す。
pub async fn takeover(training_id: &str) -> Result<TrainingRecord, TrainingApiError> {
    http::request_json("POST", &format!("/api/trainings/{training_id}/takeover"))
        .await
        .map_err(TrainingApiError::from)
}

#[cfg(test)]
mod tests {
    use super::TrainingApiError;
    use crate::http::HttpError;

    #[test]
    fn the_http_failures_map_to_the_failures_the_screen_shows() {
        assert_eq!(
            TrainingApiError::from(HttpError::Forbidden),
            TrainingApiError::Forbidden
        );
        assert_eq!(
            TrainingApiError::from(HttpError::OriginRejected),
            TrainingApiError::Unavailable
        );
        assert_eq!(
            TrainingApiError::from(HttpError::NotFound),
            TrainingApiError::NotFound
        );
        assert_eq!(
            TrainingApiError::from(HttpError::Conflict),
            TrainingApiError::Conflict
        );
        assert_eq!(
            TrainingApiError::from(HttpError::Rejected),
            TrainingApiError::Rejected
        );
        assert_eq!(
            TrainingApiError::from(HttpError::InvalidResponse),
            TrainingApiError::InvalidResponse
        );
    }
}
