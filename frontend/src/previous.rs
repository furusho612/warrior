//! 前回の記録の API。
//!
//! 種目ごとに、同じ種目、同じグリップ、同じアタッチメントの直近の終了済みを取る。
//! グリップとアタッチメントを選ぶ種目は、記録時に選んだ値を渡す。

use warrior_core::previous::PreviousResponse;

use crate::http::{self, HttpError};

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

fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value)
        .as_string()
        .unwrap_or_default()
}
