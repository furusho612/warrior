//! 種目ライブラリの一覧。
//!
//! 記録への種目の選択が使う。絞り込みと続きのページも、この応答に項目を足して表す。
//! 一覧の並びは種目マスターの `sort_order` に従う。

use warrior_core::exercise::ExerciseList;
use worker::{Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::exercise;

/// `GET /api/exercises`
pub async fn exercises(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(_authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    match exercise::list_master(&db).await {
        Ok(exercises) => Response::from_json(&ExerciseList { exercises }),
        Err(error) => {
            worker::console_error!("failed to list the exercises: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}
