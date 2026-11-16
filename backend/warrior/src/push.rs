//! プッシュ通知の購読の HTTP ハンドラ (REQ-TMR-008、ADR-008)。
//!
//! 公開鍵は環境の vars から返し、秘密鍵は扱わない。購読は認証中の利用者と、
//! 要求を送った端末トークンに紐づける。同じ利用者の同じ endpoint の購読は 1 行とし、
//! 送り直されたら鍵と端末を更新する。endpoint の検証は `warrior_core::push` が持つ。

use serde::Deserialize;
use warrior_core::push::{Subscription, SubscriptionInput};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::ownership;

#[derive(Debug, Deserialize)]
struct EndpointInput {
    endpoint: String,
}

/// `GET /api/push/public-key`
pub async fn public_key(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    if session::resolve(&request, &env).await?.is_none() {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    }
    let public_key = auth::var(&env, "VAPID_PUBLIC_KEY")?;
    Response::from_json(&serde_json::json!({ "public_key": public_key }))
}

/// `POST /api/push/subscriptions`
pub async fn create(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    if let Some(denied) =
        crate::rate_limit::check_user(&env, &authenticated.id, crate::rate_limit::Kind::PushUser)
            .await?
    {
        return Ok(denied);
    }
    let input: SubscriptionInput = match request.json().await {
        Ok(input) => input,
        Err(_) => {
            return Ok(response(
                ErrorCode::BadRequest,
                "the subscription is invalid",
            ))
        }
    };
    let subscription = match Subscription::new(input) {
        Ok(subscription) => subscription,
        Err(error) => return Ok(response(ErrorCode::BadRequest, error.message())),
    };
    let db = Db::new(env.d1("DB")?);
    let Some(terminal_id) = ownership::ensure(&db, &request, &authenticated.id).await? else {
        return Ok(response(ErrorCode::BadRequest, "the terminal is unknown"));
    };
    let now = auth::now_iso();
    db.run(&upsert_statement(
        &authenticated.id,
        &subscription,
        &terminal_id,
        &now,
    ))
    .await?;
    Ok(Response::empty()?.with_status(204))
}

/// `DELETE /api/push/subscriptions`
pub async fn delete(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    if let Some(denied) =
        crate::rate_limit::check_user(&env, &authenticated.id, crate::rate_limit::Kind::PushUser)
            .await?
    {
        return Ok(denied);
    }
    let input: EndpointInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the endpoint is required")),
    };
    if input.endpoint.is_empty() {
        return Ok(response(ErrorCode::BadRequest, "the endpoint is required"));
    }
    let db = Db::new(env.d1("DB")?);
    db.run(&delete_by_endpoint_statement(
        &authenticated.id,
        &input.endpoint,
    ))
    .await?;
    Ok(Response::empty()?.with_status(204))
}

/// 利用者の端末の購読をすべて削除する文。ログアウトで使う。
pub(crate) fn delete_for_terminal_statement(user_id: &str, terminal_id: &str) -> SqlStatement {
    SqlStatement::new(
        "DELETE FROM push_subscriptions WHERE user_id = ? AND terminal_token_id = ?",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(terminal_id.to_owned()),
        ],
    )
}

fn upsert_statement(
    user_id: &str,
    subscription: &Subscription,
    terminal_id: &str,
    now: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO push_subscriptions \
         (user_id, endpoint, p256dh, auth, terminal_token_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, endpoint) DO UPDATE SET \
         p256dh = excluded.p256dh, auth = excluded.auth, \
         terminal_token_id = excluded.terminal_token_id, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(subscription.endpoint().to_owned()),
            SqlValue::Text(subscription.p256dh().to_owned()),
            SqlValue::Text(subscription.auth().to_owned()),
            SqlValue::Text(terminal_id.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn delete_by_endpoint_statement(user_id: &str, endpoint: &str) -> SqlStatement {
    SqlStatement::new(
        "DELETE FROM push_subscriptions WHERE user_id = ? AND endpoint = ?",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(endpoint.to_owned()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use warrior_core::push::{Subscription, SubscriptionInput};
    use warrior_core::sql::SqlValue;

    use super::{delete_by_endpoint_statement, delete_for_terminal_statement, upsert_statement};

    fn subscription(endpoint: &str) -> Subscription {
        Subscription::new(SubscriptionInput {
            endpoint: endpoint.to_owned(),
            p256dh: "p256dh".to_owned(),
            auth: "auth".to_owned(),
        })
        .expect("an allowed subscription")
    }

    #[test]
    fn the_upsert_keeps_one_row_per_user_and_endpoint_and_updates_the_keys() {
        let statement = upsert_statement(
            "user-a",
            &subscription("https://fcm.googleapis.com/fcm/send/abc"),
            "terminal-a",
            "2026-11-12T00:00:00Z",
        );
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Text("https://fcm.googleapis.com/fcm/send/abc".to_owned()),
                SqlValue::Text("p256dh".to_owned()),
                SqlValue::Text("auth".to_owned()),
                SqlValue::Text("terminal-a".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
            ]
        );
        assert!(statement.sql().contains("ON CONFLICT(user_id, endpoint)"));
        assert!(statement.sql().contains("p256dh = excluded.p256dh"));
        assert!(statement.sql().contains("auth = excluded.auth"));
        assert!(
            statement
                .sql()
                .contains("terminal_token_id = excluded.terminal_token_id"),
            "the latest terminal must own the subscription"
        );
        assert!(
            !statement.sql().contains("created_at = "),
            "the first registration time must stay"
        );
    }

    #[test]
    fn the_deletions_are_scoped_to_the_user() {
        let terminal = delete_for_terminal_statement("user-a", "terminal-a");
        assert_eq!(
            terminal.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Text("terminal-a".to_owned()),
            ]
        );
        assert!(terminal
            .sql()
            .contains("user_id = ? AND terminal_token_id = ?"));

        let endpoint =
            delete_by_endpoint_statement("user-a", "https://web.push.apple.com/send/abc");
        assert_eq!(
            endpoint.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Text("https://web.push.apple.com/send/abc".to_owned()),
            ]
        );
        assert!(endpoint.sql().contains("user_id = ? AND endpoint = ?"));
    }
}
