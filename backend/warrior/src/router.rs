//! `/api/*` の経路の登録。
//!
//! このモジュールは経路の解析だけを担い、応答のヘッダとログは入口が付ける。

use warrior_core::{HEALTH_PATH, HEALTH_STATUS};
use worker::{Request, Response, Result, RouteContext, Router};

use crate::account;
use crate::ai_job;
use crate::alternative;
use crate::analysis;
use crate::auth::{oauth, passkey, session};
use crate::calendar;
use crate::custom_exercise;
use crate::exercise_override;
use crate::export;
use crate::goal;
use crate::history;
use crate::library;
use crate::profile;
use crate::push;
use crate::recovery;
use crate::rest_push;
use crate::routine;
use crate::settings;
use crate::sync;
use crate::training_record;
use crate::training_session;
use crate::training_session_exercise;

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
/// 記録の CSV の書き出しを作る経路の名前。
pub const EXPORTS_ROUTE_NAME: &str = "exports";
/// 記録の CSV をダウンロードする経路の名前。
pub const EXPORT_DOWNLOAD_ROUTE_NAME: &str = "export_download";
/// 同期の操作を適用する経路の名前。
pub const SYNC_OPERATION_ROUTE_NAME: &str = "sync_operation";
/// トレーニングを終了する経路の名前。
pub const TRAINING_FINISH_ROUTE_NAME: &str = "training_finish";
/// トレーニングのサマリーを返す経路の名前。
pub const TRAINING_SUMMARY_ROUTE_NAME: &str = "training_summary";
/// サーバーに保存済みのトレーニングを返す経路の名前。
pub const TRAINING_GET_ROUTE_NAME: &str = "training_get";
/// 種目ごととトレーニング全体の前回を返す経路の名前。
pub const TRAINING_PREVIOUS_ROUTE_NAME: &str = "training_previous";
/// 種目ライブラリの一覧を返す経路の名前。
pub const EXERCISES_ROUTE_NAME: &str = "exercises";
/// 種目ライブラリの種目の詳細を返す経路の名前。
pub const EXERCISE_ROUTE_NAME: &str = "exercise";
/// 種目のお気に入りを更新する経路の名前。
pub const EXERCISE_FAVORITE_ROUTE_NAME: &str = "exercise_favorite";
/// マスターの種目の上書きを初期化する経路の名前。
pub const EXERCISE_RESET_ROUTE_NAME: &str = "exercise_reset";
/// 代わりの種目の候補を返す経路の名前。
pub const EXERCISE_ALTERNATIVES_ROUTE_NAME: &str = "exercise_alternatives";
/// 過去に記録した種目の ID を返す経路の名前。
pub const RECORDED_EXERCISES_ROUTE_NAME: &str = "recorded_exercises";
/// 種目別の履歴を返す経路の名前。
pub const EXERCISE_HISTORY_ROUTE_NAME: &str = "exercise_history";
/// 種目別のグラフを返す経路の名前。
pub const EXERCISE_GRAPH_ROUTE_NAME: &str = "exercise_graph";
/// 種目の目標重量を保存し、取り消す経路の名前。
pub const EXERCISE_GOAL_ROUTE_NAME: &str = "exercise_goal";
/// 部位別の分析を返す経路の名前。
pub const ANALYSIS_BODY_PARTS_ROUTE_NAME: &str = "analysis_body_parts";
/// 部位区分ごとの回復状況を返す経路の名前。
pub const ANALYSIS_RECOVERY_ROUTE_NAME: &str = "analysis_recovery";
/// トレーニングの編集権を引き継ぐ経路の名前。
pub const TRAINING_TAKEOVER_ROUTE_NAME: &str = "training_takeover";
/// カレンダーの月を返す経路の名前。
pub const CALENDAR_MONTH_ROUTE_NAME: &str = "calendar_month";
/// カレンダーの日を返す経路の名前。
pub const CALENDAR_DAY_ROUTE_NAME: &str = "calendar_day";
/// 身体情報を取得し、更新する経路の名前。
pub const PROFILE_ROUTE_NAME: &str = "profile";
/// 利用者共通の設定を取得し、更新する経路の名前。
pub const SETTINGS_ROUTE_NAME: &str = "settings";
/// VAPID の公開鍵を返す経路の名前。
pub const PUSH_PUBLIC_KEY_ROUTE_NAME: &str = "push_public_key";
/// プッシュ通知の購読を登録し、解除する経路の名前。
pub const PUSH_SUBSCRIPTIONS_ROUTE_NAME: &str = "push_subscriptions";
/// レスト終了のプッシュ通知を予約する経路の名前。
pub const REST_RESERVATIONS_ROUTE_NAME: &str = "rest_reservations";
/// レスト終了のプッシュ通知の予約を変更する経路の名前。
pub const REST_RESERVATION_UPDATE_ROUTE_NAME: &str = "rest_reservation_update";
/// レスト終了のプッシュ通知の予約を取り消す経路の名前。
pub const REST_RESERVATION_DELETE_ROUTE_NAME: &str = "rest_reservation_delete";
/// プッシュ通知の受信時刻を記録する経路の名前。
pub const PUSH_RECEIVED_ROUTE_NAME: &str = "push_received";
/// ルーティンの一覧を返す経路の名前。
pub const ROUTINES_ROUTE_NAME: &str = "routines";
/// ルーティンからトレーニングを開始する経路の名前。
pub const ROUTINE_START_ROUTE_NAME: &str = "routine_start";
/// AI の生成の状態を返す経路の名前。
pub const AI_JOB_ROUTE_NAME: &str = "ai_job";
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
        .post_async("/api/exports", export::create)
        .get_async("/api/exports/:id", export::download)
        .get_async("/api/profile", profile::get)
        .put_async("/api/profile", profile::update)
        .get_async("/api/settings", settings::get)
        .put_async("/api/settings", settings::update)
        .get_async("/api/push/public-key", push::public_key)
        .post_async("/api/push/subscriptions", push::create)
        .delete_async("/api/push/subscriptions", push::delete)
        .post_async("/api/push/received", rest_push::received)
        .post_async("/api/rest/reservations", rest_push::create)
        .patch_async("/api/rest/reservations/:id", rest_push::update)
        .delete_async("/api/rest/reservations/:id", rest_push::delete)
        .get_async("/api/ai/jobs/:id", ai_job::status)
        .get_async("/api/me/passkeys", passkey::list)
        .delete_async("/api/account", account::delete)
        .post_async("/api/sync/operations", sync::apply_operation)
        .get_async("/api/routines", routine::list)
        .post_async("/api/routines/:id/start", routine::start)
        .post_async("/api/trainings/:id/finish", training_session::finish)
        .get_async("/api/trainings/:id/summary", training_session::summary)
        .get_async(
            "/api/trainings/:id/previous",
            training_session_exercise::previous,
        )
        .get_async("/api/trainings/:id", training_record::get)
        .post_async("/api/trainings/:id/takeover", training_record::takeover)
        .get_async("/api/exercises", library::exercises)
        .post_async("/api/exercises", custom_exercise::create)
        .get_async("/api/exercises/recorded", alternative::recorded)
        .get_async("/api/exercises/:id", library::exercise_detail)
        .put_async("/api/exercises/:id", library::update_exercise)
        .delete_async("/api/exercises/:id", library::delete_exercise)
        .post_async("/api/exercises/:id/reset", exercise_override::reset)
        .get_async("/api/exercises/:id/alternatives", alternative::alternatives)
        .put_async("/api/exercises/:id/favorite", library::add_favorite)
        .delete_async("/api/exercises/:id/favorite", library::remove_favorite)
        .get_async("/api/exercises/:id/history", history::history)
        .get_async("/api/exercises/:id/graph", history::graph)
        .put_async("/api/exercises/:id/goal", goal::put)
        .delete_async("/api/exercises/:id/goal", goal::delete)
        .get_async("/api/analysis/body-parts", analysis::body_parts)
        .get_async("/api/analysis/recovery", recovery::recovery)
        .get_async("/api/calendar", calendar::month)
        .get_async("/api/calendar/:date", calendar::day)
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
    if ai_job_id_path(path).is_some() {
        return AI_JOB_ROUTE_NAME;
    }
    if path.starts_with("/api/me/passkeys/") {
        return "passkeys_delete";
    }
    if export_id_path(path).is_some() {
        return EXPORT_DOWNLOAD_ROUTE_NAME;
    }
    if path.starts_with("/api/exercises/") && path.ends_with("/favorite") {
        return EXERCISE_FAVORITE_ROUTE_NAME;
    }
    if path == "/api/rest/reservations" {
        return REST_RESERVATIONS_ROUTE_NAME;
    }
    if path.starts_with("/api/rest/reservations/") {
        return REST_RESERVATION_UPDATE_ROUTE_NAME;
    }
    if path.starts_with("/api/exercises/") && path.ends_with("/reset") {
        return EXERCISE_RESET_ROUTE_NAME;
    }
    if path == "/api/exercises/recorded" {
        return RECORDED_EXERCISES_ROUTE_NAME;
    }
    if path.starts_with("/api/exercises/") && path.ends_with("/alternatives") {
        return EXERCISE_ALTERNATIVES_ROUTE_NAME;
    }
    if path.starts_with("/api/exercises/") && path.ends_with("/goal") {
        return EXERCISE_GOAL_ROUTE_NAME;
    }
    if exercise_id_path(path).is_some() {
        return EXERCISE_ROUTE_NAME;
    }
    if path.starts_with("/api/routines/") && path.ends_with("/start") {
        return ROUTINE_START_ROUTE_NAME;
    }
    if path.starts_with("/api/trainings/") && path.ends_with("/finish") {
        return TRAINING_FINISH_ROUTE_NAME;
    }
    if path.starts_with("/api/trainings/") && path.ends_with("/summary") {
        return TRAINING_SUMMARY_ROUTE_NAME;
    }
    if path.starts_with("/api/trainings/") && path.ends_with("/previous") {
        return TRAINING_PREVIOUS_ROUTE_NAME;
    }
    if path.starts_with("/api/exercises/") && path.ends_with("/history") {
        return EXERCISE_HISTORY_ROUTE_NAME;
    }
    if path.starts_with("/api/exercises/") && path.ends_with("/graph") {
        return EXERCISE_GRAPH_ROUTE_NAME;
    }
    if path.starts_with("/api/calendar/") {
        return CALENDAR_DAY_ROUTE_NAME;
    }
    if path.starts_with("/api/trainings/") && path.ends_with("/takeover") {
        return TRAINING_TAKEOVER_ROUTE_NAME;
    }
    if training_id_path(path).is_some() {
        return TRAINING_GET_ROUTE_NAME;
    }
    match path {
        HEALTH_PATH => HEALTH_ROUTE_NAME,
        "/api/auth/google/start" => GOOGLE_START_ROUTE_NAME,
        "/api/auth/google/callback" => GOOGLE_CALLBACK_ROUTE_NAME,
        "/api/auth/logout" => LOGOUT_ROUTE_NAME,
        "/api/me" => ME_ROUTE_NAME,
        "/api/exports" => EXPORTS_ROUTE_NAME,
        "/api/profile" => PROFILE_ROUTE_NAME,
        "/api/settings" => SETTINGS_ROUTE_NAME,
        "/api/push/public-key" => PUSH_PUBLIC_KEY_ROUTE_NAME,
        "/api/push/subscriptions" => PUSH_SUBSCRIPTIONS_ROUTE_NAME,
        "/api/push/received" => PUSH_RECEIVED_ROUTE_NAME,
        "/api/routines" => ROUTINES_ROUTE_NAME,
        "/api/sync/operations" => SYNC_OPERATION_ROUTE_NAME,
        "/api/exercises" => EXERCISES_ROUTE_NAME,
        "/api/exercises/:id" => EXERCISE_ROUTE_NAME,
        "/api/analysis/body-parts" => ANALYSIS_BODY_PARTS_ROUTE_NAME,
        "/api/analysis/recovery" => ANALYSIS_RECOVERY_ROUTE_NAME,
        "/api/calendar" => CALENDAR_MONTH_ROUTE_NAME,
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

/// `/api/trainings/:id` の形の経路から、トレーニングの ID の部分を取り出す。
fn training_id_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/api/trainings/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest)
}

/// `/api/ai/jobs/:id` の形の経路から、生成の ID の部分を取り出す。
fn ai_job_id_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/api/ai/jobs/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest)
}

/// `/api/exercises/:id` の形の経路から、種目の ID の部分を取り出す。
fn exercise_id_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/api/exercises/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest)
}

/// `/api/exports/:id` の形の経路から、書き出しの ID の部分を取り出す。
fn export_id_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/api/exports/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    Some(rest)
}

#[cfg(test)]
mod tests {
    use super::AI_JOB_ROUTE_NAME;
    use super::ANALYSIS_BODY_PARTS_ROUTE_NAME;
    use super::ANALYSIS_RECOVERY_ROUTE_NAME;
    use super::{
        route_name, EXERCISES_ROUTE_NAME, EXERCISE_ALTERNATIVES_ROUTE_NAME,
        EXERCISE_FAVORITE_ROUTE_NAME, EXERCISE_RESET_ROUTE_NAME, EXERCISE_ROUTE_NAME,
        GOOGLE_CALLBACK_ROUTE_NAME, GOOGLE_START_ROUTE_NAME, HEALTH_ROUTE_NAME, LOGOUT_ROUTE_NAME,
        ME_ROUTE_NAME, NOT_FOUND_ROUTE_NAME, PROFILE_ROUTE_NAME, PUSH_PUBLIC_KEY_ROUTE_NAME,
        PUSH_SUBSCRIPTIONS_ROUTE_NAME, RECORDED_EXERCISES_ROUTE_NAME, ROUTINES_ROUTE_NAME,
        ROUTINE_START_ROUTE_NAME, SETTINGS_ROUTE_NAME, SYNC_OPERATION_ROUTE_NAME,
        TRAINING_FINISH_ROUTE_NAME, TRAINING_GET_ROUTE_NAME, TRAINING_PREVIOUS_ROUTE_NAME,
        TRAINING_SUMMARY_ROUTE_NAME, TRAINING_TAKEOVER_ROUTE_NAME,
    };
    use super::{
        CALENDAR_DAY_ROUTE_NAME, CALENDAR_MONTH_ROUTE_NAME, EXERCISE_GOAL_ROUTE_NAME,
        EXERCISE_GRAPH_ROUTE_NAME, EXERCISE_HISTORY_ROUTE_NAME,
    };
    use super::{EXPORTS_ROUTE_NAME, EXPORT_DOWNLOAD_ROUTE_NAME};

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
    fn names_the_profile_route() {
        assert_eq!(route_name("/api/profile"), PROFILE_ROUTE_NAME);
    }

    #[test]
    fn names_the_settings_route() {
        assert_eq!(route_name("/api/settings"), SETTINGS_ROUTE_NAME);
    }

    #[test]
    fn names_the_push_routes() {
        assert_eq!(
            route_name("/api/push/public-key"),
            PUSH_PUBLIC_KEY_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/push/subscriptions"),
            PUSH_SUBSCRIPTIONS_ROUTE_NAME
        );
    }

    #[test]
    fn names_the_sync_operation_route() {
        assert_eq!(
            route_name("/api/sync/operations"),
            SYNC_OPERATION_ROUTE_NAME
        );
    }

    #[test]
    fn names_the_training_session_routes() {
        assert_eq!(
            route_name("/api/trainings/3f2504e0-4f89-41d3-9a0c-0305e82c6601/finish"),
            TRAINING_FINISH_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/trainings/3f2504e0-4f89-41d3-9a0c-0305e82c6601/summary"),
            TRAINING_SUMMARY_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/trainings/3f2504e0-4f89-41d3-9a0c-0305e82c6601/previous"),
            TRAINING_PREVIOUS_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/trainings/3f2504e0-4f89-41d3-9a0c-0305e82c6601"),
            TRAINING_GET_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/trainings/3f2504e0-4f89-41d3-9a0c-0305e82c6601/takeover"),
            TRAINING_TAKEOVER_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/trainings/"),
            NOT_FOUND_ROUTE_NAME,
            "an empty training id is not a route"
        );
    }

    #[test]
    fn names_the_routine_routes() {
        assert_eq!(route_name("/api/routines"), ROUTINES_ROUTE_NAME);
        assert_eq!(
            route_name("/api/routines/3f2504e0-4f89-41d3-9a0c-0305e82c5301/start"),
            ROUTINE_START_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/routines/"),
            NOT_FOUND_ROUTE_NAME,
            "an empty routine path is not a route"
        );
    }

    #[test]
    fn names_the_export_routes() {
        assert_eq!(route_name("/api/exports"), EXPORTS_ROUTE_NAME);
        assert_eq!(
            route_name("/api/exports/3f2504e0-4f89-41d3-9a0c-0305e82c6601"),
            EXPORT_DOWNLOAD_ROUTE_NAME
        );
        assert_eq!(route_name("/api/exports/"), NOT_FOUND_ROUTE_NAME);
    }

    #[test]
    fn names_the_calendar_routes() {
        assert_eq!(route_name("/api/calendar"), CALENDAR_MONTH_ROUTE_NAME);
        assert_eq!(
            route_name("/api/calendar/2026-11-12"),
            CALENDAR_DAY_ROUTE_NAME
        );
    }

    #[test]
    fn names_the_exercise_routes() {
        assert_eq!(route_name("/api/exercises"), EXERCISES_ROUTE_NAME);
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c3301"),
            EXERCISE_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c3301/favorite"),
            EXERCISE_FAVORITE_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c3301/reset"),
            EXERCISE_RESET_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c3301/alternatives"),
            EXERCISE_ALTERNATIVES_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/exercises/recorded"),
            RECORDED_EXERCISES_ROUTE_NAME
        );
        assert_eq!(route_name("/api/exercises/"), NOT_FOUND_ROUTE_NAME);
    }

    #[test]
    fn names_the_exercise_history_and_graph_routes() {
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c4803/history"),
            EXERCISE_HISTORY_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c4803/graph"),
            EXERCISE_GRAPH_ROUTE_NAME
        );
        assert_eq!(
            route_name("/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c4803/goal"),
            EXERCISE_GOAL_ROUTE_NAME
        );
    }

    #[test]
    fn names_the_body_part_analysis_route() {
        assert_eq!(
            route_name("/api/analysis/body-parts"),
            ANALYSIS_BODY_PARTS_ROUTE_NAME
        );
    }

    #[test]
    fn names_the_recovery_route() {
        assert_eq!(
            route_name("/api/analysis/recovery"),
            ANALYSIS_RECOVERY_ROUTE_NAME
        );
    }

    #[test]
    fn names_the_ai_job_route() {
        assert_eq!(
            route_name("/api/ai/jobs/3f2504e0-4f89-41d3-9a0c-0305e82c6601"),
            AI_JOB_ROUTE_NAME
        );
        assert_eq!(route_name("/api/ai/jobs/"), NOT_FOUND_ROUTE_NAME);
        assert_eq!(
            route_name("/api/ai/jobs/3f2504e0-4f89-41d3-9a0c-0305e82c6601/extra"),
            NOT_FOUND_ROUTE_NAME
        );
    }

    #[test]
    fn names_unknown_routes_as_not_found() {
        assert_eq!(route_name("/api/unknown"), NOT_FOUND_ROUTE_NAME);
        assert_eq!(route_name("/"), NOT_FOUND_ROUTE_NAME);
    }
}
