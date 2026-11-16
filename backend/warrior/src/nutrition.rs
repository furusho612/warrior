//! 目標カロリーと PFC の取得と調整 (REQ-PRG-010、REQ-PRG-013、REQ-PRG-015、REQ-PRG-016、REQ-MYP-002)。
//!
//! 身体情報と設定から計算し、未入力の項目があれば計算せずにその一覧を返す。
//! 調整値の保存は `user_settings` の PFC の列へ行う。計算は `warrior_core::nutrition` が持つ。

use serde::Serialize;
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{
    calculate, display_macros, DisplayMacros, MacroTarget, MissingField, NutritionError,
    NutritionInput, NutritionResult,
};
use warrior_core::profile::Profile;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::profile;
use crate::settings;

/// 目標カロリーと PFC の応答。
#[derive(Debug, Serialize)]
struct NutritionResponse {
    /// 計算できた結果。未入力の項目がある場合は null。
    result: Option<NutritionResult>,
    /// 画面に表示する整数の PFC。
    display: Option<DisplayMacros>,
    /// 未入力の項目。
    missing_fields: Vec<MissingField>,
    /// 保存されている PFC の調整値。
    macro_target: MacroTarget,
}

/// `GET /api/nutrition`
pub async fn get(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let profile = profile::load(&db, &authenticated.id).await?;
    let settings = settings::load(&db, &authenticated.id)
        .await?
        .map_or_else(Default::default, |row| row.into_settings());
    let Some(today) = profile::today() else {
        console_error!("the current Japan date is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let body = build_response(profile.as_ref(), settings.macro_target, today);
    Response::from_json(&body)
}

/// `PUT /api/nutrition`
pub async fn update(mut request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let input: MacroTarget = match request
        .text()
        .await
        .ok()
        .and_then(|body| serde_json::from_str::<MacroTarget>(&body).ok())
    {
        Some(input) => input,
        None => {
            return Ok(response(
                ErrorCode::BadRequest,
                "the macro target is invalid",
            ))
        }
    };
    if !input.is_valid() {
        return Ok(response(
            ErrorCode::BadRequest,
            "the macro target is out of range",
        ));
    }
    let db = Db::new(env.d1("DB")?);
    let profile = profile::load(&db, &authenticated.id).await?;
    let Some(today) = profile::today() else {
        console_error!("the current Japan date is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let input_for_calculation = nutrition_input(profile.as_ref(), input);
    let calculated = calculate(&input_for_calculation, today);
    if let Err(error) = &calculated {
        match error {
            NutritionError::MissingFields(_) => {}
            NutritionError::MacroCaloriesExceedTarget => {
                return Ok(response(
                    ErrorCode::BadRequest,
                    "the macro target exceeds the calorie target",
                ))
            }
            _ => {
                return Ok(response(
                    ErrorCode::BadRequest,
                    "the profile is not usable for the calculation",
                ))
            }
        }
    }
    let mut settings = settings::load(&db, &authenticated.id)
        .await?
        .map_or_else(Default::default, |row| row.into_settings());
    settings.macro_target = input;
    settings::save(&db, &authenticated.id, settings).await?;
    let body = match calculated {
        Ok(result) => NutritionResponse {
            display: Some(display_macros(&result)),
            result: Some(result),
            missing_fields: Vec::new(),
            macro_target: input,
        },
        Err(NutritionError::MissingFields(missing_fields)) => NutritionResponse {
            result: None,
            display: None,
            missing_fields,
            macro_target: input,
        },
        Err(error) => {
            console_error!("the nutrition calculation failed after validation: {error:?}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    Response::from_json(&body)
}

fn build_response(
    profile: Option<&Profile>,
    macro_target: MacroTarget,
    today: JstDate,
) -> NutritionResponse {
    let input = nutrition_input(profile, macro_target);
    match calculate(&input, today) {
        Ok(result) => NutritionResponse {
            display: Some(display_macros(&result)),
            result: Some(result),
            missing_fields: Vec::new(),
            macro_target,
        },
        Err(NutritionError::MissingFields(missing_fields)) => NutritionResponse {
            result: None,
            display: None,
            missing_fields,
            macro_target,
        },
        Err(error) => {
            console_error!("the stored nutrition input is invalid: {error:?}");
            NutritionResponse {
                result: None,
                display: None,
                missing_fields: input.missing_fields(),
                macro_target,
            }
        }
    }
}

pub(crate) fn nutrition_input(
    profile: Option<&Profile>,
    macro_target: MacroTarget,
) -> NutritionInput {
    match profile {
        Some(profile) => NutritionInput {
            height_cm: profile.height_cm,
            weight_kg: profile.weight_kg,
            birth_date: profile.birth_date,
            sex: profile.sex,
            goal: profile.goal,
            training_days_per_week: profile.training_days_per_week,
            activity_level: profile.activity_level,
            macro_target,
        },
        None => NutritionInput {
            macro_target,
            ..NutritionInput::default()
        },
    }
}

#[cfg(test)]
mod tests {
    //! 未入力の組み合わせと、計算できた場合の表示の例示テスト。
    //!
    //! 対応する要求: `REQ-PRG-013`、`REQ-PRG-018`。

    use warrior_core::jst::JstDate;
    use warrior_core::nutrition::{ActivityLevel, Goal, MacroTarget, MissingField, Sex};
    use warrior_core::profile::Profile;

    use super::build_response;

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }

    fn profile() -> Profile {
        Profile {
            height_cm: Some(175.0),
            weight_kg: Some(70.0),
            birth_date: Some(date(1990, 5, 10)),
            sex: Some(Sex::Male),
            training_experience: None,
            goal: Some(Goal::Diet),
            training_days_per_week: Some(4),
            activity_level: Some(ActivityLevel::Normal),
        }
    }

    #[test]
    fn req_prg_013_missing_fields_are_returned_without_a_result() {
        let mut profile = profile();
        profile.weight_kg = None;
        profile.activity_level = None;
        let response = build_response(Some(&profile), MacroTarget::default(), date(2026, 11, 12));
        assert!(response.result.is_none());
        assert!(response.display.is_none());
        assert_eq!(
            response.missing_fields,
            vec![MissingField::Weight, MissingField::ActivityLevel]
        );
    }

    #[test]
    fn req_prg_018_the_diet_range_is_only_present_for_diet() {
        let mut profile = profile();
        profile.goal = Some(Goal::Diet);
        let diet = build_response(Some(&profile), MacroTarget::default(), date(2026, 11, 12));
        assert!(diet.result.expect("a result").diet_loss_per_week.is_some());
        profile.goal = Some(Goal::Hypertrophy);
        let hypertrophy =
            build_response(Some(&profile), MacroTarget::default(), date(2026, 11, 12));
        assert!(hypertrophy
            .result
            .expect("a result")
            .diet_loss_per_week
            .is_none());
    }

    #[test]
    fn req_prg_010_the_result_changes_with_the_activity_level() {
        let mut profile = profile();
        let low = build_response(Some(&profile), MacroTarget::default(), date(2026, 11, 12));
        profile.activity_level = Some(ActivityLevel::High);
        let high = build_response(Some(&profile), MacroTarget::default(), date(2026, 11, 12));
        assert!(
            high.result.expect("a result").target_kcal > low.result.expect("a result").target_kcal
        );
    }
}
