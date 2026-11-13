//! Cloudflare Workers の HTTP の入口。
//!
//! 経路の登録を追加するまでは、すべての要求に 404 を返す。

use worker::{event, Context, Env, Request, Response, Result};

/// Worker が受け取る HTTP の要求を処理する。
#[event(fetch)]
pub async fn fetch(_req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    Response::error("Not Found", 404)
}
