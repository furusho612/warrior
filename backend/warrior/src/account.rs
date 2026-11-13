//! アカウントと利用者に属するデータの削除。
//!
//! R2 の 2 つの接頭辞を先に削除し、成功したら D1 の行を 1 つの batch で削除する。
//! 2 回目の要求はセッションが消えているため 401 になる。削除の対象の一覧は
//! `warrior_core::account_deletion` が持つ。

use serde::Deserialize;
use warrior_core::account_deletion::{r2_prefixes, USER_OWNED_TABLES};
use warrior_core::auth::{clear_elevation_cookie, clear_session_cookie};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{Env, Request, Response, Result, RouteContext};

use crate::auth::append_cookie;
use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};

#[derive(Debug, Deserialize)]
struct Confirm {
    confirm: String,
}

/// `DELETE /api/account`
pub async fn delete(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(_session) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: Confirm = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "confirmation is required")),
    };
    if input.confirm != "delete-account" {
        return Ok(response(ErrorCode::BadRequest, "confirmation is invalid"));
    }

    // R2 を先に削除する。失敗した場合は D1 を変更せず、同じセッションで再試行できる。
    delete_r2_objects(&env, &_session.id).await?;

    let db = Db::new(env.d1("DB")?);
    let statements = USER_OWNED_TABLES
        .iter()
        .map(|table| {
            let sql = if *table == "users" {
                "DELETE FROM users WHERE id = ?".to_owned()
            } else {
                format!("DELETE FROM {table} WHERE user_id = ?")
            };
            SqlStatement::new(&sql, vec![SqlValue::Text(_session.id.clone())])
        })
        .collect::<Vec<_>>();
    db.batch(&statements).await?;

    let mut response = Response::empty()?.with_status(204);
    response = append_cookie(response, &clear_session_cookie())?;
    append_cookie(response, &clear_elevation_cookie())
}

async fn delete_r2_objects(env: &Env, user_id: &str) -> Result<()> {
    let bucket = env.bucket("BUCKET")?;
    for prefix in r2_prefixes(user_id) {
        loop {
            let listed = bucket
                .list()
                .prefix(prefix.clone())
                .limit(1000)
                .execute()
                .await?;
            let keys = listed
                .objects()
                .into_iter()
                .map(|object| object.key())
                .collect::<Vec<_>>();
            if keys.is_empty() {
                break;
            }
            bucket.delete_multiple(keys).await?;
            if !listed.truncated() {
                break;
            }
        }
    }
    Ok(())
}
