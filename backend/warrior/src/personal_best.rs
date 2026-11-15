//! 自己ベストの D1 の読み書きと、セットの完了の後の判定。
//!
//! 比較と候補の作成は `warrior_core::personal_best` が担う。このモジュールは、保存済みの
//! 自己ベストを読み、更新した候補を `personal_bests` の表へ 1 行ずつ書き、画面へ返す
//! 項目を組み立てる。表の比較の単位は利用者、種目、グリップ、アタッチメント、種類である。
//! グリップとアタッチメントを選ばない記録は空文字で保存する。

use std::collections::HashMap;

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::exercise::{ExerciseSnapshot, GripSelection, Laterality, RecordMode};
use warrior_core::jst::JstDate;
use warrior_core::personal_best::{
    candidates, recompute_groups, reconcile, updated, BestGroup, PersonalBestCandidate,
    PersonalBestEntry, PersonalBestKey, TrainingSets,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::sync::{ValidatedOperation, ValidatedPayload};
use warrior_core::training_session::{BestKind, PersonalBestItem};
use worker::{console_error, Result};

use crate::db::Db;
use crate::stored_sets;

/// セットの完了の後の自己ベストの結果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompletionBests {
    /// この完了で新しく更新した項目。
    pub updated: Vec<PersonalBestItem>,
    /// このトレーニングが保持する項目。再送の応答に使う。
    pub held: Vec<PersonalBestItem>,
}

#[derive(Debug, Deserialize)]
struct TrainingDateRow {
    date: String,
}

#[derive(Debug, Deserialize)]
struct PersonalBestRow {
    exercise_id: String,
    grip: String,
    attachment: String,
    kind: String,
    value: f64,
    achieved_on: String,
    training_id: String,
}

#[derive(Debug, Deserialize)]
struct ExerciseSnapshotRow {
    exercise_id: String,
    snapshot: String,
}

/// セットが使う自己ベストの比較の組。
#[derive(Debug, Deserialize)]
struct SetGroupRow {
    exercise_id: String,
    grip: Option<String>,
}

/// 自己ベストの計算し直しの対象にする組を、変更の前の状態から返す。
///
/// セットの編集は、変更の前のグリップの組と、変更の後のグリップの組の両方を対象にする。
pub async fn groups_before_operation(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
) -> Result<Vec<BestGroup>> {
    let mut groups = match &validated.payload {
        ValidatedPayload::SetUpdate(payload) => {
            let mut groups = set_groups(db, user_id, &payload.set_id, SetScope::Set).await?;
            if let Some(grip) = payload.grip {
                if let Some(exercise_id) = groups.first().map(|group| group.exercise_id.clone()) {
                    let (grip, attachment) = grip.comparison_key();
                    groups.push(BestGroup {
                        exercise_id,
                        grip,
                        attachment,
                    });
                }
            }
            groups
        }
        ValidatedPayload::SetDelete(payload) => {
            set_groups(db, user_id, &payload.set_id, SetScope::Set).await?
        }
        ValidatedPayload::SetComplete(payload) => {
            set_groups(db, user_id, &payload.set_id, SetScope::Set).await?
        }
        ValidatedPayload::ExerciseDelete(payload) => {
            set_groups(
                db,
                user_id,
                &payload.training_exercise_id,
                SetScope::Exercise,
            )
            .await?
        }
        ValidatedPayload::Delete(payload) => {
            set_groups(db, user_id, &payload.training_id, SetScope::Training).await?
        }
        ValidatedPayload::Create(_)
        | ValidatedPayload::Update(_)
        | ValidatedPayload::Backfill(_)
        | ValidatedPayload::ExerciseCreate(_)
        | ValidatedPayload::ExerciseReorder(_)
        | ValidatedPayload::SetCreate(_)
        | ValidatedPayload::SetReorder(_)
        | ValidatedPayload::SupersetCreate(_)
        | ValidatedPayload::SupersetDelete(_)
        | ValidatedPayload::SupersetReorder(_) => Vec::new(),
    };
    groups.dedup();
    Ok(groups)
}

/// 影響する組の自己ベストを、残っている全ての記録から計算し直す。
///
/// 値がなくなった組の行は削除する。組の外の行は変えない。
pub async fn rebuild(
    db: &Db,
    user_id: &str,
    groups: &[BestGroup],
) -> Result<Vec<PersonalBestEntry>> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let mut exercise_ids = Vec::<String>::new();
    for group in groups {
        if !exercise_ids.contains(&group.exercise_id) {
            exercise_ids.push(group.exercise_id.clone());
        }
    }
    let trainings = stored_sets::load_finished_for_exercises(db, user_id, &exercise_ids).await?;
    let mut prepared = Vec::new();
    for training in &trainings {
        let achieved_on = JstDate::parse(&training.date).ok_or_else(|| {
            console_error!("the stored training date is invalid");
            worker::Error::RustError("the stored training date is invalid".to_owned())
        })?;
        let sets = stored_sets::session_sets(&training.exercises);
        prepared.push((training.training_id.as_str(), achieved_on, sets));
    }
    let training_sets = prepared
        .iter()
        .map(|(training_id, achieved_on, sets)| TrainingSets {
            training_id,
            achieved_on: *achieved_on,
            sets,
        })
        .collect::<Vec<_>>();
    let recomputed = recompute_groups(&training_sets, groups);
    let existing = load_entries(db, user_id).await?;
    let rebuilt = reconcile(&existing, &recomputed, groups);
    let now = datetime::format_utc(crate::auth::now_seconds());
    let mut statements = Vec::new();
    for entry in &rebuilt.upserts {
        statements.push(replace_statement(user_id, &now, entry));
    }
    for key in &rebuilt.deletes {
        statements.push(delete_statement(user_id, key));
    }
    if !statements.is_empty() {
        if let Err(error) = db.batch(&statements).await {
            console_error!("failed to rebuild the personal bests: {error}");
            return Err(worker::Error::RustError(
                "failed to rebuild the personal bests".to_owned(),
            ));
        }
    }
    Ok(rebuilt.applied(&existing))
}

/// 組を読むセットの範囲。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetScope {
    /// 1 つのセット。
    Set,
    /// 1 つの種目の完了したセット。
    Exercise,
    /// 1 つのトレーニングの完了したセット。
    Training,
}

async fn set_groups(db: &Db, user_id: &str, id: &str, scope: SetScope) -> Result<Vec<BestGroup>> {
    let (condition, completed) = match scope {
        SetScope::Set => ("s.id = ?", ""),
        SetScope::Exercise => ("te.id = ?", "AND s.completed = 1"),
        SetScope::Training => ("te.training_id = ?", "AND s.completed = 1"),
    };
    let statement = format!(
        "SELECT te.exercise_id AS exercise_id, s.grip AS grip FROM sets s \
         JOIN training_exercises te ON te.id = s.training_exercise_id \
         WHERE s.user_id = ? AND te.user_id = ? AND {condition} {completed}"
    );
    let rows = db
        .all(&SqlStatement::new(
            statement,
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(id.to_owned()),
            ],
        ))
        .await?
        .results::<SetGroupRow>()?;
    let mut groups = Vec::<BestGroup>::new();
    for row in rows {
        let group = group_from_row(&row)?;
        if !groups.contains(&group) {
            groups.push(group);
        }
    }
    Ok(groups)
}

fn group_from_row(row: &SetGroupRow) -> Result<BestGroup> {
    let grip = match row.grip.as_deref() {
        None => GripSelection::None,
        Some(text) => serde_json::from_str::<GripSelection>(text).map_err(|_| {
            console_error!("the stored grip is invalid");
            worker::Error::RustError("the stored grip is invalid".to_owned())
        })?,
    };
    let (grip, attachment) = grip.comparison_key();
    Ok(BestGroup {
        exercise_id: row.exercise_id.clone(),
        grip,
        attachment,
    })
}

fn replace_statement(user_id: &str, now: &str, entry: &PersonalBestEntry) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO personal_bests \
         (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, exercise_id, grip, attachment, kind) DO UPDATE SET \
         value = excluded.value, achieved_on = excluded.achieved_on, \
         training_id = excluded.training_id, updated_at = excluded.updated_at",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(entry.key.exercise_id.clone()),
            SqlValue::Text(grip_key(entry.key.grip.as_deref())),
            SqlValue::Text(grip_key(entry.key.attachment.as_deref())),
            SqlValue::Text(entry.key.kind.as_str().to_owned()),
            SqlValue::Real(entry.value),
            SqlValue::Text(entry.achieved_on.to_string()),
            SqlValue::Text(entry.training_id.clone()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn delete_statement(user_id: &str, key: &PersonalBestKey) -> SqlStatement {
    SqlStatement::new(
        "DELETE FROM personal_bests WHERE user_id = ? AND exercise_id = ? AND grip = ? \
         AND attachment = ? AND kind = ?",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(key.exercise_id.clone()),
            SqlValue::Text(grip_key(key.grip.as_deref())),
            SqlValue::Text(grip_key(key.attachment.as_deref())),
            SqlValue::Text(key.kind.as_str().to_owned()),
        ],
    )
}

struct ExerciseMetadata {
    name: String,
    record_mode: RecordMode,
    laterality: Laterality,
}

/// セットの完了の後に、自己ベストを判定して保存し、更新した項目と保持する項目を返す。
///
/// 判定はそのトレーニングの完了した全セットから作り直す。同じトレーニングで同じ種類を
/// 複数回更新した場合も、保存済みの値より大きい候補だけを書くため、最後の値が残る。
pub async fn apply_completion(
    db: &Db,
    user_id: &str,
    training_id: &str,
) -> Result<CompletionBests> {
    let Some(achieved_on) = training_date(db, user_id, training_id).await? else {
        return Ok(CompletionBests::default());
    };
    let sets = match stored_sets::load_training(db, user_id, training_id).await {
        Ok(exercises) => stored_sets::session_sets(&exercises),
        Err(error) => {
            console_error!("failed to build the sets for the personal bests: {error}");
            return Err(worker::Error::RustError(
                "the stored sets are invalid".to_owned(),
            ));
        }
    };
    let candidates = candidates(achieved_on, &sets);
    let updated = apply_candidates(db, user_id, training_id, achieved_on, &candidates).await?;
    let held = held_for_training(db, user_id, training_id, &candidates).await?;
    Ok(CompletionBests { updated, held })
}

/// 候補を保存し、保存済みの値より大きいものだけを返す。
///
/// 終了のサマリーは、読み込んだセットから作った候補を渡して同じ判定を使う。
pub async fn apply_candidates(
    db: &Db,
    user_id: &str,
    training_id: &str,
    achieved_on: JstDate,
    candidates: &[PersonalBestCandidate],
) -> Result<Vec<PersonalBestItem>> {
    let existing = load_entries(db, user_id).await?;
    let updates = updated(&existing, candidates);
    if updates.is_empty() {
        return Ok(Vec::new());
    }
    let now = datetime::format_utc(crate::auth::now_seconds());
    let statements = updates
        .iter()
        .map(|candidate| upsert_statement(user_id, training_id, achieved_on, &now, candidate))
        .collect::<Vec<_>>();
    if let Err(error) = db.batch(&statements).await {
        console_error!("failed to save the personal bests: {error}");
        return Err(worker::Error::RustError(
            "failed to save the personal bests".to_owned(),
        ));
    }
    Ok(updates.iter().map(PersonalBestCandidate::item).collect())
}

/// トレーニングが保持する自己ベストを、サマリーの項目の順で返す。
///
/// 表の行のうち達成したトレーニングがこのトレーニングであるものを選ぶ。種目名と記録方式は
/// トレーニングの種目のスナップショットから補う。候補に無い行は、表の値をそのまま返す。
pub async fn held_for_training(
    db: &Db,
    user_id: &str,
    training_id: &str,
    candidates: &[PersonalBestCandidate],
) -> Result<Vec<PersonalBestItem>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT exercise_id, grip, attachment, kind, value, achieved_on, training_id \
             FROM personal_bests WHERE user_id = ? AND training_id = ? \
             ORDER BY kind, exercise_id, grip, attachment",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    let rows = result.results::<PersonalBestRow>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let keys = rows.iter().map(row_key).collect::<Result<Vec<_>>>()?;
    let metadata = exercise_metadata(db, user_id, training_id).await?;
    let mut used = vec![false; rows.len()];
    let mut items = Vec::with_capacity(rows.len());
    for candidate in updated(&[], candidates) {
        let Some(index) = keys.iter().position(|key| *key == candidate.key()) else {
            continue;
        };
        if used[index] {
            continue;
        }
        used[index] = true;
        items.push(held_item(&rows[index], &metadata, Some(&candidate))?);
    }
    for (index, row) in rows.iter().enumerate() {
        if used[index] {
            continue;
        }
        items.push(held_item(row, &metadata, None)?);
    }
    Ok(items)
}

async fn training_date(db: &Db, user_id: &str, training_id: &str) -> Result<Option<JstDate>> {
    let row = db
        .first::<TrainingDateRow>(&SqlStatement::new(
            "SELECT date FROM training_sessions WHERE id = ? AND user_id = ?",
            vec![
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
            ],
        ))
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let date = JstDate::parse(&row.date).ok_or_else(|| {
        console_error!("the stored training date is invalid");
        worker::Error::RustError("the stored training date is invalid".to_owned())
    })?;
    Ok(Some(date))
}

async fn exercise_metadata(
    db: &Db,
    user_id: &str,
    training_id: &str,
) -> Result<HashMap<String, ExerciseMetadata>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT exercise_id, snapshot FROM training_exercises \
             WHERE user_id = ? AND training_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    let mut metadata = HashMap::new();
    for row in result.results::<ExerciseSnapshotRow>()? {
        let snapshot = serde_json::from_str::<ExerciseSnapshot>(&row.snapshot).map_err(|_| {
            console_error!("the stored exercise snapshot is invalid");
            worker::Error::RustError("the stored exercise snapshot is invalid".to_owned())
        })?;
        metadata.insert(
            row.exercise_id,
            ExerciseMetadata {
                name: snapshot.name,
                record_mode: snapshot.record_mode,
                laterality: snapshot.laterality,
            },
        );
    }
    Ok(metadata)
}

fn held_item(
    row: &PersonalBestRow,
    metadata: &HashMap<String, ExerciseMetadata>,
    candidate: Option<&PersonalBestCandidate>,
) -> Result<PersonalBestItem> {
    let key = row_key(row)?;
    let exercise = metadata.get(&row.exercise_id);
    Ok(PersonalBestItem {
        exercise_id: row.exercise_id.clone(),
        exercise_name: candidate
            .and_then(|candidate| candidate.exercise_name.clone())
            .or_else(|| exercise.map(|exercise| exercise.name.clone())),
        record_mode: candidate.map_or_else(
            || {
                exercise.map_or_else(
                    || fallback_record_mode(key.kind),
                    |exercise| exercise.record_mode,
                )
            },
            |candidate| candidate.record_mode,
        ),
        laterality: candidate.map_or_else(
            || exercise.map_or(Laterality::Bilateral, |exercise| exercise.laterality),
            |candidate| candidate.laterality,
        ),
        grip: key.grip,
        attachment: key.attachment,
        kind: key.kind,
        value: row.value,
    })
}

fn fallback_record_mode(kind: BestKind) -> RecordMode {
    match kind {
        BestKind::LongestDuration => RecordMode::Time,
        BestKind::MaxWeight | BestKind::EstimatedOneRepMax | BestKind::TotalWeight => {
            RecordMode::WeightAndReps
        }
    }
}

async fn load_entries(db: &Db, user_id: &str) -> Result<Vec<PersonalBestEntry>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT exercise_id, grip, attachment, kind, value, achieved_on, training_id \
             FROM personal_bests WHERE user_id = ?",
            vec![SqlValue::Text(user_id.to_owned())],
        ))
        .await?;
    let rows = result.results::<PersonalBestRow>()?;
    rows.iter().map(entry_from_row).collect()
}

fn entry_from_row(row: &PersonalBestRow) -> Result<PersonalBestEntry> {
    let achieved_on = JstDate::parse(&row.achieved_on).ok_or_else(|| {
        console_error!("the stored personal best date is invalid");
        worker::Error::RustError("the stored personal best date is invalid".to_owned())
    })?;
    Ok(PersonalBestEntry {
        key: row_key(row)?,
        value: row.value,
        achieved_on,
        training_id: row.training_id.clone(),
    })
}

fn row_key(row: &PersonalBestRow) -> Result<PersonalBestKey> {
    let kind = BestKind::parse(&row.kind).ok_or_else(|| {
        console_error!("the stored personal best kind is invalid");
        worker::Error::RustError("the stored personal best kind is invalid".to_owned())
    })?;
    Ok(PersonalBestKey {
        exercise_id: row.exercise_id.clone(),
        grip: optional_key(&row.grip),
        attachment: optional_key(&row.attachment),
        kind,
    })
}

fn upsert_statement(
    user_id: &str,
    training_id: &str,
    achieved_on: JstDate,
    now: &str,
    candidate: &PersonalBestCandidate,
) -> SqlStatement {
    SqlStatement::new(
        "INSERT INTO personal_bests \
         (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, exercise_id, grip, attachment, kind) DO UPDATE SET \
         value = excluded.value, achieved_on = excluded.achieved_on, \
         training_id = excluded.training_id, updated_at = excluded.updated_at \
         WHERE excluded.value > personal_bests.value",
        vec![
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Text(candidate.exercise_id.clone()),
            SqlValue::Text(grip_key(candidate.grip.as_deref())),
            SqlValue::Text(grip_key(candidate.attachment.as_deref())),
            SqlValue::Text(candidate.kind.as_str().to_owned()),
            SqlValue::Real(candidate.value),
            SqlValue::Text(achieved_on.to_string()),
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(now.to_owned()),
            SqlValue::Text(now.to_owned()),
        ],
    )
}

fn grip_key(value: Option<&str>) -> String {
    value.unwrap_or_default().to_owned()
}

fn optional_key(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use warrior_core::personal_best::PersonalBestKey;
    use warrior_core::training_session::BestKind;

    use super::{optional_key, row_key, PersonalBestRow};

    #[test]
    fn the_stored_empty_grip_and_attachment_become_none() {
        let row = row("", "");
        assert_eq!(
            row_key(&row).expect("a key"),
            PersonalBestKey {
                exercise_id: "bench".to_owned(),
                grip: None,
                attachment: None,
                kind: BestKind::MaxWeight,
            }
        );
        assert_eq!(
            optional_key("pronated_medium"),
            Some("pronated_medium".to_owned())
        );
        assert_eq!(optional_key(""), None);
    }

    #[test]
    fn a_known_kind_string_becomes_the_kind() {
        let mut row = row("wide", "wide_bar");
        row.kind = "longest_duration".to_owned();
        assert_eq!(
            row_key(&row).expect("a key").kind,
            BestKind::LongestDuration
        );
    }

    fn row(grip: &str, attachment: &str) -> PersonalBestRow {
        PersonalBestRow {
            exercise_id: "bench".to_owned(),
            grip: grip.to_owned(),
            attachment: attachment.to_owned(),
            kind: "max_weight".to_owned(),
            value: 100.0,
            achieved_on: "2026-11-12".to_owned(),
            training_id: "training".to_owned(),
        }
    }
}
