//! 種目ごとの目標重量の API と、画面に出す値の組み立て。
//!
//! 目標の意味と現在の記録の計算は `warrior_core::goal` が持つ。画面はこの module の
//! 関数だけを使い、URL と本文の形を 1 か所に集める。

use serde::{Deserialize, Serialize};
use warrior_core::goal::GoalProgress;
use warrior_core::history::GripFilter;
use warrior_core::jst::JstDate;

use crate::http::{self, HttpError};

/// 目標の保存の本文。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalInput {
    /// グリップの比較のキー。選ばない種目は None。
    pub grip: Option<String>,
    /// アタッチメントの比較のキー。
    pub attachment: Option<String>,
    /// 目標重量 (kg)。
    pub target_weight_kg: f64,
    /// 達成したい時期。任意である。
    pub target_date: Option<JstDate>,
}

#[derive(Debug, Deserialize)]
struct GoalDeleted {
    deleted: bool,
}

/// 目標を保存し、現在の記録との差を受け取る。
pub async fn save(exercise_id: &str, input: &GoalInput) -> Result<GoalProgress, HttpError> {
    http::request_json_with_body("PUT", &goal_url(exercise_id), input).await
}

/// 目標を取り消す。
pub async fn delete(exercise_id: &str, filter: &GripFilter) -> Result<bool, HttpError> {
    let result: GoalDeleted =
        http::request_json("DELETE", &delete_url(exercise_id, filter)).await?;
    Ok(result.deleted)
}

/// 目標の URL。
pub fn goal_url(exercise_id: &str) -> String {
    format!("/api/exercises/{exercise_id}/goal")
}

/// 目標の削除の URL。組をクエリに載せる。
pub fn delete_url(exercise_id: &str, filter: &GripFilter) -> String {
    let mut parts = Vec::new();
    if let Some(grip) = &filter.grip {
        parts.push(format!("grip={grip}"));
    }
    if let Some(attachment) = &filter.attachment {
        parts.push(format!("attachment={attachment}"));
    }
    if parts.is_empty() {
        goal_url(exercise_id)
    } else {
        format!("{}?{}", goal_url(exercise_id), parts.join("&"))
    }
}

/// 目標との差の文言。
pub fn difference_label(difference_kg: f64) -> String {
    if difference_kg >= 0.0 {
        format!("目標まで {difference_kg:.1} kg")
    } else {
        format!("目標を {:.1} kg 上回っています", difference_kg.abs())
    }
}

/// 重量の文言。
pub fn weight_label(weight_kg: f64) -> String {
    format!("{weight_kg:.1} kg")
}

#[cfg(test)]
mod tests {
    //! 目標の URL と文言の単体テスト。
    //!
    //! 対応する要求: `REQ-GOAL-001`、`REQ-GOAL-002`。
    //! 要求の受入基準は E2E が担い、ここは URL の組み立てと差の文言を確かめる。

    use warrior_core::history::GripFilter;

    use super::{delete_url, difference_label, goal_url, weight_label};
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn the_goal_urls_carry_the_exercise_and_the_comparison_key() {
        assert_eq!(goal_url("bench"), "/api/exercises/bench/goal");
        assert_eq!(
            delete_url("bench", &GripFilter::default()),
            "/api/exercises/bench/goal"
        );
        assert_eq!(
            delete_url(
                "bench",
                &GripFilter::new(Some("pronated_medium".to_owned()), None),
            ),
            "/api/exercises/bench/goal?grip=pronated_medium"
        );
        assert_eq!(
            delete_url(
                "bench",
                &GripFilter::new(Some("supinated".to_owned()), Some("wide_bar".to_owned()),),
            ),
            "/api/exercises/bench/goal?grip=supinated&attachment=wide_bar"
        );
    }

    #[wasm_bindgen_test]
    fn the_difference_and_the_weight_use_one_decimal_place() {
        assert_eq!(difference_label(10.0), "目標まで 10.0 kg");
        assert_eq!(difference_label(-2.5), "目標を 2.5 kg 上回っています");
        assert_eq!(weight_label(100.0), "100.0 kg");
    }
}
