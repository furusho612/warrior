//! 記録の CSV の書き出しと、認証済みの経路からのダウンロード (REQ-MYP-008、ADR-017)。
//!
//! 書き出しは利用者の全期間の記録から CSV を組み立て、R2 の
//! `exports/<利用者 ID>/<書き出し ID>.csv` に保存する。D1 には書き出しのメタデータだけを置く。
//! ダウンロードは認証を必須とし、R2 のオブジェクトを Worker が読んで返す。
//! 署名付き URL と公開 URL は使わない。R2 のオブジェクトが無い書き出しは 404 にする。

use serde::{Deserialize, Serialize};
use warrior_core::csv::{self, CsvSet};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Env, Request, Response, Result, RouteContext};

use crate::auth::{now_iso, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::random::random_uuid;
use crate::stored_sets::{self, StoredTrainingSets};

/// R2 に保存した書き出しの状態。
const STATUS_READY: &str = "ready";

/// CSV の応答の Content-Type。
const CSV_CONTENT_TYPE: &str = "text/csv; charset=utf-8";

/// `POST /api/exports` の応答。
#[derive(Debug, Serialize)]
struct ExportJob {
    /// 書き出しの ID。
    id: String,
    /// 書き出しの状態。
    status: &'static str,
    /// 書き出しの作成時刻 (ISO 8601 の UTC)。
    created_at: String,
}

/// D1 から読む書き出しのメタデータ。
#[derive(Debug, Deserialize)]
struct ExportJobRow {
    status: String,
    r2_key: String,
}

/// `POST /api/exports`
pub async fn create(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let trainings = stored_sets::load_all(&db, &authenticated.id).await?;
    let sets = csv_sets(&trainings)?;
    let document = csv::encode_document(&sets);

    let export_id = random_uuid()?;
    let key = export_key(&authenticated.id, &export_id);
    let bucket = env.bucket("BUCKET")?;
    bucket.put(key.as_str(), document).execute().await?;

    let created_at = now_iso();
    let statement = insert_job_statement(&export_id, &authenticated.id, &key, &created_at);
    if let Err(error) = db.run(&statement).await {
        remove_object_after_failure(&env, &key).await;
        return Err(error);
    }
    let job = ExportJob {
        id: export_id,
        status: STATUS_READY,
        created_at,
    };
    Ok(Response::from_json(&job)?.with_status(201))
}

/// `GET /api/exports/:id`
pub async fn download(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(export_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(job) = load_job(&db, &export_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if job.status != STATUS_READY {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    let bucket = env.bucket("BUCKET")?;
    let Some(object) = bucket.get(job.r2_key.as_str()).execute().await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let Some(body) = object.body() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let mut csv_response = Response::from_body(body.response_body()?)?;
    let headers = csv_response.headers_mut();
    headers.set("Content-Type", CSV_CONTENT_TYPE)?;
    headers.set("Content-Disposition", &content_disposition(&export_id))?;
    // 書き出しは利用者の記録を含むため、端末や中継のキャッシュに残さない。
    headers.set("Cache-Control", "no-store")?;
    Ok(csv_response)
}

fn export_key(user_id: &str, export_id: &str) -> String {
    format!("exports/{user_id}/{export_id}.csv")
}

fn content_disposition(export_id: &str) -> String {
    format!("attachment; filename=\"warrior-records-{export_id}.csv\"")
}

fn insert_job_statement(
    export_id: &str,
    user_id: &str,
    key: &str,
    created_at: &str,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO export_jobs (id, user_id, status, r2_key, created_at) \
         VALUES (?, ?, ?, ?, ?)",
        vec![
            SqlValue::Text(export_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(STATUS_READY.to_owned()),
            SqlValue::Text(key.to_owned()),
            SqlValue::Text(created_at.to_owned()),
        ],
    )
}

async fn load_job(db: &Db, export_id: &str, user_id: &str) -> Result<Option<ExportJobRow>> {
    db.first(&SqlStatement::new(
        "SELECT status, r2_key FROM export_jobs WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(export_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

/// メタデータを保存できなかった書き出しのオブジェクトを消す。
async fn remove_object_after_failure(env: &Env, key: &str) {
    let deleted = match env.bucket("BUCKET") {
        Ok(bucket) => bucket.delete(key).await,
        Err(error) => Err(error),
    };
    if let Err(error) = deleted {
        console_error!(
            "failed to delete the export object after the metadata write failed: {error}"
        );
    }
}

fn csv_sets(trainings: &[StoredTrainingSets]) -> Result<Vec<CsvSet>> {
    let mut sets = Vec::new();
    for training in trainings {
        for exercise in &training.exercises {
            let memo = csv::join_memos(&training.memo, &exercise.memo);
            for (index, record) in exercise.sets.iter().enumerate() {
                let Some(grip) = exercise.grip_selection(index) else {
                    return Err(invalid_stored_grip());
                };
                sets.push(CsvSet {
                    date: training.date.clone(),
                    exercise_name: exercise.snapshot.name.clone(),
                    body_part: exercise.snapshot.body_part,
                    set_number: set_number(index)?,
                    record: record.clone(),
                    grip,
                    memo: memo.clone(),
                });
            }
        }
    }
    Ok(sets)
}

fn set_number(index: usize) -> Result<u32> {
    u32::try_from(index + 1)
        .map_err(|_| worker::Error::RustError("the set number is out of range".into()))
}

fn invalid_stored_grip() -> worker::Error {
    console_error!("the stored grip is invalid");
    worker::Error::RustError("the stored grip is invalid".into())
}
