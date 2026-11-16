//! Cron Triggers の入口と、期限切れの短期データの削除。

use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{event, Env, Result, ScheduleContext, ScheduledEvent};

use crate::ai_job;
use crate::auth::{now_iso, now_seconds};
use crate::db::Db;

/// 1 分ごとの Cron で期限切れの行を削除し、AI の生成の行を Queue へ送る。
#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _context: ScheduleContext) {
    if let Err(error) = remove_expired(&env).await {
        worker::console_error!("the scheduled cleanup failed: {error}");
    }
    let now = now_seconds();
    if let Err(error) = ai_job::enqueue_pending(&env, now).await {
        worker::console_error!("the scheduled AI job enqueue failed: {error}");
    }
    if let Err(error) = crate::rest_push::enqueue_due(&env, now).await {
        worker::console_error!("the scheduled rest push enqueue failed: {error}");
    }
}

async fn remove_expired(env: &Env) -> Result<()> {
    let db = Db::new(env.d1("DB")?);
    let now = now_iso();
    let statements = [
        "DELETE FROM auth_sessions WHERE expires_at <= ?",
        "DELETE FROM webauthn_challenges WHERE expires_at <= ?",
        "DELETE FROM oauth_states WHERE expires_at <= ?",
        "DELETE FROM user_rate_limits WHERE expires_at <= ?",
        "DELETE FROM ip_rate_limits WHERE expires_at <= ?",
    ]
    .into_iter()
    .map(|sql| SqlStatement::new(sql, vec![SqlValue::Text(now.clone())]))
    .collect::<Vec<_>>();
    db.batch(&statements).await?;
    Ok(())
}
