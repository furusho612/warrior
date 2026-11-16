//! 目標カロリーと PFC の取得と調整 (REQ-PRG-010、REQ-PRG-013、REQ-PRG-016)。

use serde::Deserialize;
use warrior_core::nutrition::{DisplayMacros, MacroTarget, MissingField, NutritionResult};

use crate::http::{self, HttpError};

/// 目標カロリーと PFC の応答。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NutritionView {
    /// 計算できた結果。未入力の項目がある場合は None。
    pub result: Option<NutritionResult>,
    /// 画面に表示する整数の PFC。
    pub display: Option<DisplayMacros>,
    /// 未入力の項目。
    pub missing_fields: Vec<MissingField>,
    /// 保存されている PFC の調整値。
    pub macro_target: MacroTarget,
}

/// 目標カロリーと PFC を取得する。
pub async fn fetch() -> Result<NutritionView, HttpError> {
    http::request_json("GET", "/api/nutrition").await
}

/// PFC の調整値を保存し、計算し直した結果を受け取る。
pub async fn save(target: MacroTarget) -> Result<NutritionView, HttpError> {
    http::request_json_with_body("PUT", "/api/nutrition", &target).await
}
