//! Cloudflare Workers の HTTP の入口。
//!
//! ヘルスチェックだけを返す。経路の登録とエラーの形は後続の変更で置き換える。

use worker::{event, Context, Env, Request, Response, Result};

/// Worker が受け取る HTTP の要求を処理する。
#[event(fetch)]
pub async fn fetch(req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    if req.path() == warrior_core::HEALTH_PATH {
        return Response::from_json(&serde_json::json!({ "status": warrior_core::HEALTH_STATUS }));
    }

    Response::error("Not Found", 404)
}
