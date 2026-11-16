//! プログラムの一覧、推奨、取り込み、進行の API (REQ-PRG-002、REQ-PRG-005、REQ-PRG-019)。

use serde::Deserialize;
use warrior_core::exercise::GripSelection;

use crate::http::{self, HttpError};

/// プログラムの概要。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProgramSummary {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub level: String,
    pub days_per_week: u32,
}

/// プログラムの日の種目。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProgramDayExercise {
    pub exercise_id: String,
    pub name: String,
    pub record_mode: String,
    pub laterality: String,
    pub grip_mode: String,
    pub grip_options: Vec<GripSelection>,
    pub set_count: u32,
    pub reps: Option<u32>,
    pub duration_seconds: Option<u32>,
    pub rest_seconds: u32,
}

/// プログラムの日。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProgramDay {
    pub id: String,
    pub name: String,
    pub position: i64,
    pub exercises: Vec<ProgramDayExercise>,
}

/// 進行中のプログラム。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CurrentProgram {
    pub program: ProgramSummary,
    pub routine_id: String,
    pub day: ProgramDay,
    pub day_position: i64,
    pub total_days: i64,
}

/// プログラムの一覧の応答。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProgramsResponse {
    pub recommended: Vec<ProgramSummary>,
    pub references: Vec<ProgramSummary>,
    pub current: Option<CurrentProgram>,
}

/// プログラムの一覧を取る。
pub async fn list() -> Result<ProgramsResponse, HttpError> {
    http::request_json("GET", "/api/programs").await
}

/// プログラムを選び、日ごとのルーティンを取り込む。
pub async fn select(program_id: &str) -> Result<CurrentProgram, HttpError> {
    http::request_json_with_body(
        "POST",
        &format!("/api/programs/{program_id}/select"),
        &serde_json::json!({}),
    )
    .await
}

/// 進行中のプログラムをやめる。
pub async fn stop() -> Result<(), HttpError> {
    http::send_json_without_response("DELETE", "/api/programs/current", &serde_json::json!({}))
        .await
}
