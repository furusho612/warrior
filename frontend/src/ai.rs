//! AI の評価と次回セットの提案の取得 (REQ-AI-001、REQ-AI-004、REQ-AI-006、REQ-AI-018)。

use serde::Deserialize;
use warrior_core::ai::output::{Evaluation, MenuChange, SetProposal};

use crate::http::{self, HttpError};

/// 評価の 1 件。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvaluationEntry {
    /// 生成の ID。
    pub id: String,
    /// 生成の種別。
    pub kind: String,
    /// 対象のトレーニングの ID。
    pub training_id: Option<String>,
    /// 期間の開始日。
    pub period_start: Option<String>,
    /// 期間の終了日。
    pub period_end: Option<String>,
    /// 生成の状態。
    pub status: String,
    /// 評価。生成できなかった場合は None。
    pub evaluation: Option<Evaluation>,
}

/// 評価の一覧の応答。
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct EvaluationsResponse {
    evaluations: Vec<EvaluationEntry>,
}

/// 次回セットの提案の 1 件。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProposalEntry {
    /// 生成の ID。
    pub id: String,
    /// 種目の ID。
    pub exercise_id: String,
    /// 生成の状態。
    pub status: String,
    /// 提案。生成できなかった場合は None。
    pub proposal: Option<SetProposal>,
}

/// 次回セットの提案の一覧の応答。
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ProposalsResponse {
    proposals: Vec<ProposalEntry>,
}

/// 評価の一覧を取る。日付を指定すると、その日の日の評価だけを取る。
pub async fn evaluations(date: Option<&str>) -> Result<Vec<EvaluationEntry>, HttpError> {
    let url = match date {
        Some(date) => format!("/api/ai/evaluations?date={date}"),
        None => "/api/ai/evaluations".to_owned(),
    };
    let response: EvaluationsResponse = http::request_json("GET", &url).await?;
    Ok(response.evaluations)
}

/// 次回セットの提案を取る。種目を指定すると、その種目の提案だけを取る。
pub async fn proposals(exercise_id: Option<&str>) -> Result<Vec<ProposalEntry>, HttpError> {
    let url = match exercise_id {
        Some(exercise_id) => format!("/api/ai/proposals?exercise_id={exercise_id}"),
        None => "/api/ai/proposals".to_owned(),
    };
    let response: ProposalsResponse = http::request_json("GET", &url).await?;
    Ok(response.proposals)
}

/// 次回セットの提案の修正を保存する。
pub async fn update_proposal(id: &str, proposal: &SetProposal) -> Result<SetProposal, HttpError> {
    #[derive(serde::Serialize)]
    struct Request<'a> {
        weight_kg: Option<f64>,
        reps: Option<u32>,
        duration_seconds: Option<u32>,
        sets: u32,
        reason: &'a str,
    }
    http::request_json_with_body(
        "PUT",
        &format!("/api/ai/proposals/{id}"),
        &Request {
            weight_kg: proposal.weight_kg,
            reps: proposal.reps,
            duration_seconds: proposal.duration_seconds,
            sets: proposal.sets,
            reason: &proposal.reason,
        },
    )
    .await
}

/// メニューの最適化の応答。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MenuView {
    /// 生成の ID。
    pub id: Option<String>,
    /// 生成の状態。
    pub status: String,
    /// 期間の開始日。
    pub period_start: Option<String>,
    /// 期間の終了日。
    pub period_end: Option<String>,
    /// 変更案。
    pub changes: Vec<MenuChange>,
}

/// メニューの最適化を取る。
pub async fn menu() -> Result<MenuView, HttpError> {
    http::request_json("GET", "/api/ai/menu").await
}

/// 承認した変更だけをルーティンへ反映する。
pub async fn apply_menu(changes: &[MenuChange]) -> Result<usize, HttpError> {
    #[derive(serde::Serialize)]
    struct Request<'a> {
        changes: &'a [MenuChange],
    }
    #[derive(Deserialize)]
    struct Response {
        applied: usize,
    }
    let response: Response =
        http::request_json_with_body("POST", "/api/ai/menu/apply", &Request { changes }).await?;
    Ok(response.applied)
}

/// 作り直しの結果。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RedoResult {
    /// `pending` または `minimal`。
    pub status: String,
}

/// 評価または提案の作り直しを依頼する。
pub async fn redo(job_id: &str) -> Result<RedoResult, HttpError> {
    http::request_json_with_body(
        "POST",
        &format!("/api/ai/jobs/{job_id}/redo"),
        &serde_json::json!({}),
    )
    .await
}
