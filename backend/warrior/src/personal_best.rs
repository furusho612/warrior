//! 自己ベストの D1 の読み書きと、セットの完了の後の判定。
//!
//! 比較と候補の作成は `warrior_core::personal_best` が担う。このモジュールは、保存済みの
//! 自己ベストを読み、更新した候補を `personal_bests` の表へ 1 行ずつ書き、画面へ返す
//! 項目を組み立てる。表の比較の単位は利用者、種目、グリップ、アタッチメント、種類である。
//! グリップとアタッチメントを選ばない記録は空文字で保存する。

use serde::Deserialize;
use warrior_core::datetime;
use warrior_core::jst::JstDate;
use warrior_core::personal_best::{
    candidates, updated, PersonalBestCandidate, PersonalBestEntry, PersonalBestKey,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::training_session::{BestKind, PersonalBestItem};
use worker::{console_error, Result};

use crate::db::Db;
use crate::stored_sets;

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

/// セットの完了の後に、自己ベストを判定して保存し、更新した項目を返す。
///
/// 判定はそのトレーニングの完了した全セットから作り直す。同じトレーニングで同じ種類を
/// 複数回更新した場合も、保存済みの値より大きい候補だけを書くため、最後の値が残る。
pub async fn apply_completion(
    db: &Db,
    user_id: &str,
    training_id: &str,
) -> Result<Vec<PersonalBestItem>> {
    let Some(achieved_on) = training_date(db, user_id, training_id).await? else {
        return Ok(Vec::new());
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
    apply_candidates(db, user_id, training_id, achieved_on, &candidates).await
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

/// トレーニングで更新した自己ベストを、サマリーの項目の順で返す。
///
/// 表の行のうち達成したトレーニングがこのトレーニングであるものを選ぶ。
/// サマリーは同じトレーニングの全セットから作った候補を持つため、表示に必要な種目名と
/// 記録方式をそこから取る。
pub async fn updated_for_training(
    db: &Db,
    user_id: &str,
    training_id: &str,
    session_bests: &[PersonalBestItem],
) -> Result<Vec<PersonalBestItem>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT exercise_id, grip, attachment, kind, value, achieved_on, training_id \
             FROM personal_bests WHERE user_id = ? AND training_id = ?",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(training_id.to_owned()),
            ],
        ))
        .await?;
    let rows = result.results::<PersonalBestRow>()?;
    let keys = rows.iter().map(row_key).collect::<Result<Vec<_>>>()?;
    Ok(session_bests
        .iter()
        .filter(|item| {
            let key = PersonalBestKey {
                exercise_id: item.exercise_id.clone(),
                grip: item.grip.clone(),
                attachment: item.attachment.clone(),
                kind: item.kind,
            };
            keys.contains(&key)
        })
        .cloned()
        .collect())
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
