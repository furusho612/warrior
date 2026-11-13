//! トレーニングの終了と、保存したサマリーの取得。
//!
//! 終了の時点でサーバーが D1 の種目、セット、段から今回のセッションのベストを計算し、
//! 過去の終了済みトレーニングのベストと比較して、更新した自己ベストを返す。
//! 今回のセッションのベストは `training_sessions.personal_bests` に保存する (ADR-015)。
//! 保存と読み直しは 1 つの `batch()` にまとめ、終了済みのトレーニングの終了は
//! 保存済みの値を返す (ADR-002)。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::exercise::{ExerciseSnapshot, GripSelection, Laterality};
use warrior_core::jst::JstDate;
use warrior_core::ownership::{decide_write, WriteDecision};
use warrior_core::set_calc::{
    BestKey, SetKind, SetRecord, SetSegment, SetValidationError, SideValue,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::{
    summary_from_sets, updated_personal_bests, PersonalBestItem, SessionSet, TrainingSessionSummary,
};
use worker::{console_error, Date, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::ownership;

#[derive(Debug, Deserialize)]
struct SessionSummaryRow {
    date: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    owner_terminal_id: Option<String>,
    duration_seconds: Option<i64>,
    total_weight_kg: Option<f64>,
    exercise_count: Option<i64>,
    set_count: Option<i64>,
    personal_bests: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PersonalBestsRow {
    personal_bests: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SetSegmentRow {
    training_exercise_id: String,
    snapshot: String,
    set_id: String,
    kind: String,
    set_rpe: Option<f64>,
    grip: Option<String>,
    segment_id: Option<String>,
    weight_kg: Option<f64>,
    reps: Option<i64>,
    momentum_reps: Option<i64>,
    assisted_reps: Option<i64>,
    duration_seconds: Option<i64>,
    left_weight_kg: Option<f64>,
    left_reps: Option<i64>,
    left_duration_seconds: Option<i64>,
    right_weight_kg: Option<f64>,
    right_reps: Option<i64>,
    right_duration_seconds: Option<i64>,
}

const SESSION_SUMMARY_COLUMNS: &str = "date, started_at, finished_at, owner_terminal_id, \
     duration_seconds, total_weight_kg, exercise_count, set_count, personal_bests";

const SET_SEGMENT_QUERY: &str = "SELECT te.id AS training_exercise_id, te.snapshot, \
     s.id AS set_id, s.kind AS kind, s.rpe AS set_rpe, s.grip AS grip, \
     sg.id AS segment_id, \
     sg.weight_kg, sg.reps, sg.momentum_reps, sg.assisted_reps, sg.duration_seconds, \
     sg.left_weight_kg, sg.left_reps, sg.left_duration_seconds, \
     sg.right_weight_kg, sg.right_reps, sg.right_duration_seconds \
     FROM training_exercises te \
     JOIN sets s ON s.training_exercise_id = te.id AND s.completed = 1 \
     LEFT JOIN set_segments sg ON sg.set_id = s.id AND sg.user_id = ? \
     WHERE te.user_id = ? AND te.training_id = ? AND s.user_id = ? \
     ORDER BY te.position, s.position, sg.position";

/// `POST /api/trainings/:id/finish`
pub async fn finish(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(session_row) = load_session(&db, &training_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if session_row.finished_at.is_some() {
        return summary_response(&db, &session_row, &training_id, &authenticated.id).await;
    }
    let Some(terminal_id) = ownership::ensure(&db, &request, &authenticated.id).await? else {
        return Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        ));
    };
    if decide_write(
        false,
        session_row.owner_terminal_id.as_deref(),
        &terminal_id,
    ) == WriteDecision::Forbid
    {
        return Ok(response(
            ErrorCode::Forbidden,
            "the training is being edited on another terminal",
        ));
    }
    let Some(started_at) = session_row.started_at.as_deref() else {
        return Ok(response(
            ErrorCode::BadRequest,
            "the training has no start time",
        ));
    };
    let Some(started_seconds) = datetime::parse_utc(started_at) else {
        console_error!("the stored start time is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let Some(achieved_on) = JstDate::parse(&session_row.date) else {
        console_error!("the stored date is invalid");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let sets = match load_sets(&db, &training_id, &authenticated.id).await {
        Ok(sets) => sets,
        Err(error) => {
            console_error!("failed to build the sets: {error:?}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let now_seconds = current_seconds();
    let finished_at = datetime::format_utc(now_seconds);
    let duration_seconds = now_seconds.saturating_sub(started_seconds);
    let summary = summary_from_sets(achieved_on, duration_seconds, &sets);
    let personal_bests = match serde_json::to_string(&summary.personal_bests) {
        Ok(personal_bests) => personal_bests,
        Err(error) => {
            console_error!("failed to serialize the personal bests: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let statements = vec![
        finish_statement(
            &training_id,
            &authenticated.id,
            &terminal_id,
            &finished_at,
            &summary,
            &personal_bests,
        ),
        select_summary_statement(&training_id, &authenticated.id),
    ];
    let results = match db.batch(&statements).await {
        Ok(results) => results,
        Err(error) => {
            console_error!("failed to save the summary: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let saved = match results.get(1) {
        Some(result) => result.results::<SessionSummaryRow>()?.into_iter().next(),
        None => None,
    };
    let Some(saved) = saved else {
        console_error!("the summary select returned no row");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    summary_response(&db, &saved, &training_id, &authenticated.id).await
}

/// `GET /api/trainings/:id/summary`
pub async fn summary(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(session_row) = load_session(&db, &training_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if session_row.finished_at.is_none() {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    summary_response(&db, &session_row, &training_id, &authenticated.id).await
}

fn current_seconds() -> i64 {
    (Date::now().as_millis() / 1_000) as i64
}

async fn load_session(
    db: &Db,
    training_id: &str,
    user_id: &str,
) -> Result<Option<SessionSummaryRow>> {
    db.first(&select_summary_statement(training_id, user_id))
        .await
}

fn select_summary_statement(training_id: &str, user_id: &str) -> SqlStatement {
    SqlStatement::new(
        format!(
            "SELECT {SESSION_SUMMARY_COLUMNS} FROM training_sessions WHERE id = ? AND user_id = ?"
        ),
        vec![
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

fn finish_statement(
    training_id: &str,
    user_id: &str,
    terminal_id: &str,
    finished_at: &str,
    summary: &TrainingSessionSummary,
    personal_bests: &str,
) -> SqlStatement {
    SqlStatement::new(
        "UPDATE training_sessions SET finished_at = ?, duration_seconds = ?, \
         total_weight_kg = ?, exercise_count = ?, set_count = ?, personal_bests = ?, \
         owner_terminal_id = NULL \
         WHERE id = ? AND user_id = ? AND finished_at IS NULL \
         AND (owner_terminal_id IS NULL OR owner_terminal_id = ?)",
        vec![
            SqlValue::Text(finished_at.to_owned()),
            SqlValue::Integer(summary.duration_seconds),
            SqlValue::Real(summary.total_weight_kg),
            SqlValue::Integer(i64::from(summary.exercise_count)),
            SqlValue::Integer(i64::from(summary.set_count)),
            SqlValue::Text(personal_bests.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(terminal_id.to_owned()),
        ],
    )
}

async fn summary_response(
    db: &Db,
    row: &SessionSummaryRow,
    training_id: &str,
    user_id: &str,
) -> Result<Response> {
    let Some(summary) = stored_summary(row) else {
        console_error!("the stored summary is incomplete");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    let finished_at = row.finished_at.as_deref().unwrap_or_default();
    let past = past_personal_bests(db, user_id, training_id, finished_at).await?;
    let personal_bests = updated_personal_bests(&summary.personal_bests, &past);
    Response::from_json(&TrainingSessionSummary {
        duration_seconds: summary.duration_seconds,
        total_weight_kg: summary.total_weight_kg,
        exercise_count: summary.exercise_count,
        set_count: summary.set_count,
        personal_bests,
    })
}

fn stored_summary(row: &SessionSummaryRow) -> Option<TrainingSessionSummary> {
    Some(TrainingSessionSummary {
        duration_seconds: row.duration_seconds?,
        total_weight_kg: row.total_weight_kg?,
        exercise_count: u32::try_from(row.exercise_count?).ok()?,
        set_count: u32::try_from(row.set_count?).ok()?,
        personal_bests: serde_json::from_str::<Vec<PersonalBestItem>>(
            row.personal_bests.as_deref()?,
        )
        .ok()?,
    })
}

async fn past_personal_bests(
    db: &Db,
    user_id: &str,
    training_id: &str,
    finished_at: &str,
) -> Result<Vec<PersonalBestItem>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT personal_bests FROM training_sessions \
             WHERE user_id = ? AND id <> ? AND finished_at IS NOT NULL AND finished_at < ? \
             AND personal_bests IS NOT NULL",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(finished_at.to_owned()),
            ],
        ))
        .await?;
    let rows = result.results::<PersonalBestsRow>()?;
    let mut items = Vec::new();
    for row in rows {
        let Some(personal_bests) = row.personal_bests else {
            continue;
        };
        let stored =
            serde_json::from_str::<Vec<PersonalBestItem>>(&personal_bests).map_err(|error| {
                worker::Error::RustError(format!("the stored personal bests are invalid: {error}"))
            })?;
        items.extend(stored);
    }
    Ok(items)
}

async fn load_sets(db: &Db, training_id: &str, user_id: &str) -> Result<Vec<SessionSet>> {
    let result = db
        .all(&SqlStatement::new(
            SET_SEGMENT_QUERY,
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?;
    let rows = result.results::<SetSegmentRow>()?;
    build_sets(rows).map_err(|error| {
        worker::Error::RustError(format!("the stored sets are invalid: {error:?}"))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildSetsError {
    /// 種目のスナップショットを復元できない。
    UnreadableSnapshot,
    /// セット種別の文字列が既知の値でない。
    UnknownSetKind,
    /// グリップの JSON を復元できない。
    InvalidGrip,
    /// 完了したセットに段が無い。
    MissingSegments,
    /// セットが計算規則の検証を通らない。
    InvalidSetRecord(SetValidationError),
}

struct ExerciseGroup {
    training_exercise_id: String,
    snapshot: ExerciseSnapshot,
    sets: Vec<StoredSet>,
}

struct StoredSet {
    id: String,
    kind: String,
    rpe: Option<f64>,
    grip: Option<String>,
    segments: Vec<SetSegment>,
}

fn build_sets(rows: Vec<SetSegmentRow>) -> Result<Vec<SessionSet>, BuildSetsError> {
    let mut groups = Vec::<ExerciseGroup>::new();
    for row in rows {
        let index = match groups
            .iter()
            .position(|group| group.training_exercise_id == row.training_exercise_id)
        {
            Some(index) => index,
            None => {
                let snapshot = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot)
                    .map_err(|_| BuildSetsError::UnreadableSnapshot)?;
                groups.push(ExerciseGroup {
                    training_exercise_id: row.training_exercise_id.clone(),
                    snapshot,
                    sets: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        let segment = row
            .segment_id
            .as_ref()
            .map(|_| segment_from_row(&row, group.snapshot.laterality));
        match group.sets.iter_mut().find(|set| set.id == row.set_id) {
            Some(set) => {
                if let Some(segment) = segment {
                    set.segments.push(segment);
                }
            }
            None => {
                let mut segments = Vec::new();
                if let Some(segment) = segment {
                    segments.push(segment);
                }
                group.sets.push(StoredSet {
                    id: row.set_id.clone(),
                    kind: row.kind.clone(),
                    rpe: row.set_rpe,
                    grip: row.grip.clone(),
                    segments,
                });
            }
        }
    }

    let mut sets = Vec::new();
    for group in groups {
        for set in group.sets {
            if set.segments.is_empty() {
                return Err(BuildSetsError::MissingSegments);
            }
            let grip = parse_grip(set.grip.as_deref())?;
            let (grip, attachment) = grip.comparison_key();
            let key = BestKey {
                exercise_id: group.snapshot.exercise_id.clone(),
                grip,
                attachment,
            };
            let kind = parse_set_kind(&set.kind).ok_or(BuildSetsError::UnknownSetKind)?;
            let record = SetRecord::new(
                key,
                kind,
                set.segments,
                set.rpe,
                None,
                group.snapshot.record_mode,
            )
            .map_err(BuildSetsError::InvalidSetRecord)?;
            sets.push(SessionSet {
                record,
                exercise_name: group.snapshot.name.clone(),
                laterality: group.snapshot.laterality,
            });
        }
    }
    Ok(sets)
}

fn parse_grip(text: Option<&str>) -> Result<GripSelection, BuildSetsError> {
    match text {
        None => Ok(GripSelection::None),
        Some(text) => serde_json::from_str(text).map_err(|_| BuildSetsError::InvalidGrip),
    }
}

fn parse_set_kind(text: &str) -> Option<SetKind> {
    match text {
        "normal" => Some(SetKind::Normal),
        "warmup" => Some(SetKind::Warmup),
        "drop" => Some(SetKind::Drop),
        _ => None,
    }
}

fn segment_from_row(row: &SetSegmentRow, laterality: Laterality) -> SetSegment {
    match laterality {
        Laterality::Unilateral => SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: to_u32(row.momentum_reps),
            assisted_reps: to_u32(row.assisted_reps),
            left: Some(side_value(
                row.left_weight_kg,
                row.left_reps,
                row.left_duration_seconds,
            )),
            right: Some(side_value(
                row.right_weight_kg,
                row.right_reps,
                row.right_duration_seconds,
            )),
            duration_seconds: None,
        },
        Laterality::Bilateral => SetSegment {
            weight_kg: row.weight_kg.unwrap_or(0.0),
            reps: to_u32(row.reps),
            momentum_reps: to_u32(row.momentum_reps),
            assisted_reps: to_u32(row.assisted_reps),
            left: None,
            right: None,
            duration_seconds: row
                .duration_seconds
                .and_then(|value| u32::try_from(value).ok()),
        },
    }
}

fn side_value(
    weight_kg: Option<f64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
) -> SideValue {
    SideValue {
        weight_kg: weight_kg.unwrap_or(0.0),
        reps: to_u32(reps),
        duration_seconds: duration_seconds.and_then(|value| u32::try_from(value).ok()),
    }
}

fn to_u32(value: Option<i64>) -> u32 {
    value
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use warrior_core::exercise::Laterality;
    use warrior_core::set_calc::{RecordMode, SetKind};
    use warrior_core::training_session::{BestKind, TrainingSessionSummary};

    use super::{
        build_sets, finish_statement, stored_summary, BuildSetsError, SessionSummaryRow,
        SetSegmentRow,
    };

    #[test]
    fn the_finish_batch_scopes_the_update_to_the_user_the_owner_and_an_unfinished_session() {
        let statement = finish_statement(
            "3f2504e0-4f89-41d3-9a0c-0305e82c6601",
            "3f2504e0-4f89-41d3-9a0c-0305e82c6602",
            "3f2504e0-4f89-41d3-9a0c-0305e82c6603",
            "2026-11-12T04:00:00Z",
            &TrainingSessionSummary {
                duration_seconds: 3_600,
                total_weight_kg: 1_640.0,
                exercise_count: 2,
                set_count: 3,
                personal_bests: Vec::new(),
            },
            "[]",
        );
        assert!(statement.sql().contains("owner_terminal_id = NULL"));
        assert!(statement
            .sql()
            .contains("WHERE id = ? AND user_id = ? AND finished_at IS NULL"));
        assert!(statement
            .sql()
            .contains("(owner_terminal_id IS NULL OR owner_terminal_id = ?)"));
        assert_eq!(statement.bindings().len(), 9);
        assert_eq!(
            statement.bindings()[6],
            warrior_core::sql::SqlValue::Text("3f2504e0-4f89-41d3-9a0c-0305e82c6601".to_owned())
        );
        assert_eq!(
            statement.bindings()[8],
            warrior_core::sql::SqlValue::Text("3f2504e0-4f89-41d3-9a0c-0305e82c6603".to_owned())
        );
    }

    #[test]
    fn a_stored_summary_round_trips_through_the_row() {
        let row = SessionSummaryRow {
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            finished_at: Some("2026-11-12T04:00:00Z".to_owned()),
            owner_terminal_id: None,
            duration_seconds: Some(3_600),
            total_weight_kg: Some(1_640.0),
            exercise_count: Some(2),
            set_count: Some(3),
            personal_bests: Some(
                r#"[{"exercise_id":"bench","exercise_name":"ベンチプレス","record_mode":"weight_and_reps","laterality":"bilateral","grip":null,"attachment":null,"kind":"max_weight","value":100.0}]"#
                    .to_owned(),
            ),
        };
        let summary = stored_summary(&row).expect("a stored summary");
        assert_eq!(summary.duration_seconds, 3_600);
        assert_eq!(summary.total_weight_kg, 1_640.0);
        assert_eq!(summary.exercise_count, 2);
        assert_eq!(summary.set_count, 3);
        assert_eq!(summary.personal_bests[0].kind, BestKind::MaxWeight);
        assert_eq!(summary.personal_bests[0].value, 100.0);
    }

    #[test]
    fn an_unfinished_summary_row_has_no_summary() {
        let row = SessionSummaryRow {
            date: "2026-11-12".to_owned(),
            started_at: Some("2026-11-12T03:00:00Z".to_owned()),
            finished_at: None,
            owner_terminal_id: None,
            duration_seconds: None,
            total_weight_kg: None,
            exercise_count: None,
            set_count: None,
            personal_bests: None,
        };
        assert_eq!(stored_summary(&row), None);
    }

    #[test]
    fn the_stored_rows_become_the_weight_and_time_sets() {
        let rows = vec![
            segment_row(
                "exercise-weight",
                "set-warmup",
                "warmup",
                Some(60.0),
                Some(10),
                None,
            ),
            segment_row(
                "exercise-weight",
                "set-normal",
                "normal",
                Some(100.0),
                Some(5),
                None,
            ),
            segment_row(
                "exercise-weight",
                "set-drop",
                "drop",
                Some(100.0),
                Some(8),
                None,
            ),
            segment_row(
                "exercise-weight",
                "set-drop",
                "drop",
                Some(80.0),
                Some(6),
                None,
            ),
            segment_row("exercise-time", "set-time", "normal", None, None, Some(90)),
        ];
        let sets = build_sets(rows).expect("the stored sets are valid");
        assert_eq!(sets.len(), 4);
        assert_eq!(sets[0].record.kind(), SetKind::Warmup);
        assert_eq!(sets[0].record.total_weight_kg(), 0.0);
        assert_eq!(sets[1].record.mode(), RecordMode::WeightAndReps);
        assert_eq!(sets[1].record.total_weight_kg(), 500.0);
        assert_eq!(sets[2].record.kind(), SetKind::Drop);
        assert_eq!(sets[2].record.segments().len(), 2);
        assert_eq!(sets[2].record.total_weight_kg(), 1_280.0);
        assert_eq!(sets[3].record.mode(), RecordMode::Time);
        assert_eq!(sets[3].record.longest_duration_seconds(), Some(90));
        assert_eq!(sets[1].exercise_name, "exercise-weight");
        assert_eq!(sets[1].laterality, Laterality::Bilateral);
    }

    #[test]
    fn an_unknown_set_kind_is_rejected() {
        let rows = vec![segment_row(
            "exercise-weight",
            "set-normal",
            "unknown",
            Some(100.0),
            Some(5),
            None,
        )];
        assert_eq!(build_sets(rows), Err(BuildSetsError::UnknownSetKind));
    }

    #[test]
    fn a_completed_set_without_segments_is_rejected() {
        let mut row = segment_row(
            "exercise-weight",
            "set-normal",
            "normal",
            Some(100.0),
            Some(5),
            None,
        );
        row.segment_id = None;
        assert_eq!(build_sets(vec![row]), Err(BuildSetsError::MissingSegments));
    }

    #[test]
    fn an_invalid_grip_json_is_rejected() {
        let mut row = segment_row(
            "exercise-weight",
            "set-normal",
            "normal",
            Some(100.0),
            Some(5),
            None,
        );
        row.grip = Some("not-json".to_owned());
        assert_eq!(build_sets(vec![row]), Err(BuildSetsError::InvalidGrip));
    }

    fn segment_row(
        exercise_id: &str,
        set_id: &str,
        kind: &str,
        weight_kg: Option<f64>,
        reps: Option<i64>,
        duration_seconds: Option<i64>,
    ) -> SetSegmentRow {
        let record_mode = if duration_seconds.is_some() {
            "time"
        } else {
            "weight_and_reps"
        };
        let snapshot = json!({
            "exercise_id": exercise_id,
            "name": exercise_id,
            "body_part": "chest",
            "muscles": [],
            "record_mode": record_mode,
            "laterality": "bilateral",
            "grip_mode": "none",
            "grip_options": []
        });
        SetSegmentRow {
            training_exercise_id: exercise_id.to_owned(),
            snapshot: snapshot.to_string(),
            set_id: set_id.to_owned(),
            kind: kind.to_owned(),
            set_rpe: None,
            grip: None,
            segment_id: Some(format!("{set_id}-segment")),
            weight_kg,
            reps,
            momentum_reps: Some(0),
            assisted_reps: Some(0),
            duration_seconds,
            left_weight_kg: None,
            left_reps: None,
            left_duration_seconds: None,
            right_weight_kg: None,
            right_reps: None,
            right_duration_seconds: None,
        }
    }
}
