//! AI の生成に渡すコンテキストを D1 から組み立てる (ADR-010)。
//!
//! 対象のトレーニングと同じ日の終了済みトレーニングを読み、プロフィールと合わせて
//! `AiContext` を作る。アカウントの識別子、メールアドレス、端末の識別子、プッシュ購読は
//! 持たない。必須のプロフィール項目が欠けている場合は、生成を失敗させるために None を返す。

use serde::Deserialize;
use warrior_core::ai::context::{
    AiContext, AiProfile, ContextExercise, ContextSet, ContextTraining, PartAnalysis, PartCategory,
    PartSetReference, TrainingExperience as AiTrainingExperience,
};
use warrior_core::exercise::{BodyPart, MuscleRole};
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{Goal, Sex};
use warrior_core::profile::{Profile, TrainingExperience};
use warrior_core::set_calc::{PartRole, SetRecord};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::Result;

use crate::db::Db;
use crate::stored_sets::{self, StoredExercise};

/// コンテキストの組み立てに使うプロフィールの行。
#[derive(Debug, Deserialize)]
struct ProfileRow {
    height_cm: Option<f64>,
    weight_kg: Option<f64>,
    birth_date: Option<JstDate>,
    sex: Option<Sex>,
    training_experience: Option<TrainingExperience>,
    goal: Option<Goal>,
    training_days_per_week: Option<u32>,
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
            activity_level: None,
        }
    }
}

/// コンテキストの組み立てに使うトレーニングの行。
#[derive(Debug, Deserialize)]
struct TrainingRow {
    id: String,
    date: String,
    memo: String,
}

/// 対象のトレーニングのコンテキストを組み立てる。
///
/// 対象のトレーニングが終了済みで、プロフィールの必須項目が揃っている場合だけ Some を返す。
pub async fn build(db: &Db, user_id: &str, training_id: &str) -> Result<Option<AiContext>> {
    let Some(profile) = load_profile(db, user_id).await? else {
        return Ok(None);
    };
    let profile = profile.into_profile();
    let Some(target) = load_training(db, user_id, training_id).await? else {
        return Ok(None);
    };
    let Some(date) = JstDate::parse(&target.date) else {
        return Ok(None);
    };
    let Some(age) = profile.age_on(date) else {
        return Ok(None);
    };
    let (
        Some(height_cm),
        Some(weight_kg),
        Some(sex),
        Some(experience),
        Some(goal),
        Some(training_days_per_week),
    ) = (
        profile.height_cm,
        profile.weight_kg,
        profile.sex,
        profile.training_experience,
        profile.goal,
        profile.training_days_per_week,
    )
    else {
        return Ok(None);
    };
    let trainings = load_day_trainings(db, user_id, &target.date).await?;
    let exercises = stored_sets::load_day(db, user_id, &target.date).await?;
    let recorded = exercises
        .iter()
        .filter(|exercise| {
            trainings
                .iter()
                .any(|training| training.id == exercise.training_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let records = trainings
        .iter()
        .map(|training| ContextTraining {
            date,
            memo: optional_memo(&training.memo),
            exercises: recorded
                .iter()
                .filter(|exercise| exercise.training_id == training.id)
                .map(context_exercise)
                .collect(),
        })
        .collect();
    let (part_analysis, assisted_set_reference) = part_totals(&recorded);
    Ok(Some(AiContext {
        goal,
        profile: AiProfile {
            height_cm,
            weight_kg,
            age,
            sex,
        },
        experience: ai_experience(experience),
        training_days_per_week,
        records,
        goals: Vec::new(),
        part_analysis,
        assisted_set_reference,
        recovery: Vec::new(),
        routine: Vec::new(),
        exercises: Vec::new(),
    }))
}

async fn load_profile(db: &Db, user_id: &str) -> Result<Option<ProfileRow>> {
    db.first(&SqlStatement::new(
        "SELECT height_cm, weight_kg, birth_date, sex, training_experience, goal, \
         training_days_per_week FROM profiles WHERE user_id = ?",
        vec![SqlValue::Text(user_id.to_owned())],
    ))
    .await
}

async fn load_training(db: &Db, user_id: &str, training_id: &str) -> Result<Option<TrainingRow>> {
    db.first(&SqlStatement::new(
        "SELECT id, date, memo FROM training_sessions \
         WHERE id = ? AND user_id = ? AND finished_at IS NOT NULL",
        vec![
            SqlValue::Text(training_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

async fn load_day_trainings(db: &Db, user_id: &str, date: &str) -> Result<Vec<TrainingRow>> {
    let result = db
        .all(&SqlStatement::new(
            "SELECT id, date, memo FROM training_sessions \
             WHERE user_id = ? AND date = ? AND finished_at IS NOT NULL \
             ORDER BY finished_at, id",
            vec![
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(date.to_owned()),
            ],
        ))
        .await?;
    result.results::<TrainingRow>()
}

fn optional_memo(memo: &str) -> Option<String> {
    let trimmed = memo.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn ai_experience(experience: TrainingExperience) -> AiTrainingExperience {
    match experience {
        TrainingExperience::Beginner => AiTrainingExperience::Beginner,
        TrainingExperience::Intermediate => AiTrainingExperience::Intermediate,
        TrainingExperience::Advanced => AiTrainingExperience::Advanced,
    }
}

fn context_exercise(exercise: &StoredExercise) -> ContextExercise {
    let first = exercise.sets.first();
    ContextExercise {
        exercise_id: exercise.snapshot.exercise_id.clone(),
        name: exercise.snapshot.name.clone(),
        grip: first.and_then(|set| set.key().grip.clone()),
        attachment: first.and_then(|set| set.key().attachment.clone()),
        mode: exercise.snapshot.record_mode,
        memo: optional_memo(&exercise.memo),
        sets: exercise.sets.iter().map(context_set).collect(),
    }
}

fn context_set(record: &SetRecord) -> ContextSet {
    let first = record.segments().first();
    ContextSet {
        kind: record.kind(),
        weight_kg: first.map_or(0.0, |segment| segment.weight_kg),
        reps: first.map_or(0, |segment| segment.reps),
        duration_seconds: record
            .duration_seconds()
            .or_else(|| first.and_then(|segment| segment.duration_seconds)),
        rpe: record.rpe(),
        momentum_reps: first.map_or(0, |segment| segment.momentum_reps),
        assisted_reps: first.map_or(0, |segment| segment.assisted_reps),
    }
}

fn part_totals(exercises: &[StoredExercise]) -> (Vec<PartAnalysis>, Vec<PartSetReference>) {
    let mut totals = Vec::<PartTotal>::new();
    let mut references = Vec::<PartReference>::new();
    for exercise in exercises {
        let primary_part = part_category(exercise.snapshot.body_part);
        for set in &exercise.sets {
            add_total(&mut totals, primary_part, set);
            for muscle in &exercise.snapshot.muscles {
                let part = part_category(muscle.muscle.body_part());
                let role = match muscle.role {
                    MuscleRole::Primary => PartRole::Primary,
                    MuscleRole::Secondary => PartRole::Assisted,
                };
                add_reference(&mut references, part, set.part_set_count(role));
            }
        }
    }
    let mut part_analysis = Vec::new();
    let mut assisted_set_reference = Vec::new();
    for part in PART_ORDER {
        if let Some(total) = totals.iter().find(|total| total.part == part) {
            if total.sets > 0.0 {
                part_analysis.push(PartAnalysis {
                    part,
                    sets: total.sets,
                    total_weight_kg: total.total_weight_kg,
                });
            }
        }
        if let Some(reference) = references.iter().find(|reference| reference.part == part) {
            if reference.sets > 0.0 {
                assisted_set_reference.push(PartSetReference {
                    part,
                    sets_with_assisted: reference.sets,
                });
            }
        }
    }
    (part_analysis, assisted_set_reference)
}

/// 画面と AI の両方で使う部位区分の並び。
const PART_ORDER: [PartCategory; 8] = [
    PartCategory::Chest,
    PartCategory::Back,
    PartCategory::Shoulders,
    PartCategory::BicepsForearm,
    PartCategory::Triceps,
    PartCategory::Legs,
    PartCategory::Abs,
    PartCategory::Other,
];

struct PartTotal {
    part: PartCategory,
    sets: f64,
    total_weight_kg: f64,
}

struct PartReference {
    part: PartCategory,
    sets: f64,
}

fn add_total(totals: &mut Vec<PartTotal>, part: PartCategory, set: &SetRecord) {
    let index = match totals.iter().position(|total| total.part == part) {
        Some(index) => index,
        None => {
            totals.push(PartTotal {
                part,
                sets: 0.0,
                total_weight_kg: 0.0,
            });
            totals.len() - 1
        }
    };
    let total = &mut totals[index];
    total.sets += set.part_set_count(PartRole::Primary);
    total.total_weight_kg += set.total_weight_kg();
}

fn add_reference(references: &mut Vec<PartReference>, part: PartCategory, sets: f64) {
    let index = match references
        .iter()
        .position(|reference| reference.part == part)
    {
        Some(index) => index,
        None => {
            references.push(PartReference { part, sets: 0.0 });
            references.len() - 1
        }
    };
    references[index].sets += sets;
}

fn part_category(part: BodyPart) -> PartCategory {
    match part {
        BodyPart::Chest => PartCategory::Chest,
        BodyPart::Back => PartCategory::Back,
        BodyPart::Shoulders => PartCategory::Shoulders,
        BodyPart::BicepsForearm => PartCategory::BicepsForearm,
        BodyPart::Triceps => PartCategory::Triceps,
        BodyPart::Legs => PartCategory::Legs,
        BodyPart::Abs => PartCategory::Abs,
        BodyPart::Other => PartCategory::Other,
    }
}
