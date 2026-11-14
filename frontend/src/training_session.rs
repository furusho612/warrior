//! トレーニングの終了と、終了したサマリーの取得の API。

use warrior_core::training_session::TrainingSessionSummary;

use crate::http::{self, HttpError};

/// トレーニングを終了し、サーバーが保存したサマリーを返す。
pub async fn finish(training_id: &str) -> Result<TrainingSessionSummary, HttpError> {
    http::request_json("POST", &format!("/api/trainings/{training_id}/finish")).await
}

/// 終了したトレーニングのサマリーを返す。
pub async fn summary(training_id: &str) -> Result<TrainingSessionSummary, HttpError> {
    http::request_json("GET", &format!("/api/trainings/{training_id}/summary")).await
}
