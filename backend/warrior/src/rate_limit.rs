//! 固定の時間窓のレート制限。

use serde::Deserialize;
use warrior_core::rate_limit::{retry_after, window_start};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{Env, Request, Response, Result};

use crate::auth::{now_seconds, var};
use crate::db::Db;
use crate::error::{response, ErrorCode};

/// レート制限の対象。
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// 認証の開始とコールバック。IP ごと。
    AuthIp,
    /// パスキーの登録とログイン。IP ごと。
    PasskeyIp,
    /// AI の生成。利用者ごと。
    AiUser,
    /// プッシュ購読。利用者ごと。
    PushUser,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Kind::AuthIp => "auth",
            Kind::PasskeyIp => "passkey",
            Kind::AiUser => "ai",
            Kind::PushUser => "push",
        }
    }

    fn limit_var(self) -> &'static str {
        match self {
            Kind::AuthIp => "RATE_LIMIT_AUTH_IP",
            Kind::PasskeyIp => "RATE_LIMIT_PASSKEY_IP",
            Kind::AiUser => "RATE_LIMIT_AI_USER",
            Kind::PushUser => "RATE_LIMIT_PUSH_USER",
        }
    }

    fn window_var(self) -> &'static str {
        match self {
            Kind::AuthIp => "RATE_LIMIT_AUTH_WINDOW",
            Kind::PasskeyIp => "RATE_LIMIT_PASSKEY_WINDOW",
            Kind::AiUser => "RATE_LIMIT_AI_WINDOW",
            Kind::PushUser => "RATE_LIMIT_PUSH_WINDOW",
        }
    }

    fn scope(self) -> Scope {
        match self {
            Kind::AuthIp | Kind::PasskeyIp => Scope::Ip,
            Kind::AiUser | Kind::PushUser => Scope::User,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Ip,
    User,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

/// IP ごとのレート制限を検査する。上限内なら None、拒否なら応答を返す。
pub async fn check_ip(request: &Request, env: &Env, kind: Kind) -> Result<Option<Response>> {
    if kind.scope() != Scope::Ip {
        return Err(worker::Error::RustError(
            "the rate limit kind does not use an IP".to_owned(),
        ));
    }
    let Some(ip) = request
        .headers()
        .get("CF-Connecting-IP")?
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(Some(response(
            ErrorCode::BadRequest,
            "CF-Connecting-IP is required",
        )));
    };
    let db = Db::new(env.d1("DB")?);
    check_counter(&db, &ip, kind, env).await
}

/// 利用者ごとのレート制限を検査する。上限内なら None、拒否なら応答を返す。
pub async fn check_user(env: &Env, user_id: &str, kind: Kind) -> Result<Option<Response>> {
    if kind.scope() != Scope::User {
        return Err(worker::Error::RustError(
            "the rate limit kind does not use a user".to_owned(),
        ));
    }
    let db = Db::new(env.d1("DB")?);
    check_counter(&db, user_id, kind, env).await
}

async fn check_counter(db: &Db, identity: &str, kind: Kind, env: &Env) -> Result<Option<Response>> {
    let limit = parse_var(env, kind.limit_var())?;
    let window = parse_var(env, kind.window_var())?;
    let now = now_seconds();
    let start = window_start(now, window);
    let expires = warrior_core::datetime::format_utc(start + window);
    let table = match kind.scope() {
        Scope::Ip => "ip_rate_limits",
        Scope::User => "user_rate_limits",
    };
    let identity_column = match kind.scope() {
        Scope::Ip => "ip",
        Scope::User => "user_id",
    };
    let insert = SqlStatement::new(
        format!(
            "INSERT INTO {table} ({identity_column}, kind, window_start, count, expires_at) \
             VALUES (?, ?, ?, 1, ?) \
             ON CONFLICT ({identity_column}, kind, window_start) \
             DO UPDATE SET count = count + 1"
        ),
        vec![
            SqlValue::Text(identity.to_owned()),
            SqlValue::Text(kind.key().to_owned()),
            SqlValue::Integer(start),
            SqlValue::Text(expires),
        ],
    );
    let select = SqlStatement::new(
        format!(
            "SELECT count FROM {table} \
             WHERE {identity_column} = ? AND kind = ? AND window_start = ?"
        ),
        vec![
            SqlValue::Text(identity.to_owned()),
            SqlValue::Text(kind.key().to_owned()),
            SqlValue::Integer(start),
        ],
    );
    let results = db.batch(&[insert, select]).await?;
    let count = results
        .get(1)
        .and_then(|result| result.results::<CountRow>().ok())
        .and_then(|rows| rows.into_iter().next())
        .map(|row| row.count)
        .unwrap_or(0);
    if count > limit {
        let retry = retry_after(now, window);
        let mut denied = response(ErrorCode::RateLimited, "rate limit exceeded");
        denied
            .headers_mut()
            .set("Retry-After", &retry.max(1).to_string())?;
        return Ok(Some(denied));
    }
    Ok(None)
}

fn parse_var(env: &Env, name: &str) -> Result<i64> {
    let text = var(env, name)?;
    text.parse::<i64>().map_err(|_| {
        worker::Error::RustError(format!("the {name} var must be an integer but was {text}"))
    })
}
