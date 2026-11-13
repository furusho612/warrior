//! `/api/*` の経路の登録。
//!
//! このモジュールは経路の解析だけを担い、応答のヘッダとログは入口が付ける。

use warrior_core::{HEALTH_PATH, HEALTH_STATUS};
use worker::{Request, Response, Result, RouteContext, Router};

/// ヘルスチェックの経路の名前。
pub const HEALTH_ROUTE_NAME: &str = "health";
/// 経路が見つからないときのログの名前。
pub const NOT_FOUND_ROUTE_NAME: &str = "not_found";

/// `/api/*` の Router を組み立てる。
pub fn build_router() -> Router<'static, ()> {
    Router::new().get_async(HEALTH_PATH, health)
}

/// 経路のパスからログに載せる名前を決める。
pub fn route_name(path: &str) -> &'static str {
    if path == HEALTH_PATH {
        HEALTH_ROUTE_NAME
    } else {
        NOT_FOUND_ROUTE_NAME
    }
}

async fn health(_request: Request, _context: RouteContext<()>) -> Result<Response> {
    Response::from_json(&serde_json::json!({ "status": HEALTH_STATUS }))
}

#[cfg(test)]
mod tests {
    use super::{route_name, HEALTH_ROUTE_NAME, NOT_FOUND_ROUTE_NAME};

    #[test]
    fn names_the_health_route() {
        assert_eq!(route_name("/api/health"), HEALTH_ROUTE_NAME);
    }

    #[test]
    fn names_unknown_routes_as_not_found() {
        assert_eq!(route_name("/api/unknown"), NOT_FOUND_ROUTE_NAME);
        assert_eq!(route_name("/"), NOT_FOUND_ROUTE_NAME);
    }
}
