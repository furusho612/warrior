//! 独自のプログラムの作成、編集、削除、選択 (REQ-PRG-021、REQ-PRG-022、REQ-PRG-023、REQ-PRG-024)。

use serde::{Deserialize, Serialize};

use crate::http::{self, HttpError};

/// 独自のプログラム。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CustomProgram {
    pub id: String,
    pub name: String,
    pub routine_ids: Vec<String>,
}

/// 独自のプログラムの一覧の応答。
#[derive(Debug, Deserialize)]
struct CustomProgramsResponse {
    programs: Vec<CustomProgram>,
}

#[derive(Debug, Serialize)]
struct CustomInput<'a> {
    name: &'a str,
    routine_ids: &'a [String],
}

/// 独自のプログラムの一覧を取る。
pub async fn list() -> Result<Vec<CustomProgram>, HttpError> {
    let response: CustomProgramsResponse =
        http::request_json("GET", "/api/custom-programs").await?;
    Ok(response.programs)
}

/// 独自のプログラムを作る。
pub async fn create(name: &str, routine_ids: &[String]) -> Result<CustomProgram, HttpError> {
    http::request_json_with_body(
        "POST",
        "/api/custom-programs",
        &CustomInput { name, routine_ids },
    )
    .await
}

/// 独自のプログラムを更新する。
pub async fn update(
    id: &str,
    name: &str,
    routine_ids: &[String],
) -> Result<CustomProgram, HttpError> {
    http::request_json_with_body(
        "PUT",
        &format!("/api/custom-programs/{id}"),
        &CustomInput { name, routine_ids },
    )
    .await
}

/// 独自のプログラムを削除する。
pub async fn delete(id: &str) -> Result<(), HttpError> {
    http::send_json_without_response(
        "DELETE",
        &format!("/api/custom-programs/{id}"),
        &serde_json::json!({}),
    )
    .await
}

/// 独自のプログラムを選び、進行を開始する。
pub async fn select(id: &str) -> Result<(), HttpError> {
    http::request_json_with_body::<serde_json::Value, _>(
        "POST",
        &format!("/api/custom-programs/{id}/select"),
        &serde_json::json!({}),
    )
    .await
    .map(|_| ())
}
