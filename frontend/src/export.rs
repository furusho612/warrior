//! 記録の CSV の書き出しの要求。

use serde::Deserialize;

use crate::http;

/// 書き出しのメタデータの応答。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExportJob {
    /// 書き出しの ID。
    pub id: String,
    /// 書き出しの作成時刻 (ISO 8601 の UTC)。
    pub created_at: String,
}

/// 全期間の記録の書き出しを依頼する。
pub async fn start() -> Result<ExportJob, http::HttpError> {
    http::request_json("POST", "/api/exports").await
}
