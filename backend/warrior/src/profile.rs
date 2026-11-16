//! 身体情報の取得と更新の HTTP ハンドラ。
//!
//! 最新値だけを 1 行に保存する。未登録の利用者は 200 で `null` を返す。
//! 入力の検証は `warrior_core::profile` が持ち、このモジュールは保存と応答だけを行う。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{ActivityLevel, Goal, Sex};
use warrior_core::profile::{Profile, TrainingExperience};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};

const PROFILE_COLUMNS: &str = "height_cm, weight_kg, birth_date, sex, training_experience, \
     goal, training_days_per_week, activity_level";

#[derive(Debug, Deserialize)]
struct ProfileRow {
    height_cm: Option<f64>,
    weight_kg: Option<f64>,
    birth_date: Option<JstDate>,
    sex: Option<Sex>,
    training_experience: Option<TrainingExperience>,
    goal: Option<Goal>,
    training_days_per_week: Option<u32>,
    activity_level: Option<ActivityLevel>,
}

impl ProfileRow {
    fn into_profile(self) -> Profile {
        Profile {
            height_cm: self.height_cm,
            weight_kg: self.weight_kg,
            birth_date: self.birth_date,
            sex: self.sex,
            training_experience: self.training_experience,
            goal: self.goal,
            training_days_per_week: self.training_days_per_week,
            activity_level: self.activity_level,
        }
    }
}

/// `GET /api/profile`
pub async fn get(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    match load(&db, &authenticated.id).await? {
        Some(profile) => Response::from_json(&profile),
        None => Response::from_json(&serde_json::Value::Null),
    }
}

/// `PUT /api/profile`
pub async fn update(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: Profile = match request
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<Profile>(&body).ok())
    {
        Some(input) => input,
        None => return Ok(response(ErrorCode::BadRequest, "the profile is invalid")),
    };
    let Some(today) = today() else {
        console_error!("the current Japan date is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    if input.validate(today).is_err() {
        return Ok(response(ErrorCode::BadRequest, "the profile is invalid"));
    }
    let db = Db::new(env.d1("DB")?);
    save(&db, &authenticated.id, &input).await?;
    Response::from_json(&input)
}

pub(crate) async fn load(db: &Db, user_id: &str) -> Result<Option<Profile>> {
    let row = db
        .first::<ProfileRow>(&SqlStatement::new(
            format!("SELECT {PROFILE_COLUMNS} FROM profiles WHERE user_id = ?"),
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?;
    Ok(row.map(ProfileRow::into_profile))
}

async fn save(db: &Db, user_id: &str, profile: &Profile) -> Result<()> {
    let now = auth::now_iso();
    db.run(&upsert_statement(user_id, profile, &now)).await?;
    Ok(())
}

fn upsert_statement(user_id: &str, profile: &Profile, now: &str) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
         training_experience, goal, training_days_per_week, activity_level, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET \
         height_cm = excluded.height_cm, weight_kg = excluded.weight_kg, \
         birth_date = excluded.birth_date, sex = excluded.sex, \
         training_experience = excluded.training_experience, goal = excluded.goal, \
         training_days_per_week = excluded.training_days_per_week, \
         activity_level = excluded.activity_level, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            optional_real(profile.height_cm),
            optional_real(profile.weight_kg),
            optional_text(profile.birth_date.map(|date| date.to_string())),
            optional_text(profile.sex.map(Sex::as_str).map(str::to_owned)),
            optional_text(
                profile
                    .training_experience
                    .map(TrainingExperience::as_str)
                    .map(str::to_owned),
            ),
            optional_text(profile.goal.map(Goal::as_str).map(str::to_owned)),
            optional_integer(profile.training_days_per_week),
            optional_text(
                profile
                    .activity_level
                    .map(ActivityLevel::as_str)
                    .map(str::to_owned),
            ),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn optional_real(value: Option<f64>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Real)
}

fn optional_integer(value: Option<u32>) -> SqlValue {
    value.map_or(SqlValue::Null, |value| SqlValue::Integer(i64::from(value)))
}

fn optional_text(value: Option<String>) -> SqlValue {
    value.map_or(SqlValue::Null, SqlValue::Text)
}

pub(crate) fn today() -> Option<JstDate> {
    JstDate::parse(&datetime::japan_date(auth::now_seconds()))
}

#[cfg(test)]
mod tests {
    use warrior_core::jst::JstDate;
    use warrior_core::nutrition::{ActivityLevel, Goal, Sex};
    use warrior_core::profile::{Profile, TrainingExperience};
    use warrior_core::sql::SqlValue;

    use super::upsert_statement;

    #[test]
    fn the_upsert_writes_every_profile_column_and_replaces_the_latest_value() {
        let profile = Profile {
            height_cm: Some(175.0),
            weight_kg: Some(70.0),
            birth_date: Some(JstDate::new(1990, 5, 10).unwrap()),
            sex: Some(Sex::Male),
            training_experience: Some(TrainingExperience::Intermediate),
            goal: Some(Goal::Hypertrophy),
            training_days_per_week: Some(4),
            activity_level: Some(ActivityLevel::Normal),
        };
        let statement = upsert_statement("user-a", &profile, "2026-11-12T00:00:00Z");
        assert!(statement.sql().contains("ON CONFLICT(user_id) DO UPDATE"));
        assert_eq!(
            statement.bindings(),
            [
                SqlValue::Text("user-a".to_owned()),
                SqlValue::Real(175.0),
                SqlValue::Real(70.0),
                SqlValue::Text("1990-05-10".to_owned()),
                SqlValue::Text("male".to_owned()),
                SqlValue::Text("intermediate".to_owned()),
                SqlValue::Text("hypertrophy".to_owned()),
                SqlValue::Integer(4),
                SqlValue::Text("normal".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
                SqlValue::Text("2026-11-12T00:00:00Z".to_owned()),
            ]
        );
    }

    #[test]
    fn the_upsert_writes_null_for_unset_fields() {
        let statement = upsert_statement("user-a", &Profile::default(), "2026-11-12T00:00:00Z");
        assert_eq!(
            &statement.bindings()[1..9],
            [
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
                SqlValue::Null,
            ]
        );
    }
}
