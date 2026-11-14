//! 種目ライブラリの一覧の API。
//!
//! 記録への種目の選択が使う。絞り込みと続きのページも、この API に項目を足して表す。

use warrior_core::exercise::ExerciseList;

use crate::http::{self, HttpError};

/// 種目マスターの一覧を取る。
pub async fn exercises() -> Result<ExerciseList, HttpError> {
    http::request_json("GET", "/api/exercises").await
}
