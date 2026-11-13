//! サーバーに保存済みのトレーニングの取得と、編集権の引き継ぎ。
//!
//! 同じアカウントでログインしたほかの端末が記録を閲覧し、記録中のトレーニングを
//! 引き継ぐために使う (REQ-NFR-006、REQ-NFR-015)。
//! 応答は `warrior_core::training_record` の形で返し、端末は IndexedDB のセッション木を
//! この内容で置き換える。

use serde::Deserialize;
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::SessionRecord;
use warrior_core::training_record::{
    SetRecordRow, SetSegmentRecord, SupersetMemberRecord, SupersetRecord, TrainingExerciseRecord,
    TrainingRecord,
};
use warrior_core::training_session::SessionSource;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::ownership;

const SESSION_QUERY: &str = "SELECT id, date, started_at, memo, source_kind, routine_id, \
     program_day_id, custom_program_day_id, finished_at, duration_seconds, total_weight_kg, \
     exercise_count, set_count, personal_bests, created_at, updated_at \
     FROM training_sessions WHERE id = ? AND user_id = ?";

const EXERCISES_QUERY: &str = "SELECT id, training_id, exercise_id, snapshot, position, \
     created_at, updated_at \
     FROM training_exercises WHERE user_id = ? AND training_id = ? ORDER BY position, id";

const SETS_QUERY: &str = "SELECT s.id, s.training_exercise_id, s.position, s.kind, s.completed, \
     s.rpe, s.grip, s.recorded_at, s.created_at, s.updated_at \
     FROM sets s JOIN training_exercises te ON te.id = s.training_exercise_id \
     WHERE te.user_id = ? AND te.training_id = ? \
     ORDER BY s.training_exercise_id, s.position";

const SEGMENTS_QUERY: &str = "SELECT sg.id, sg.set_id, sg.position, sg.weight_kg, sg.reps, \
     sg.momentum_reps, sg.assisted_reps, sg.duration_seconds, sg.left_weight_kg, sg.left_reps, \
     sg.left_duration_seconds, sg.right_weight_kg, sg.right_reps, sg.right_duration_seconds, \
     sg.created_at, sg.updated_at \
     FROM set_segments sg JOIN sets s ON s.id = sg.set_id \
     JOIN training_exercises te ON te.id = s.training_exercise_id \
     WHERE te.user_id = ? AND te.training_id = ? ORDER BY sg.set_id, sg.position";

const SUPERSETS_QUERY: &str = "SELECT id, training_id, position, created_at, updated_at \
     FROM supersets WHERE user_id = ? AND training_id = ? ORDER BY position, id";

const SUPERSET_MEMBERS_QUERY: &str = "SELECT sm.superset_id, sm.training_exercise_id, sm.position \
     FROM superset_members sm JOIN supersets ss ON ss.id = sm.superset_id \
     WHERE ss.user_id = ? AND ss.training_id = ? ORDER BY sm.superset_id, sm.position";

#[derive(Debug, Deserialize)]
struct SessionRecordRow {
    id: String,
    date: String,
    started_at: Option<String>,
    memo: String,
    source_kind: String,
    routine_id: Option<String>,
    program_day_id: Option<String>,
    custom_program_day_id: Option<String>,
    finished_at: Option<String>,
    duration_seconds: Option<i64>,
    total_weight_kg: Option<f64>,
    exercise_count: Option<i64>,
    set_count: Option<i64>,
    personal_bests: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct SupersetRow {
    id: String,
    training_id: String,
    position: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct SupersetMemberRow {
    superset_id: String,
    training_exercise_id: String,
    position: i64,
}

#[derive(Debug, Deserialize)]
struct SessionStateRow {
    finished_at: Option<String>,
}

/// `GET /api/trainings/:id`
pub async fn get(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    record_response(&db, &training_id, &authenticated.id).await
}

/// `POST /api/trainings/:id/takeover`
pub async fn takeover(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(state) = load_state(&db, &training_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if state.finished_at.is_some() {
        return Ok(response(
            ErrorCode::Conflict,
            "the finished training cannot be taken over",
        ));
    }
    let Some(terminal_id) = ownership::ensure(&db, &request, &authenticated.id).await? else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the terminal token is required",
        ));
    };
    let statement = SqlStatement::new(
        "UPDATE training_sessions SET owner_terminal_id = ? \
         WHERE id = ? AND user_id = ? AND finished_at IS NULL",
        vec![
            SqlValue::Text(terminal_id),
            SqlValue::Text(training_id.clone()),
            SqlValue::Text(authenticated.id.clone()),
        ],
    );
    db.run(&statement).await?;
    record_response(&db, &training_id, &authenticated.id).await
}

async fn load_state(db: &Db, training_id: &str, user_id: &str) -> Result<Option<SessionStateRow>> {
    db.first(&SqlStatement::new(
        "SELECT finished_at FROM training_sessions WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

async fn record_response(db: &Db, training_id: &str, user_id: &str) -> Result<Response> {
    let Some(record) = load_record(db, training_id, user_id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    Response::from_json(&record)
}

async fn load_record(db: &Db, training_id: &str, user_id: &str) -> Result<Option<TrainingRecord>> {
    let Some(row) = db
        .first::<SessionRecordRow>(&SqlStatement::new(
            SESSION_QUERY,
            vec![
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?
    else {
        return Ok(None);
    };
    let session = stored_session(row).map_err(invalid_stored_data)?;
    Ok(Some(TrainingRecord {
        session,
        exercises: load_rows::<TrainingExerciseRecord>(db, EXERCISES_QUERY, user_id, training_id)
            .await?,
        sets: load_rows::<SetRecordRow>(db, SETS_QUERY, user_id, training_id).await?,
        segments: load_rows::<SetSegmentRecord>(db, SEGMENTS_QUERY, user_id, training_id).await?,
        supersets: load_supersets(db, user_id, training_id).await?,
    }))
}

async fn load_supersets(db: &Db, user_id: &str, training_id: &str) -> Result<Vec<SupersetRecord>> {
    let supersets = load_rows::<SupersetRow>(db, SUPERSETS_QUERY, user_id, training_id).await?;
    let members =
        load_rows::<SupersetMemberRow>(db, SUPERSET_MEMBERS_QUERY, user_id, training_id).await?;
    Ok(supersets
        .into_iter()
        .map(|row| {
            let members = member_records(&members, &row.id);
            SupersetRecord {
                id: row.id,
                training_id: row.training_id,
                position: row.position,
                members,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        })
        .collect())
}

fn member_records(members: &[SupersetMemberRow], superset_id: &str) -> Vec<SupersetMemberRecord> {
    members
        .iter()
        .filter(|member| member.superset_id == superset_id)
        .map(|member| SupersetMemberRecord {
            superset_id: member.superset_id.clone(),
            training_exercise_id: member.training_exercise_id.clone(),
            position: member.position,
        })
        .collect()
}

async fn load_rows<T: serde::de::DeserializeOwned>(
    db: &Db,
    sql: &str,
    user_id: &str,
    training_id: &str,
) -> Result<Vec<T>> {
    let result = db
        .all(&SqlStatement::new(
            sql,
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    result.results::<T>()
}

fn stored_session(row: SessionRecordRow) -> Result<SessionRecord, StoredRecordError> {
    let source_kind =
        SessionSource::parse(&row.source_kind).ok_or(StoredRecordError::UnknownSourceKind)?;
    let personal_bests = match row.personal_bests {
        Some(personal_bests) => serde_json::from_str(&personal_bests)
            .map_err(|_| StoredRecordError::InvalidPersonalBests)?,
        None => Vec::new(),
    };
    Ok(SessionRecord {
        id: row.id,
        date: row.date,
        started_at: row.started_at,
        memo: row.memo,
        confirmed_memo: String::new(),
        confirmed_sequence: 0,
        source_kind,
        routine_id: row.routine_id,
        program_day_id: row.program_day_id,
        custom_program_day_id: row.custom_program_day_id,
        finished_at: row.finished_at,
        duration_seconds: row.duration_seconds,
        total_weight_kg: row.total_weight_kg,
        exercise_count: to_u32(row.exercise_count),
        set_count: to_u32(row.set_count),
        personal_bests,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

/// 保存された行から記録を組み立てる誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredRecordError {
    /// 開始元の文字列が既知の値でない。
    UnknownSourceKind,
    /// 自己ベストの JSON を復元できない。
    InvalidPersonalBests,
}

fn invalid_stored_data(error: StoredRecordError) -> worker::Error {
    console_error!("the stored training record is invalid: {error:?}");
    worker::Error::RustError("the stored training record is invalid".into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{stored_session, SessionRecordRow, StoredRecordError};
    use warrior_core::training_session::SessionSource;

    fn row() -> SessionRecordRow {
        SessionRecordRow {
            id: "3f2504e0-4f89-41d3-9a0c-0305e82c4601".to_owned(),
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            memo: "メモ".to_owned(),
            source_kind: "routine".to_owned(),
            routine_id: Some("3f2504e0-4f89-41d3-9a0c-0305e82c4602".to_owned()),
            program_day_id: None,
            custom_program_day_id: None,
            finished_at: None,
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: Some(3),
            personal_bests: None,
            created_at: "2026-11-12T03:00:00Z".to_owned(),
            updated_at: "2026-11-12T04:00:00Z".to_owned(),
        }
    }

    #[test]
    fn a_stored_session_becomes_the_client_record() {
        let session = stored_session(row()).expect("a stored session");
        assert_eq!(session.id, "3f2504e0-4f89-41d3-9a0c-0305e82c4601");
        assert_eq!(session.memo, "メモ");
        assert_eq!(session.source_kind, SessionSource::Routine);
        assert_eq!(
            session.routine_id.as_deref(),
            Some("3f2504e0-4f89-41d3-9a0c-0305e82c4602")
        );
        assert_eq!(session.set_count, Some(3));
        assert!(session.personal_bests.is_empty());
        assert_eq!(session.confirmed_sequence, 0);
    }

    #[test]
    fn stored_personal_bests_are_parsed() {
        let mut row = row();
        row.personal_bests = Some(
            json!([{
                "exercise_id": "bench-press",
                "record_mode": "weight_and_reps",
                "laterality": "bilateral",
                "kind": "max_weight",
                "value": 100.0
            }])
            .to_string(),
        );
        let session = stored_session(row).expect("a stored session");
        assert_eq!(session.personal_bests.len(), 1);
        assert_eq!(session.personal_bests[0].value, 100.0);
    }

    #[test]
    fn an_unknown_source_kind_is_rejected() {
        let mut row = row();
        row.source_kind = "unknown".to_owned();
        assert_eq!(
            stored_session(row),
            Err(StoredRecordError::UnknownSourceKind)
        );
    }

    #[test]
    fn invalid_personal_bests_are_rejected() {
        let mut row = row();
        row.personal_bests = Some("not-json".to_owned());
        assert_eq!(
            stored_session(row),
            Err(StoredRecordError::InvalidPersonalBests)
        );
    }
}
