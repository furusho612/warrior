//! レスト終了のプッシュ通知の予約の API (REQ-TMR-009、REQ-TMR-010)。
//!
//! 予約の作成、延長と短縮、取り消しをサーバーへ送る。通信できない場合は予約を
//! 作らず、レストタイマーは音と画面表示だけで動く (REQ-TMR-011)。

use serde::{Deserialize, Serialize};

use crate::http::{self, HttpError};

/// サーバーが保存した予約。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestReservation {
    /// 予約の ID。
    pub id: String,
    /// 予約の版。
    pub version: i64,
    /// 終了予定時刻 (Unix 時刻の秒)。
    pub end_at: i64,
}

#[derive(Debug, Serialize)]
struct CreateRequest<'a> {
    training_id: &'a str,
    remaining_seconds: u32,
}

#[derive(Debug, Serialize)]
struct UpdateRequest {
    remaining_seconds: u32,
}

/// 予約を作る。
pub async fn create(
    training_id: &str,
    remaining_seconds: u32,
) -> Result<RestReservation, HttpError> {
    http::request_json_with_body(
        "POST",
        "/api/rest/reservations",
        &CreateRequest {
            training_id,
            remaining_seconds,
        },
    )
    .await
}

/// 予約の終了予定時刻を変更し、版を進める。
pub async fn update(
    reservation_id: &str,
    remaining_seconds: u32,
) -> Result<RestReservation, HttpError> {
    http::request_json_with_body(
        "PATCH",
        &format!("/api/rest/reservations/{reservation_id}"),
        &UpdateRequest { remaining_seconds },
    )
    .await
}

/// 予約を取り消す。
pub async fn cancel(reservation_id: &str) -> Result<(), HttpError> {
    http::send_json_without_response(
        "DELETE",
        &format!("/api/rest/reservations/{reservation_id}"),
        &serde_json::json!({}),
    )
    .await
}

/// 通知を受け取った時刻をサーバーへ送る。Service Worker からも同じ経路を使う。
pub async fn received(reservation_id: &str, version: i64) -> Result<(), HttpError> {
    http::send_json_without_response(
        "POST",
        "/api/push/received",
        &serde_json::json!({
            "reservation_id": reservation_id,
            "version": version,
        }),
    )
    .await
}
