//! ルーティンと、ルーティンの種目ごとの目標の型と検証 (PRD の「ルーティン」)。
//!
//! ルーティンは名称と並び順を持ち、順序を持つ種目を持つ。種目は保存した時点の
//! スナップショットと、セット数、目標重量、目標回数または目標時間、グリップ、
//! レスト時間を持つ。保存と読み直しは `backend/warrior/src/routine.rs` が担う。

use serde::{Deserialize, Serialize};

use crate::exercise::{ExerciseSnapshot, GripMode, GripSelection};
use crate::ids::is_valid_uuid_v4;
use crate::rest_timer::{MAX_SECONDS as REST_MAX_SECONDS, MIN_SECONDS as REST_MIN_SECONDS};
use crate::set_calc::RecordMode;

/// ルーティンの名称の上限の文字数。
pub const NAME_MAX_CHARS: usize = 100;

/// 目標時間の上限の秒数。1 日とする。
pub const DURATION_MAX_SECONDS: u32 = 86_400;

/// ルーティン全体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    /// ルーティンの UUID v4。
    pub id: String,
    /// 名称。
    pub name: String,
    /// 一覧の中での並び順。
    pub position: i64,
    /// 順序を持つ種目。
    pub exercises: Vec<RoutineExercise>,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// ルーティンの一覧の応答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineList {
    /// 利用者のルーティン。並び順の昇順である。
    pub routines: Vec<Routine>,
}

/// ルーティンの 1 種目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineExercise {
    /// 種目の行の UUID v4。
    pub id: String,
    /// 対象のルーティンの UUID v4。
    pub routine_id: String,
    /// 種目の ID。
    pub exercise_id: String,
    /// 保存した時点の種目の内容。
    pub snapshot: ExerciseSnapshot,
    /// ルーティンの中での並び順。1 以上。
    pub position: i64,
    /// 種目ごとの目標。
    pub target: RoutineTarget,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// ルーティンの種目ごとの目標。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineTarget {
    /// 目標のセット数。1 以上。
    pub set_count: u32,
    /// 目標重量 (kg)。自重種目は加重である。指定しないときは None。
    pub weight_kg: Option<f64>,
    /// 目標回数。時間で記録する種目は None。
    pub reps: Option<u32>,
    /// 目標時間 (秒)。時間で記録する種目だけが持つ。
    pub duration_seconds: Option<u32>,
    /// グリップまたはアタッチメント。選ばない種目は None。
    pub grip: Option<GripSelection>,
    /// この種目に指定するレスト時間 (秒)。指定しないときは全体の既定値を使う。
    pub rest_seconds: Option<u32>,
}

/// 保存する種目の入力。行の ID と時刻を除いた、検証の対象になる値。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutineExerciseInput {
    /// 種目の行の UUID v4。
    pub id: String,
    /// 種目の ID。
    pub exercise_id: String,
    /// 保存する時点の種目の内容。
    pub snapshot: ExerciseSnapshot,
    /// ルーティンの中での並び順。1 以上。
    pub position: i64,
    /// 種目ごとの目標。
    pub target: RoutineTarget,
}

impl Routine {
    /// ルーティンの不変条件を検証する。
    pub fn validate(&self) -> Result<(), RoutineError> {
        validate(&self.name, &self.inputs())
    }

    /// 検証に渡す、種目の入力の並び。
    pub fn inputs(&self) -> Vec<RoutineExerciseInput> {
        self.exercises
            .iter()
            .map(|exercise| RoutineExerciseInput {
                id: exercise.id.clone(),
                exercise_id: exercise.exercise_id.clone(),
                snapshot: exercise.snapshot.clone(),
                position: exercise.position,
                target: exercise.target.clone(),
            })
            .collect()
    }

    /// 元の内容を写した別のルーティンを作る。
    ///
    /// 新しいルーティンと種目の行の ID は呼び出し元が決める。ID の数が種目の数と
    /// 一致しないとき、または空か重複する ID があるときは None を返す。
    pub fn duplicate(
        &self,
        new_id: &str,
        new_position: i64,
        new_exercise_ids: &[String],
        now: &str,
    ) -> Option<Self> {
        if new_id.is_empty() || new_exercise_ids.len() != self.exercises.len() {
            return None;
        }
        let mut seen = Vec::new();
        for id in new_exercise_ids {
            if id.is_empty() || seen.contains(id) {
                return None;
            }
            seen.push(id.clone());
        }
        let exercises = self
            .exercises
            .iter()
            .zip(new_exercise_ids)
            .map(|(source, id)| RoutineExercise {
                id: id.clone(),
                routine_id: new_id.to_owned(),
                exercise_id: source.exercise_id.clone(),
                snapshot: source.snapshot.clone(),
                position: source.position,
                target: source.target.clone(),
                created_at: now.to_owned(),
                updated_at: now.to_owned(),
            })
            .collect();
        Some(Self {
            id: new_id.to_owned(),
            name: self.name.clone(),
            position: new_position,
            exercises,
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        })
    }
}

/// ルーティンの検証の失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutineError {
    /// 名称が空である。
    EmptyName,
    /// 名称が 100 文字を超える。
    NameTooLong,
    /// 種目の行または種目の ID が UUID v4 でない。
    InvalidId,
    /// 種目の並び順が 1 未満、または重複している。
    InvalidPosition,
    /// 種目のスナップショットが不正である、または種目の ID と一致しない。
    InvalidSnapshot,
    /// セット数が 0 である。
    InvalidSetCount,
    /// 目標重量が負、または有限でない。
    InvalidWeight,
    /// 目標回数が 0 である。
    InvalidReps,
    /// 目標時間が範囲外である。
    InvalidDuration,
    /// 記録方式に対する目標値の組み合わせが不正である。
    InvalidTarget,
    /// グリップが種目の選択肢に合わない。
    InvalidGrip,
    /// レスト時間が範囲外である。
    InvalidRest,
}

/// 名称と種目の入力から、ルーティンの不変条件を検証する。
pub fn validate(name: &str, exercises: &[RoutineExerciseInput]) -> Result<(), RoutineError> {
    if name.trim().is_empty() {
        return Err(RoutineError::EmptyName);
    }
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(RoutineError::NameTooLong);
    }
    let mut positions = Vec::new();
    let mut ids = Vec::new();
    for exercise in exercises {
        validate_exercise(exercise)?;
        if ids.contains(&exercise.id) {
            return Err(RoutineError::InvalidId);
        }
        ids.push(exercise.id.clone());
        if positions.contains(&exercise.position) {
            return Err(RoutineError::InvalidPosition);
        }
        positions.push(exercise.position);
    }
    Ok(())
}

fn validate_exercise(exercise: &RoutineExerciseInput) -> Result<(), RoutineError> {
    if !is_valid_uuid_v4(&exercise.id) || !is_valid_uuid_v4(&exercise.exercise_id) {
        return Err(RoutineError::InvalidId);
    }
    if exercise.position < 1 {
        return Err(RoutineError::InvalidPosition);
    }
    if exercise.snapshot.exercise_id != exercise.exercise_id
        || exercise.snapshot.validate().is_err()
    {
        return Err(RoutineError::InvalidSnapshot);
    }
    validate_target(&exercise.snapshot, &exercise.target)
}

fn validate_target(
    snapshot: &ExerciseSnapshot,
    target: &RoutineTarget,
) -> Result<(), RoutineError> {
    if target.set_count < 1 {
        return Err(RoutineError::InvalidSetCount);
    }
    if target
        .weight_kg
        .is_some_and(|weight| !weight.is_finite() || weight < 0.0)
    {
        return Err(RoutineError::InvalidWeight);
    }
    match snapshot.record_mode {
        RecordMode::Time => {
            if target.weight_kg.is_some() || target.reps.is_some() {
                return Err(RoutineError::InvalidTarget);
            }
            if !target
                .duration_seconds
                .is_some_and(|seconds| (1..=DURATION_MAX_SECONDS).contains(&seconds))
            {
                return Err(RoutineError::InvalidDuration);
            }
        }
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
            if target.duration_seconds.is_some() {
                return Err(RoutineError::InvalidTarget);
            }
            if !target.reps.is_some_and(|reps| reps >= 1) {
                return Err(RoutineError::InvalidReps);
            }
        }
    }
    validate_grip(snapshot, target.grip)?;
    if target
        .rest_seconds
        .is_some_and(|seconds| !(REST_MIN_SECONDS..=REST_MAX_SECONDS).contains(&seconds))
    {
        return Err(RoutineError::InvalidRest);
    }
    Ok(())
}

fn validate_grip(
    snapshot: &ExerciseSnapshot,
    grip: Option<GripSelection>,
) -> Result<(), RoutineError> {
    match snapshot.grip_mode {
        GripMode::None => {
            if grip.is_some() {
                return Err(RoutineError::InvalidGrip);
            }
        }
        GripMode::Grip | GripMode::Attachment => {
            let Some(grip) = grip else {
                return Err(RoutineError::InvalidGrip);
            };
            if !snapshot.grip_options.contains(&grip)
                || grip.validate_for(snapshot.grip_mode).is_err()
            {
                return Err(RoutineError::InvalidGrip);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-RTN-001`、`REQ-RTN-003`。

    use super::{
        validate, Routine, RoutineError, RoutineExercise, RoutineExerciseInput, RoutineTarget,
    };
    use crate::exercise::{
        BodyPart, ExerciseSnapshot, GripMode, GripSelection, GripStyle, GripWidth, Laterality,
        MuscleAssignment, MuscleRole, MuscleUnit,
    };
    use crate::set_calc::RecordMode;

    const ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5101";
    const EXERCISE_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5102";
    const EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5103";

    #[test]
    fn req_rtn_001_a_valid_routine_passes() {
        assert_eq!(validate("胸の日", &[input()]), Ok(()));
        assert_eq!(routine().validate(), Ok(()));
    }

    #[test]
    fn req_rtn_001_an_empty_name_is_rejected() {
        assert_eq!(validate(" ", &[input()]), Err(RoutineError::EmptyName));
    }

    #[test]
    fn req_rtn_001_a_target_with_zero_sets_is_rejected() {
        let mut value = input();
        value.target.set_count = 0;
        assert_eq!(
            validate("胸の日", &[value]),
            Err(RoutineError::InvalidSetCount)
        );
    }

    #[test]
    fn req_rtn_001_duplicate_exercise_rows_are_rejected() {
        let first = input();
        let mut second = input();
        second.position = 2;
        assert_eq!(
            validate("胸の日", &[first, second]),
            Err(RoutineError::InvalidId)
        );
    }

    #[test]
    fn req_rtn_001_a_time_target_needs_a_duration_and_no_reps() {
        let mut value = input();
        value.snapshot = snapshot(RecordMode::Time, GripMode::None);
        value.target = target();
        value.target.reps = Some(10);
        value.target.weight_kg = None;
        assert_eq!(
            validate("有酸素", &[value.clone()]),
            Err(RoutineError::InvalidTarget)
        );

        value.target.reps = None;
        assert_eq!(
            validate("有酸素", &[value.clone()]),
            Err(RoutineError::InvalidDuration)
        );

        value.target.duration_seconds = Some(60);
        assert_eq!(validate("有酸素", &[value]), Ok(()));
    }

    #[test]
    fn req_rtn_001_a_grip_target_must_match_the_exercise_options() {
        let mut value = input();
        value.snapshot = snapshot(RecordMode::WeightAndReps, GripMode::Grip);
        value.target.grip = None;
        assert_eq!(
            validate("胸の日", &[value.clone()]),
            Err(RoutineError::InvalidGrip)
        );

        value.target.grip = Some(GripSelection::Grip {
            style: GripStyle::Supinated,
            width: GripWidth::Narrow,
        });
        assert_eq!(validate("胸の日", &[value]), Err(RoutineError::InvalidGrip));
    }

    #[test]
    fn req_rtn_003_a_duplicate_has_new_ids_and_the_same_content() {
        let source = routine();
        let new_exercise_id = "3f2504e0-4f89-41d3-9a0c-0305e82c5110".to_owned();
        let copy = source
            .duplicate(
                "3f2504e0-4f89-41d3-9a0c-0305e82c5120",
                2,
                std::slice::from_ref(&new_exercise_id),
                "2026-11-13T00:00:00Z",
            )
            .expect("a duplicate");

        assert_eq!(copy.id, "3f2504e0-4f89-41d3-9a0c-0305e82c5120");
        assert_eq!(copy.position, 2);
        assert_eq!(copy.name, source.name);
        assert_eq!(copy.exercises.len(), 1);
        assert_eq!(copy.exercises[0].id, new_exercise_id);
        assert_eq!(copy.exercises[0].routine_id, copy.id);
        assert_eq!(
            copy.exercises[0].exercise_id,
            source.exercises[0].exercise_id
        );
        assert_eq!(copy.exercises[0].snapshot, source.exercises[0].snapshot);
        assert_eq!(copy.exercises[0].position, source.exercises[0].position);
        assert_eq!(copy.exercises[0].target, source.exercises[0].target);
        assert_eq!(copy.created_at, "2026-11-13T00:00:00Z");
    }

    #[test]
    fn req_rtn_003_editing_a_duplicate_keeps_the_source_unchanged() {
        let source = routine();
        let mut copy = source
            .duplicate(
                "3f2504e0-4f89-41d3-9a0c-0305e82c5120",
                2,
                &["3f2504e0-4f89-41d3-9a0c-0305e82c5110".to_owned()],
                "2026-11-13T00:00:00Z",
            )
            .expect("a duplicate");
        copy.name = "脚の日".to_owned();
        copy.exercises[0].target.reps = Some(20);
        copy.exercises[0].position = 9;
        assert_eq!(source.name, "胸の日");
        assert_eq!(source.exercises[0].target.reps, Some(10));
        assert_eq!(source.exercises[0].position, 1);
    }

    #[test]
    fn req_rtn_003_a_duplicate_rejects_an_id_count_mismatch() {
        let source = routine();
        assert_eq!(source.duplicate("new", 2, &[], "now"), None);
    }

    #[test]
    fn req_rtn_003_a_duplicate_rejects_empty_and_duplicate_ids() {
        let mut source = routine();
        source.exercises.push(RoutineExercise {
            id: "3f2504e0-4f89-41d3-9a0c-0305e82c5104".to_owned(),
            routine_id: ROUTINE_ID.to_owned(),
            exercise_id: EXERCISE_ID.to_owned(),
            snapshot: snapshot(RecordMode::WeightAndReps, GripMode::None),
            position: 2,
            target: target(),
            created_at: "2026-11-12T00:00:00Z".to_owned(),
            updated_at: "2026-11-12T00:00:00Z".to_owned(),
        });
        assert_eq!(
            source.duplicate("new", 2, &["same".to_owned(), "same".to_owned()], "now"),
            None
        );
        assert_eq!(
            source.duplicate("new", 2, &[String::new(), "other".to_owned()], "now"),
            None
        );
    }

    fn routine() -> Routine {
        Routine {
            id: ROUTINE_ID.to_owned(),
            name: "胸の日".to_owned(),
            position: 1,
            exercises: vec![RoutineExercise {
                id: EXERCISE_ROW_ID.to_owned(),
                routine_id: ROUTINE_ID.to_owned(),
                exercise_id: EXERCISE_ID.to_owned(),
                snapshot: snapshot(RecordMode::WeightAndReps, GripMode::None),
                position: 1,
                target: target(),
                created_at: "2026-11-12T00:00:00Z".to_owned(),
                updated_at: "2026-11-12T00:00:00Z".to_owned(),
            }],
            created_at: "2026-11-12T00:00:00Z".to_owned(),
            updated_at: "2026-11-12T00:00:00Z".to_owned(),
        }
    }

    fn input() -> RoutineExerciseInput {
        RoutineExerciseInput {
            id: EXERCISE_ROW_ID.to_owned(),
            exercise_id: EXERCISE_ID.to_owned(),
            snapshot: snapshot(RecordMode::WeightAndReps, GripMode::None),
            position: 1,
            target: target(),
        }
    }

    fn target() -> RoutineTarget {
        RoutineTarget {
            set_count: 3,
            weight_kg: Some(100.0),
            reps: Some(10),
            duration_seconds: None,
            grip: None,
            rest_seconds: Some(120),
        }
    }

    fn snapshot(record_mode: RecordMode, grip_mode: GripMode) -> ExerciseSnapshot {
        ExerciseSnapshot {
            exercise_id: EXERCISE_ID.to_owned(),
            name: "ベンチプレス".to_owned(),
            body_part: BodyPart::Chest,
            muscles: vec![MuscleAssignment {
                muscle: MuscleUnit::PectoralisMajorMiddle,
                role: MuscleRole::Primary,
            }],
            record_mode,
            laterality: Laterality::Bilateral,
            grip_mode,
            grip_options: if grip_mode == GripMode::None {
                Vec::new()
            } else {
                vec![GripSelection::Grip {
                    style: GripStyle::Pronated,
                    width: GripWidth::Medium,
                }]
            },
        }
    }
}
