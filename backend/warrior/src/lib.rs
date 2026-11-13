//! Cloudflare Workers の HTTP の入口。
//!
//! `/api/*` の経路を Router に渡し、応答にセキュリティヘッダとリクエスト ID を付け、
//! リクエスト 1 件ごとに JSON のログを出す。

use worker::{console_error, event, Context, Date, Env, Request, Response, Result};

pub mod account;
pub mod auth;
pub mod db;
pub mod error;
pub mod exercise;
pub mod log;
pub mod random;
pub mod rate_limit;
pub mod router;
pub mod scheduled;
pub mod security;
pub mod sync;

use error::ErrorCode;

/// Worker が受け取る HTTP の要求を処理する。
#[event(fetch)]
pub async fn fetch(request: Request, env: Env, _context: Context) -> Result<Response> {
    let started_at_ms = Date::now().as_millis();
    let request_id = log::next_request_id();
    let method = request.method();
    let path = request.path();
    let route = router::route_name(&path);

    let (mut status, mut response) = if security::changes_state(&method) {
        match request_origin_allowed(&request, &env) {
            Ok(true) => run_router(request, env).await,
            Ok(false) => (
                ErrorCode::Forbidden.status(),
                error::response(
                    ErrorCode::Forbidden,
                    "the request origin is not the app origin",
                ),
            ),
            Err(error) => {
                console_error!("failed to check the request origin: {error}");
                (
                    ErrorCode::Internal.status(),
                    error::response(ErrorCode::Internal, "internal error"),
                )
            }
        }
    } else {
        run_router(request, env).await
    };

    if let Err(error) = security::apply_security_headers(&mut response) {
        console_error!("failed to apply security headers: {error}");
        response = error::response(ErrorCode::Internal, "internal error");
        status = response.status_code();
    }
    if let Err(error) = response.headers_mut().set("X-Request-Id", &request_id) {
        console_error!("failed to set the request id header: {error}");
    }

    let duration_ms = Date::now().as_millis().saturating_sub(started_at_ms);
    log::log_request(&log::RequestLog::new(
        route,
        method.as_ref(),
        status,
        duration_ms,
        error::code_for_status(status),
        &request_id,
    ));
    Ok(response)
}

/// 状態を変更するリクエストの `Origin` が、環境の Origin と一致するかを返す。
fn request_origin_allowed(request: &Request, env: &Env) -> Result<bool> {
    let expected_origin = env.var("ORIGIN")?.to_string();
    security::is_same_origin(request, &expected_origin)
}

/// Router を実行し、Router 自身の 404 と 405 を API のエラー JSON に置き換える。
async fn run_router(request: Request, env: Env) -> (u16, Response) {
    match router::build_router().run(request, env).await {
        Ok(response) => {
            let mut response = response;
            if response.status_code() == 404 {
                response = error::response(ErrorCode::NotFound, "route not found");
            } else if response.status_code() == 405 {
                response = error::response(ErrorCode::MethodNotAllowed, "method not allowed");
            }
            (response.status_code(), response)
        }
        Err(error) => {
            console_error!("router failed: {error}");
            (
                ErrorCode::Internal.status(),
                error::response(ErrorCode::Internal, "internal error"),
            )
        }
    }
}
