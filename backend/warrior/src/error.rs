//! API のエラーのコードと JSON の形。
//!
//! エラーは `{"error":{"code":"...","message":"..."}}` に統一する。
//! `code` は画面の文言を引くキーにし、`message` は英語にする。

use serde::Serialize;
use worker::{console_error, Response};

/// API のエラーの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// 入力が不正である。
    BadRequest,
    /// 認証が必要である。
    Unauthorized,
    /// 同一オリジンでない、または権限がない。
    Forbidden,
    /// 経路または資源が見つからない。
    NotFound,
    /// 経路はあるがメソッドが許可されていない。
    MethodNotAllowed,
    /// サーバーの内部の失敗。
    Internal,
}

impl ErrorCode {
    /// 応答の JSON の `code` に載せる値。
    pub fn code(self) -> &'static str {
        match self {
            ErrorCode::BadRequest => "bad_request",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::NotFound => "not_found",
            ErrorCode::MethodNotAllowed => "method_not_allowed",
            ErrorCode::Internal => "internal",
        }
    }

    /// 応答の状態コード。
    pub fn status(self) -> u16 {
        match self {
            ErrorCode::BadRequest => 400,
            ErrorCode::Unauthorized => 401,
            ErrorCode::Forbidden => 403,
            ErrorCode::NotFound => 404,
            ErrorCode::MethodNotAllowed => 405,
            ErrorCode::Internal => 500,
        }
    }
}

/// 状態コードに対応するエラーのコード。成功の状態コードには対応しない。
pub fn code_for_status(status: u16) -> Option<&'static str> {
    match status {
        400 => Some(ErrorCode::BadRequest.code()),
        401 => Some(ErrorCode::Unauthorized.code()),
        403 => Some(ErrorCode::Forbidden.code()),
        404 => Some(ErrorCode::NotFound.code()),
        405 => Some(ErrorCode::MethodNotAllowed.code()),
        500..=599 => Some(ErrorCode::Internal.code()),
        _ => None,
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

/// エラーの JSON を組み立てる。
pub fn envelope(code: ErrorCode, message: &str) -> impl Serialize {
    ErrorEnvelope {
        error: ErrorBody {
            code: code.code(),
            message: message.to_owned(),
        },
    }
}

/// エラーの応答を組み立てる。応答の構築に失敗した場合は 500 の平文に落とす。
pub fn response(code: ErrorCode, message: &str) -> Response {
    match Response::from_json(&envelope(code, message)) {
        Ok(response) => response.with_status(code.status()),
        Err(error) => {
            console_error!("failed to build the error response: {error}");
            Response::error("internal error", ErrorCode::Internal.status())
                .expect("a plain error response is always constructible")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{code_for_status, envelope, ErrorCode};

    #[test]
    fn envelope_has_the_shared_error_shape() {
        let value = serde_json::to_value(envelope(ErrorCode::NotFound, "route not found"))
            .expect("serializable error");
        assert_eq!(
            value,
            serde_json::json!({
                "error": {
                    "code": "not_found",
                    "message": "route not found"
                }
            })
        );
    }

    #[test]
    fn error_codes_and_statuses_are_stable() {
        for (code, expected_code, expected_status) in [
            (ErrorCode::BadRequest, "bad_request", 400),
            (ErrorCode::Unauthorized, "unauthorized", 401),
            (ErrorCode::Forbidden, "forbidden", 403),
            (ErrorCode::NotFound, "not_found", 404),
            (ErrorCode::MethodNotAllowed, "method_not_allowed", 405),
            (ErrorCode::Internal, "internal", 500),
        ] {
            assert_eq!(code.code(), expected_code);
            assert_eq!(code.status(), expected_status);
        }
    }

    #[test]
    fn maps_error_statuses_to_codes() {
        assert_eq!(code_for_status(200), None);
        assert_eq!(code_for_status(400), Some("bad_request"));
        assert_eq!(code_for_status(401), Some("unauthorized"));
        assert_eq!(code_for_status(403), Some("forbidden"));
        assert_eq!(code_for_status(404), Some("not_found"));
        assert_eq!(code_for_status(405), Some("method_not_allowed"));
        assert_eq!(code_for_status(500), Some("internal"));
        assert_eq!(code_for_status(599), Some("internal"));
    }
}
