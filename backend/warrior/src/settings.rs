//! 利用者共通の設定の取得と更新の HTTP ハンドラ。
//!
//! 利用者ごとに 1 行を保存する。行が無い利用者は既定値を返し、更新で行を作る。
//! 値の検証は `warrior_core::settings` が持ち、このモジュールは保存と応答だけを行う。

use serde::Deserialize;
use warrior_core::nutrition::MacroTarget;
use warrior_core::settings::{DisplayMode, Settings};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};

const SETTINGS_COLUMNS: &str = "rest_default_seconds, display_mode, protein_g_per_kg, fat_ratio";

#[derive(Debug, Deserialize)]
struct SettingsRow {
    rest_default_seconds: u32,
    display_mode: DisplayMode,
    protein_g_per_kg: f64,
    fat_ratio: f64,
}

impl SettingsRow {
    fn into_settings(self) -> Settings {
        Settings {
            rest_default_seconds: self.rest_default_seconds,
            display_mode: self.display_mode,
            macro_target: MacroTarget {
                protein_g_per_kg: self.protein_g_per_kg,
                fat_ratio: self.fat_ratio,
            },
        }
    }
}

/// `GET /api/settings`
pub async fn get(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let settings = load(&db, &authenticated.id)
        .await?
        .map_or_else(Settings::default, SettingsRow::into_settings);
    Response::from_json(&settings)
}

/// `PUT /api/settings`
pub async fn update(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: Settings = match request
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<Settings>(&body).ok())
    {
        Some(input) => input,
        None => return Ok(response(ErrorCode::BadRequest, "the settings are invalid")),
    };
    if input.validate().is_err() {
        return Ok(response(ErrorCode::BadRequest, "the settings are invalid"));
    }
    let db = Db::new(env.d1("DB")?);
    save(&db, &authenticated.id, input).await?;
    Response::from_json(&input)
}

async fn load(db: &Db, user_id: &str) -> Result<Option<SettingsRow>> {
    db.first::<SettingsRow>(&SqlStatement::new(
        format!("SELECT {SETTINGS_COLUMNS} FROM user_settings WHERE user_id = ?"),
        vec![SqlValue::Text(user_id.to_owned())],
    ))
    .await
}

async fn save(db: &Db, user_id: &str, settings: Settings) -> Result<()> {
    let now = crate::auth::now_iso();
    db.run(&upsert_statement(user_id, settings, &now)).await?;
    Ok(())
}

fn upsert_statement(user_id: &str, settings: Settings, now: &str) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO user_settings (user_id, rest_default_seconds, display_mode, \
         protein_g_per_kg, fat_ratio, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET \
         rest_default_seconds = excluded.rest_default_seconds, \
         display_mode = excluded.display_mode, \
         protein_g_per_kg = excluded.protein_g_per_kg, \
         fat_ratio = excluded.fat_ratio, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Integer(i64::from(settings.rest_default_seconds)),
            SqlValue::Text(settings.display_mode.as_str().to_owned()),
            SqlValue::Real(settings.macro_target.protein_g_per_kg),
            SqlValue::Real(settings.macro_target.fat_ratio),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use warrior_core::nutrition::MacroTarget;
    use warrior_core::settings::{DisplayMode, Settings};
    use warrior_core::sql::SqlValue;

    use super::upsert_statement;

    #[test]
    fn the_upsert_writes_every_setting_and_replaces_the_latest_value() {
        let settings = Settings {
            rest_default_seconds: 120,
            display_mode: DisplayMode::Dark,
            macro_target: MacroTarget {
                protein_g_per_kg: 1.8,
                fat_ratio: 0.25,
            },
        };
        let statement = upsert_statement("user-a", settings, "2026-11-12T00:00:00Z");
        assert!(statement.sql().contains("ON CONFLICT(user_id) DO UPDATE"));
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Integer(120),
                SqlValue::Text("dark".to_owned()),
                SqlValue::Real(1.8),
                SqlValue::Real(0.25),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
            ]
        );
    }

    #[test]
    fn the_upsert_writes_the_defaults() {
        let statement = upsert_statement("user-a", Settings::default(), "2026-11-12T00:00:00Z");
        assert_eq!(
            &statement.bindings()[1..5],
            [
                SqlValue::Integer(90),
                SqlValue::Text("system".to_owned()),
                SqlValue::Real(2.0),
                SqlValue::Real(0.2),
            ]
        );
    }
}
