//! 前回の記録の API。
//!
//! 種目ごとに、同じ種目、同じグリップ、同じアタッチメントの直近の終了済みを取る。
//! グリップとアタッチメントを選ぶ種目は、現在の選択を渡す。

use warrior_core::exercise::GripSelection;
use warrior_core::previous::PreviousResponse;

use crate::http::{self, HttpError};

/// 前回の取得に渡すグリップの JSON とアタッチメントのキーを返す。
///
/// サーバーは `grip` を `GripSelection` の JSON として読み、比較のキーを作る。
/// アタッチメントのキーは、サーバーが作るキーと一致することを確かめるために渡す。
pub fn grip_query(grip: Option<GripSelection>) -> (Option<String>, Option<String>) {
    let Some(grip) = grip else {
        return (None, None);
    };
    let (_, attachment) = grip.comparison_key();
    (serde_json::to_string(&grip).ok(), attachment)
}

/// 種目ごととトレーニング全体の前回を取る。
pub async fn fetch(
    training_id: &str,
    exercise_id: &str,
    grip: Option<&str>,
    attachment: Option<&str>,
) -> Result<PreviousResponse, HttpError> {
    let mut url = format!(
        "/api/trainings/{training_id}/previous?exercise_id={}",
        encode(exercise_id)
    );
    if let Some(grip) = grip {
        url.push_str("&grip=");
        url.push_str(&encode(grip));
    }
    if let Some(attachment) = attachment {
        url.push_str("&attachment=");
        url.push_str(&encode(attachment));
    }
    http::request_json("GET", &url).await
}

/// トレーニング全体の前回だけを取る。種目が 0 件でも全体の差を出せるようにする。
pub async fn fetch_training(training_id: &str) -> Result<PreviousResponse, HttpError> {
    http::request_json("GET", &format!("/api/trainings/{training_id}/previous")).await
}

fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}
