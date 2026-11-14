//! トレーニングの種目の追加、並べ替え、削除と、前回の記録の取得。
//!
//! 対応する要求: `REQ-REC-002`、`REQ-REC-003`、`REQ-REC-004`。
//! 種目の操作は端末の送信待ちの列から届き、`backend/warrior/src/sync.rs` が編集権を
//! 検査してからこのモジュールの文を同じ `batch()` で適用する (ADR-002、ADR-003)。
//! 前回の選択は `warrior_core::previous` の純粋な関数が行い、このモジュールは D1 の
//! 候補を読んで応答を組み立てる。前回の規則は PRD の「計算ルールと用語定義」の「前回」に従う。

use std::collections::HashMap;

use serde::Deserialize;
use warrior_core::exercise::GripSelection;
use warrior_core::previous::{
    select_exercise_previous, select_training_previous, PreviousExercise,
    PreviousExerciseCandidate, PreviousExerciseKey, PreviousResponse, PreviousSet, PreviousSetKey,
    PreviousTraining, PreviousTrainingCandidate, PreviousTrainingSource,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{
    TrainingExerciseCreatePayload, TrainingExerciseDeletePayload, TrainingExerciseReorderPayload,
};
use warrior_core::training_session::SessionSource;
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::session;
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::stored_sets;

/// 種目のデータ文に付ける編集権の条件。
///
/// 先頭のガード文が 0 行でも後続の文が実行されるため、各データ文でも同じ条件を確かめる。
/// `Claim` のあとはガード文が所有者を設定しているので、`Terminal` として扱う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseOwnership<'a> {
    /// この端末が記録中のトレーニングの所有者である。
    Terminal(&'a str),
    /// 終了したトレーニングである。
    Finished,
}

pub(crate) fn ownership_condition(
    user_id: &str,
    training_id: &str,
    ownership: ExerciseOwnership<'_>,
) -> (String, Vec<SqlValue>) {
    match ownership {
        ExerciseOwnership::Terminal(terminal_id) => (
            "EXISTS (SELECT 1 FROM training_sessions ts WHERE ts.id = ? AND ts.user_id = ? \
             AND (ts.finished_at IS NOT NULL OR ts.owner_terminal_id = ?))"
                .to_owned(),
            vec![
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(terminal_id.to_owned()),
            ],
        ),
        ExerciseOwnership::Finished => (
            "EXISTS (SELECT 1 FROM training_sessions ts WHERE ts.id = ? AND ts.user_id = ? \
             AND ts.finished_at IS NOT NULL)"
                .to_owned(),
            vec![
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ),
    }
}

/// 前回の取得で読む、現在のトレーニングの開始元。
#[derive(Debug, Deserialize)]
struct CurrentSessionRow {
    routine_id: Option<String>,
    program_day_id: Option<String>,
    custom_program_day_id: Option<String>,
}

/// 種目ごとの前回の候補を組み立てる、完了したセットの行。
#[derive(Debug, Deserialize)]
struct ExerciseCandidateSetRow {
    training_id: String,
    finished_at: String,
    exercise_id: String,
    grip: Option<String>,
}

/// トレーニング全体の前回の候補の行。
#[derive(Debug, Deserialize)]
struct TrainingCandidateRow {
    training_id: String,
    finished_at: String,
    source_kind: String,
    routine_id: Option<String>,
    program_day_id: Option<String>,
    custom_program_day_id: Option<String>,
    total_weight_kg: Option<f64>,
}

/// `GET /api/trainings/:id/previous`
///
/// 種目ごとの前回とトレーニング全体の前回を返す。前回が無い部分は `null` にする。
/// `exercise_id` を省略すると、トレーニング全体の前回だけを返す。`grip` は現在の
/// グリップの選択の JSON、`attachment` はアタッチメントの比較のキーである。
pub async fn previous(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(training_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let query = request
        .url()?
        .query_pairs()
        .into_owned()
        .collect::<HashMap<_, _>>();
    let exercise_id = query
        .get("exercise_id")
        .filter(|text| !text.is_empty())
        .cloned();
    let key = if exercise_id.is_some() {
        let Some(key) = previous_key(
            query.get("grip").map(String::as_str),
            query.get("attachment").map(String::as_str),
        ) else {
            return Ok(response(
                ErrorCode::BadRequest,
                "the grip or attachment is invalid",
            ));
        };
        key
    } else {
        (None, None)
    };
    let db = Db::new(env.d1("DB")?);
    let Some(current) = load_current_session(&db, &training_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    match build_previous(
        &db,
        &authenticated.id,
        &training_id,
        &current,
        exercise_id.as_deref(),
        key,
    )
    .await
    {
        Ok(body) => Response::from_json(&body),
        Err(error) => {
            console_error!("failed to build the previous record: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}

/// グリップの JSON とアタッチメントから、前回を選ぶ比較のキーを作る。
///
/// グリップの JSON は `sets.grip` と同じ `GripSelection` の形である。アタッチメントを
/// 併せて指定した場合は、グリップの選択から作った値と一致することを求める。
fn previous_key(
    grip: Option<&str>,
    attachment: Option<&str>,
) -> Option<(Option<String>, Option<String>)> {
    match grip {
        None => Some((None, attachment.map(str::to_owned))),
        Some(text) => {
            let selection = serde_json::from_str::<GripSelection>(text).ok()?;
            let (grip_key, attachment_key) = selection.comparison_key();
            if let Some(attachment) = attachment {
                if attachment_key.as_deref() != Some(attachment) {
                    return None;
                }
            }
            Some((grip_key, attachment_key))
        }
    }
}

async fn load_current_session(
    db: &Db,
    training_id: &str,
    user_id: &str,
) -> Result<Option<CurrentSessionRow>> {
    db.first(&SqlStatement::new(
        "SELECT routine_id, program_day_id, custom_program_day_id \
         FROM training_sessions WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

async fn build_previous(
    db: &Db,
    user_id: &str,
    training_id: &str,
    current: &CurrentSessionRow,
    exercise_id: Option<&str>,
    key: (Option<String>, Option<String>),
) -> Result<PreviousResponse> {
    let exercise = match exercise_id {
        Some(exercise_id) => {
            load_exercise_previous(db, user_id, training_id, exercise_id, key).await?
        }
        None => None,
    };
    let training = load_training_previous(db, user_id, training_id, current).await?;
    Ok(PreviousResponse { exercise, training })
}

async fn load_exercise_previous(
    db: &Db,
    user_id: &str,
    training_id: &str,
    exercise_id: &str,
    key: (Option<String>, Option<String>),
) -> Result<Option<PreviousExercise>> {
    let rows = db
        .all(&exercise_candidates_statement(
            user_id,
            training_id,
            exercise_id,
        ))
        .await?
        .results::<ExerciseCandidateSetRow>()?;
    let candidates = exercise_candidates(rows)?;
    let key = PreviousExerciseKey {
        exercise_id: exercise_id.to_owned(),
        grip: key.0,
        attachment: key.1,
    };
    let Some(selected) = select_exercise_previous(&candidates, &key) else {
        return Ok(None);
    };
    let exercises = stored_sets::load_training(db, user_id, &selected.training_id).await?;
    Ok(previous_exercise_from(&exercises, &key))
}

fn exercise_candidates(
    rows: Vec<ExerciseCandidateSetRow>,
) -> Result<Vec<PreviousExerciseCandidate>> {
    let mut candidates = Vec::<PreviousExerciseCandidate>::new();
    for row in rows {
        let selection = match row.grip {
            None => GripSelection::None,
            Some(text) => serde_json::from_str::<GripSelection>(&text).map_err(|error| {
                worker::Error::RustError(format!("the stored grip is invalid: {error}"))
            })?,
        };
        let (grip, attachment) = selection.comparison_key();
        let key = PreviousSetKey {
            exercise_id: row.exercise_id,
            grip,
            attachment,
        };
        match candidates
            .iter_mut()
            .find(|candidate| candidate.training_id == row.training_id)
        {
            Some(candidate) => {
                if !candidate.sets.contains(&key) {
                    candidate.sets.push(key);
                }
            }
            None => candidates.push(PreviousExerciseCandidate {
                training_id: row.training_id,
                finished_at: row.finished_at,
                sets: vec![key],
            }),
        }
    }
    Ok(candidates)
}

fn previous_exercise_from(
    exercises: &[stored_sets::StoredExercise],
    key: &PreviousExerciseKey,
) -> Option<PreviousExercise> {
    let mut sets = Vec::new();
    let mut total_weight_kg = 0.0;
    let mut total_duration_seconds = 0;
    let mut max_duration_seconds = 0;
    for exercise in exercises
        .iter()
        .filter(|exercise| exercise.snapshot.exercise_id == key.exercise_id)
    {
        for (index, record) in exercise.sets.iter().enumerate() {
            if record.key().grip != key.grip || record.key().attachment != key.attachment {
                continue;
            }
            total_weight_kg += record.total_weight_kg();
            total_duration_seconds += record.total_duration_seconds();
            max_duration_seconds =
                max_duration_seconds.max(record.longest_duration_seconds().unwrap_or(0));
            sets.push(PreviousSet {
                kind: record.kind(),
                rpe: record.rpe(),
                grip: exercise
                    .set_grips
                    .get(index)
                    .cloned()
                    .flatten()
                    .and_then(|grip| serde_json::from_str::<GripSelection>(&grip).ok()),
                body_weight_kg: exercise.set_body_weights.get(index).copied().flatten(),
                segments: record.segments().to_vec(),
            });
        }
    }
    if sets.is_empty() {
        return None;
    }
    Some(PreviousExercise {
        sets,
        total_weight_kg,
        total_duration_seconds,
        max_duration_seconds,
    })
}

async fn load_training_previous(
    db: &Db,
    user_id: &str,
    training_id: &str,
    current: &CurrentSessionRow,
) -> Result<Option<PreviousTraining>> {
    let rows = db
        .all(&training_candidates_statement(user_id, training_id))
        .await?
        .results::<TrainingCandidateRow>()?;
    let candidates = training_candidates(rows);
    let source = PreviousTrainingSource::new(
        current.routine_id.clone(),
        current.program_day_id.clone(),
        current.custom_program_day_id.clone(),
    );
    Ok(
        select_training_previous(&candidates, &source).map(|candidate| PreviousTraining {
            total_weight_kg: candidate.total_weight_kg,
        }),
    )
}

fn training_candidates(rows: Vec<TrainingCandidateRow>) -> Vec<PreviousTrainingCandidate> {
    // 開始元が不明な行と総重量の無い行は前回に選べないため、候補から外す。
    let mut candidates = Vec::new();
    for row in rows {
        let Some(source_kind) = SessionSource::parse(&row.source_kind) else {
            continue;
        };
        let Some(total_weight_kg) = row.total_weight_kg.filter(|value| value.is_finite()) else {
            continue;
        };
        candidates.push(PreviousTrainingCandidate {
            training_id: row.training_id,
            finished_at: row.finished_at,
            source_kind,
            routine_id: row.routine_id,
            program_day_id: row.program_day_id,
            custom_program_day_id: row.custom_program_day_id,
            total_weight_kg,
        });
    }
    candidates
}

fn exercise_candidates_statement(
    user_id: &str,
    training_id: &str,
    exercise_id: &str,
) -> SqlStatement {
    SqlStatement::new(
        "SELECT ts.id AS training_id, ts.finished_at, te.exercise_id, s.grip \
         FROM training_sessions ts \
         JOIN training_exercises te ON te.training_id = ts.id AND te.user_id = ts.user_id \
         JOIN sets s ON s.training_exercise_id = te.id AND s.user_id = ts.user_id \
          AND s.completed = 1 \
         WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL AND ts.id <> ? \
          AND te.exercise_id = ? \
         ORDER BY ts.finished_at DESC, ts.id DESC, s.position, s.id",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(exercise_id.to_owned()),
        ],
    )
}

fn training_candidates_statement(user_id: &str, training_id: &str) -> SqlStatement {
    SqlStatement::new(
        "SELECT id AS training_id, finished_at, source_kind, routine_id, program_day_id, \
         custom_program_day_id, total_weight_kg \
         FROM training_sessions \
         WHERE user_id = ? AND finished_at IS NOT NULL AND id <> ? \
         ORDER BY finished_at DESC, id DESC",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(training_id.to_owned()),
        ],
    )
}

/// 種目の追加の文。呼び出し元が編集権の確認と操作 ID の保存の間に置く。
pub fn create_statements(
    user_id: &str,
    training_id: &str,
    payload: &TrainingExerciseCreatePayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Result<Vec<SqlStatement>> {
    let snapshot = serde_json::to_string(&payload.snapshot)
        .map_err(|error| worker::Error::RustError(format!("the snapshot is invalid: {error}")))?;
    let (condition, mut bindings) = ownership_condition(user_id, training_id, ownership);
    let mut values = vec![
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(payload.exercise_id.clone()),
        SqlValue::Text(snapshot),
        SqlValue::Integer(payload.position),
        SqlValue::Text(now.to_owned()),
        SqlValue::Text(now.to_owned()),
    ];
    values.append(&mut bindings);
    Ok(vec![SqlStatement::new(
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             SELECT ?, ?, ?, ?, ?, ?, ?, ? WHERE {condition} \
             ON CONFLICT(id) DO UPDATE SET exercise_id = excluded.exercise_id, \
             snapshot = excluded.snapshot, position = excluded.position, \
             updated_at = excluded.updated_at \
             WHERE training_exercises.user_id = excluded.user_id \
             AND training_exercises.training_id = excluded.training_id"
        ),
        values,
    )])
}

/// 種目の削除の文。セット、段、スーパーセットの参照も消す。
pub fn delete_statements(
    user_id: &str,
    training_id: &str,
    payload: &TrainingExerciseDeletePayload,
    ownership: ExerciseOwnership<'_>,
) -> Vec<SqlStatement> {
    let (condition, condition_bindings) = ownership_condition(user_id, training_id, ownership);
    let mut segments = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    segments.extend(condition_bindings.iter().cloned());
    let mut sets = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    sets.extend(condition_bindings.iter().cloned());
    let mut members = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Text(training_id.to_owned()),
        SqlValue::Text(user_id.to_owned()),
    ];
    members.extend(condition_bindings.iter().cloned());
    let mut supersets = vec![
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    supersets.extend(condition_bindings.iter().cloned());
    let mut exercise = vec![
        SqlValue::Text(payload.training_exercise_id.clone()),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    exercise.extend(condition_bindings);
    vec![
        SqlStatement::new(
            format!(
                "DELETE FROM set_segments WHERE user_id = ? AND set_id IN \
                 (SELECT s.id FROM sets s JOIN training_exercises te \
                  ON te.id = s.training_exercise_id \
                  WHERE te.id = ? AND te.training_id = ? AND te.user_id = ?) \
                 AND {condition}"
            ),
            segments,
        ),
        SqlStatement::new(
            format!(
                "DELETE FROM sets WHERE user_id = ? AND training_exercise_id IN \
                 (SELECT id FROM training_exercises WHERE id = ? AND training_id = ? \
                  AND user_id = ?) AND {condition}"
            ),
            sets,
        ),
        SqlStatement::new(
            format!(
                "DELETE FROM superset_members WHERE user_id = ? AND training_exercise_id IN \
                 (SELECT id FROM training_exercises WHERE id = ? AND training_id = ? \
                  AND user_id = ?) AND {condition}"
            ),
            members,
        ),
        SqlStatement::new(
            format!(
                "DELETE FROM supersets WHERE user_id = ? AND training_id = ? \
                 AND (SELECT COUNT(*) FROM superset_members sm \
                  WHERE sm.superset_id = supersets.id AND sm.user_id = supersets.user_id) < 2 \
                 AND {condition}"
            ),
            supersets,
        ),
        SqlStatement::new(
            format!(
                "DELETE FROM training_exercises WHERE id = ? AND user_id = ? AND training_id = ? \
                 AND {condition}"
            ),
            exercise,
        ),
    ]
}

/// 種目の並べ替えの文。指定した順に 1 から位置を振る。
pub fn reorder_statements(
    user_id: &str,
    training_id: &str,
    payload: &TrainingExerciseReorderPayload,
    ownership: ExerciseOwnership<'_>,
    now: &str,
) -> Vec<SqlStatement> {
    let (condition, condition_bindings) = ownership_condition(user_id, training_id, ownership);
    payload
        .ordered_ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let mut bindings = vec![
                SqlValue::Integer(index as i64 + 1),
                SqlValue::Text(now.to_owned()),
                SqlValue::Text(id.clone()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ];
            bindings.extend(condition_bindings.iter().cloned());
            SqlStatement::new(
                format!(
                    "UPDATE training_exercises SET position = ?, updated_at = ? \
                     WHERE id = ? AND user_id = ? AND training_id = ? AND {condition}"
                ),
                bindings,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use warrior_core::previous::PreviousExerciseKey;
    use warrior_core::sql::SqlValue;
    use warrior_core::sync::{
        TrainingExerciseCreatePayload, TrainingExerciseDeletePayload,
        TrainingExerciseReorderPayload,
    };
    use warrior_core::training_session::SessionSource;

    use super::{
        create_statements, delete_statements, exercise_candidates, previous_exercise_from,
        previous_key, reorder_statements, training_candidates, ExerciseCandidateSetRow,
        ExerciseOwnership, TrainingCandidateRow,
    };
    use crate::stored_sets::StoredExercise;
    use warrior_core::exercise::{BodyPart, ExerciseSnapshot, Laterality};
    use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};

    const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4801";
    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4802";
    const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4803";
    const ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4804";
    const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4805";
    const NOW: &str = "2026-11-12T00:00:00Z";

    fn create_payload() -> TrainingExerciseCreatePayload {
        TrainingExerciseCreatePayload {
            training_id: TRAINING_ID.to_owned(),
            training_exercise_id: ROW_ID.to_owned(),
            exercise_id: EXERCISE_ID.to_owned(),
            snapshot: ExerciseSnapshot {
                exercise_id: EXERCISE_ID.to_owned(),
                name: "ベンチプレス".to_owned(),
                body_part: BodyPart::Chest,
                muscles: Vec::new(),
                record_mode: RecordMode::WeightAndReps,
                laterality: Laterality::Bilateral,
                grip_mode: warrior_core::exercise::GripMode::None,
                grip_options: Vec::new(),
            },
            position: 1,
        }
    }

    #[test]
    fn the_create_statement_is_scoped_to_the_user_and_the_training() {
        let statements = create_statements(
            USER_ID,
            TRAINING_ID,
            &create_payload(),
            ExerciseOwnership::Terminal(TERMINAL_ID),
            NOW,
        )
        .expect("statements");
        assert_eq!(statements.len(), 1);
        let sql = statements[0].sql();
        assert!(sql.contains("WHERE EXISTS (SELECT 1 FROM training_sessions"));
        assert!(sql.contains("ON CONFLICT(id) DO UPDATE"));
        assert!(sql.contains("training_exercises.user_id = excluded.user_id"));
        assert!(sql.contains("training_exercises.training_id = excluded.training_id"));
        assert_eq!(
            statements[0].bindings()[0],
            SqlValue::Text(ROW_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[1],
            SqlValue::Text(USER_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[2],
            SqlValue::Text(TRAINING_ID.to_owned())
        );
        assert_eq!(statements[0].bindings()[5], SqlValue::Integer(1));
        assert_eq!(
            statements[0].bindings()[8],
            SqlValue::Text(TRAINING_ID.to_owned())
        );
        assert_eq!(
            statements[0].bindings()[10],
            SqlValue::Text(TERMINAL_ID.to_owned())
        );
        match &statements[0].bindings()[4] {
            SqlValue::Text(text) => assert!(text.contains("ベンチプレス")),
            other => panic!("the snapshot must be text: {other:?}"),
        }
    }

    #[test]
    fn the_delete_statements_remove_the_sets_the_segments_and_the_members() {
        let statements = delete_statements(
            USER_ID,
            TRAINING_ID,
            &TrainingExerciseDeletePayload {
                training_id: TRAINING_ID.to_owned(),
                training_exercise_id: ROW_ID.to_owned(),
            },
            ExerciseOwnership::Finished,
        );
        assert_eq!(statements.len(), 5);
        for statement in &statements {
            assert!(statement
                .sql()
                .contains("EXISTS (SELECT 1 FROM training_sessions"));
        }
        assert!(statements[0].sql().contains("set_segments"));
        assert!(statements[0].sql().contains("te.training_id = ?"));
        assert!(statements[1].sql().contains("DELETE FROM sets"));
        assert!(statements[1].sql().contains("training_id = ?"));
        assert!(statements[2].sql().contains("superset_members"));
        assert!(statements[2].sql().contains("training_id = ?"));
        assert!(statements[3].sql().contains("DELETE FROM supersets"));
        assert!(statements[3]
            .sql()
            .contains("sm.superset_id = supersets.id"));
        assert!(statements[3].sql().contains(") < 2"));
        assert!(statements[4]
            .sql()
            .contains("DELETE FROM training_exercises"));
        assert!(statements[4].sql().contains("training_id = ?"));
        assert_eq!(
            statements[4].bindings()[2],
            SqlValue::Text(TRAINING_ID.to_owned())
        );
    }

    #[test]
    fn the_reorder_statements_number_the_positions_from_one() {
        let statements = reorder_statements(
            USER_ID,
            TRAINING_ID,
            &TrainingExerciseReorderPayload {
                training_id: TRAINING_ID.to_owned(),
                ordered_ids: vec![ROW_ID.to_owned(), EXERCISE_ID.to_owned()],
            },
            ExerciseOwnership::Terminal(TERMINAL_ID),
            NOW,
        );
        assert_eq!(statements.len(), 2);
        for statement in &statements {
            assert!(statement
                .sql()
                .contains("EXISTS (SELECT 1 FROM training_sessions"));
        }
        assert_eq!(statements[0].bindings()[0], SqlValue::Integer(1));
        assert_eq!(statements[1].bindings()[0], SqlValue::Integer(2));
        assert_eq!(
            statements[0].bindings()[2],
            SqlValue::Text(ROW_ID.to_owned())
        );
        assert_eq!(
            statements[1].bindings()[2],
            SqlValue::Text(EXERCISE_ID.to_owned())
        );
        assert!(statements[0].sql().contains("training_id = ?"));
    }

    #[test]
    fn the_grip_key_is_built_from_the_selection() {
        assert_eq!(previous_key(None, None), Some((None, None)));
        assert_eq!(
            previous_key(
                Some(r#"{"grip":{"style":"pronated","width":"medium"}}"#),
                None,
            ),
            Some((Some("pronated_medium".to_owned()), None))
        );
        assert_eq!(
            previous_key(
                Some(r#"{"attachment":{"attachment":"wide_bar","style":"supinated"}}"#),
                Some("wide_bar"),
            ),
            Some((Some("supinated".to_owned()), Some("wide_bar".to_owned())))
        );
        assert_eq!(
            previous_key(
                Some(r#"{"grip":{"style":"pronated","width":"medium"}}"#),
                Some("wide_bar"),
            ),
            None
        );
        assert_eq!(previous_key(Some("not-json"), None), None);
    }

    #[test]
    fn the_exercise_candidates_group_the_sets_of_a_training() {
        let rows = vec![
            ExerciseCandidateSetRow {
                training_id: "training-a".to_owned(),
                finished_at: "2026-11-12T00:00:00Z".to_owned(),
                exercise_id: EXERCISE_ID.to_owned(),
                grip: None,
            },
            ExerciseCandidateSetRow {
                training_id: "training-a".to_owned(),
                finished_at: "2026-11-12T00:00:00Z".to_owned(),
                exercise_id: EXERCISE_ID.to_owned(),
                grip: Some(r#"{"grip":{"style":"pronated","width":"medium"}}"#.to_owned()),
            },
        ];
        let candidates = exercise_candidates(rows).expect("candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].sets.len(), 2);
        assert_eq!(
            candidates[0].sets[1].grip.as_deref(),
            Some("pronated_medium")
        );
    }

    #[test]
    fn the_training_candidates_skip_rows_without_a_total_or_a_source() {
        let rows = vec![
            TrainingCandidateRow {
                training_id: "training-a".to_owned(),
                finished_at: "2026-11-12T00:00:00Z".to_owned(),
                source_kind: "empty".to_owned(),
                routine_id: None,
                program_day_id: None,
                custom_program_day_id: None,
                total_weight_kg: Some(900.0),
            },
            TrainingCandidateRow {
                training_id: "training-b".to_owned(),
                finished_at: "2026-11-12T00:00:00Z".to_owned(),
                source_kind: "unknown".to_owned(),
                routine_id: None,
                program_day_id: None,
                custom_program_day_id: None,
                total_weight_kg: Some(900.0),
            },
            TrainingCandidateRow {
                training_id: "training-c".to_owned(),
                finished_at: "2026-11-12T00:00:00Z".to_owned(),
                source_kind: "empty".to_owned(),
                routine_id: None,
                program_day_id: None,
                custom_program_day_id: None,
                total_weight_kg: None,
            },
        ];
        let candidates = training_candidates(rows);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].training_id, "training-a");
        assert_eq!(candidates[0].source_kind, SessionSource::Empty);
    }

    #[test]
    fn the_previous_exercise_uses_the_matching_sets_and_the_set_calc_totals() {
        let key = PreviousExerciseKey {
            exercise_id: EXERCISE_ID.to_owned(),
            grip: None,
            attachment: None,
        };
        let exercises = vec![StoredExercise {
            training_id: "training-a".to_owned(),
            snapshot: ExerciseSnapshot {
                exercise_id: EXERCISE_ID.to_owned(),
                name: "ベンチプレス".to_owned(),
                body_part: BodyPart::Chest,
                muscles: Vec::new(),
                record_mode: RecordMode::WeightAndReps,
                laterality: Laterality::Bilateral,
                grip_mode: warrior_core::exercise::GripMode::None,
                grip_options: Vec::new(),
            },
            sets: vec![
                SetRecord::new(
                    BestKey {
                        exercise_id: EXERCISE_ID.to_owned(),
                        grip: None,
                        attachment: None,
                    },
                    SetKind::Warmup,
                    vec![segment(40.0, 10)],
                    None,
                    None,
                    RecordMode::WeightAndReps,
                )
                .expect("a warmup"),
                SetRecord::new(
                    BestKey {
                        exercise_id: EXERCISE_ID.to_owned(),
                        grip: None,
                        attachment: None,
                    },
                    SetKind::Normal,
                    vec![segment(100.0, 5)],
                    None,
                    None,
                    RecordMode::WeightAndReps,
                )
                .expect("a normal set"),
            ],
            set_grips: vec![None, None],
            set_body_weights: vec![Some(70.0), Some(70.0)],
        }];
        let previous = previous_exercise_from(&exercises, &key).expect("a previous exercise");
        assert_eq!(previous.sets.len(), 2);
        assert_eq!(previous.sets[0].kind, SetKind::Warmup);
        assert_eq!(previous.sets[0].body_weight_kg, Some(70.0));
        assert_eq!(previous.sets[1].body_weight_kg, Some(70.0));
        assert_eq!(previous.total_weight_kg, 500.0);
        assert_eq!(previous.total_duration_seconds, 0);
        assert_eq!(previous.max_duration_seconds, 0);
    }

    fn segment(weight_kg: f64, reps: u32) -> SetSegment {
        SetSegment {
            weight_kg,
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        }
    }
}
