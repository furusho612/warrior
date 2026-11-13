//! `/api/*` の経路の登録。
//!
//! このモジュールは経路の解析だけを担い、応答のヘッダとログは入口が付ける。

use warrior_core::{HEALTH_PATH, HEALTH_STATUS};
use worker::{Request, Response, Result, RouteContext, Router};

use crate::account;
use crate::auth::{oauth, passkey, session};

/// ヘルスチェックの経路の名前。
pub const HEALTH_ROUTE_NAME: &str = "health";
/// Google OAuth を開始する経路の名前。
pub const GOOGLE_START_ROUTE_NAME: &str = "auth_google_start";
/// Google OAuth のコールバックの経路の名前。
pub const GOOGLE_CALLBACK_ROUTE_NAME: &str = "auth_google_callback";
/// ログアウトの経路の名前。
pub const LOGOUT_ROUTE_NAME: &str = "auth_logout";
/// 現在の利用者を返す経路の名前。
pub const ME_ROUTE_NAME: &str = "me";
/// 経路が見つからないときのログの名前。
pub const NOT_FOUND_ROUTE_NAME: &str = "not_found";

/// `/api/*` の Router を組み立てる。
pub fn build_router() -> Router<'static, ()> {
    Router::new()
        .get_async(HEALTH_PATH, health)
        .get_async("/api/auth/google/start", oauth::start)
        .get_async("/api/auth/google/callback", oauth::callback)
        .post_async("/api/auth/logout", session::logout)
        .get_async("/api/me", session::me)
        .get_async("/api/me/passkeys", passkey::list)
        .delete_async("/api/account", account::delete)
        .delete_async("/api/me/passkeys/:id", passkey::delete)
        .get_async(
            "/api/auth/reauthenticate/google/start",
            oauth::start_elevation,
        )
        .post_async(
            "/api/auth/reauthenticate/passkey/start",
            passkey::reauthenticate_start,
        )
        .post_async(
            "/api/auth/reauthenticate/passkey/finish",
            passkey::reauthenticate_finish,
        )
        .post_async("/api/auth/passkey/register/start", passkey::register_start)
        .post_async(
            "/api/auth/passkey/register/finish",
            passkey::register_finish,
        )
        .post_async("/api/auth/passkey/login/start", passkey::login_start)
        .post_async("/api/auth/passkey/login/finish", passkey::login_finish)
}

/// 経路のパスからログに載せる名前を決める。
pub fn route_name(path: &str) -> &'static str {
    if path.starts_with("/api/me/passkeys/") {
        return "passkeys_delete";
    }
    match path {
        HEALTH_PATH => HEALTH_ROUTE_NAME,
        "/api/auth/google/start" => GOOGLE_START_ROUTE_NAME,
        "/api/auth/google/callback" => GOOGLE_CALLBACK_ROUTE_NAME,
        "/api/auth/logout" => LOGOUT_ROUTE_NAME,
        "/api/me" => ME_ROUTE_NAME,
        "/api/me/passkeys" => "passkeys_list",
        "/api/me/passkeys/:id" => "passkeys_delete",
        "/api/account" => "account_delete",
        "/api/auth/reauthenticate/google/start" => "reauth_google_start",
        "/api/auth/reauthenticate/passkey/start" => "reauth_passkey_start",
        "/api/auth/reauthenticate/passkey/finish" => "reauth_passkey_finish",
        "/api/auth/passkey/register/start" => "passkey_register_start",
        "/api/auth/passkey/register/finish" => "passkey_register_finish",
        "/api/auth/passkey/login/start" => "passkey_login_start",
        "/api/auth/passkey/login/finish" => "passkey_login_finish",
        _ => NOT_FOUND_ROUTE_NAME,
    }
}

async fn health(_request: Request, _context: RouteContext<()>) -> Result<Response> {
    Response::from_json(&serde_json::json!({ "status": HEALTH_STATUS }))
}

#[cfg(test)]
mod tests {
    use super::{
        route_name, GOOGLE_CALLBACK_ROUTE_NAME, GOOGLE_START_ROUTE_NAME, HEALTH_ROUTE_NAME,
        LOGOUT_ROUTE_NAME, ME_ROUTE_NAME, NOT_FOUND_ROUTE_NAME,
    };

    #[test]
    fn names_the_health_route() {
        assert_eq!(route_name("/api/health"), HEALTH_ROUTE_NAME);
    }

    #[test]
    fn names_the_auth_routes() {
        assert_eq!(
            route_name("/api/auth/google/start"),
            GOOGLE_START_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/auth/google/callback"),
            GOOGLE_CALLBACK_ROUTE_NAME
        );
        assert_eq!(route_name("/api/auth/logout"), LOGOUT_ROUTE_NAME);
        assert_eq!(route_name("/api/me"), ME_ROUTE_NAME);
    }

    #[test]
    fn names_unknown_routes_as_not_found() {
        assert_eq!(route_name("/api/unknown"), NOT_FOUND_ROUTE_NAME);
        assert_eq!(route_name("/"), NOT_FOUND_ROUTE_NAME);
    }
}
