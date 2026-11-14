//! 前回のトレーニングの選択と、今回との差の計算 (PRD の「計算ルールと用語定義」の「前回」)。
//!
//! 前回は、種目ごととトレーニング全体で別の規則で選ぶ。選択を純粋な関数に置き、
//! D1 からの候補の読み込みは `backend/warrior/src/training_session_exercise.rs` が担う。
//! 差の計算もここに置き、画面とサーバーで同じ値を使う。

use serde::{Deserialize, Serialize};

use crate::set_calc::{RecordMode, SetKind, SetSegment};
use crate::training_session::SessionSource;

/// 前回の取得の応答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviousResponse {
    /// 種目ごとの前回。前回が無ければ None。
    pub exercise: Option<PreviousExercise>,
    /// トレーニング全体の前回。前回が無ければ None。
    pub training: Option<PreviousTraining>,
}

/// 種目ごとの前回の記録。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviousExercise {
    /// 完了したセット。並びは記録した順である。
    pub sets: Vec<PreviousSet>,
    /// セットの総重量 (kg)。ウォームアップと時間で記録するセットは含めない。
    pub total_weight_kg: f64,
    /// 時間で記録するセットの合計時間 (秒)。
    pub total_duration_seconds: u32,
    /// 時間で記録するセットの最長時間 (秒)。
    pub max_duration_seconds: u32,
}

/// 前回の 1 セット。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviousSet {
    /// セット種別。
    pub kind: SetKind,
    /// RPE。
    pub rpe: Option<f64>,
    /// 記録時のグリップの選択の JSON。選ばない種目は None。
    pub grip: Option<String>,
    /// 段の値。ドロップセットは複数の段を持つ。
    pub segments: Vec<SetSegment>,
}

/// トレーニング全体の前回の記録。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviousTraining {
    /// 時間で記録する種目を除いた総重量 (kg)。終了時に保存した値を使う。
    pub total_weight_kg: f64,
}

/// 種目ごとの前回を選ぶ比較のキー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousExerciseKey {
    /// 種目の ID。
    pub exercise_id: String,
    /// グリップの比較のキー。
    pub grip: Option<String>,
    /// アタッチメントの比較のキー。
    pub attachment: Option<String>,
}

/// 前回の候補のセットが持つ比較のキー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousSetKey {
    /// セットを記録した種目の ID。
    pub exercise_id: String,
    /// グリップの比較のキー。
    pub grip: Option<String>,
    /// アタッチメントの比較のキー。
    pub attachment: Option<String>,
}

/// 種目ごとの前回の候補。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousExerciseCandidate {
    /// 候補のトレーニングの ID。
    pub training_id: String,
    /// 終了した時刻の ISO 8601 の UTC の文字列。
    pub finished_at: String,
    /// 候補のトレーニングが持つ完了したセットの比較のキー。
    pub sets: Vec<PreviousSetKey>,
}

/// 種目ごとの前回を選ぶ。
///
/// 同じ種目、同じグリップ、同じアタッチメントのセットを持つ候補のうち、終了した時刻が
/// 最も新しいものを返す。同じ時刻の候補はトレーニングの ID の大きい方を選び、順序を固定する。
pub fn select_exercise_previous<'a>(
    candidates: &'a [PreviousExerciseCandidate],
    key: &PreviousExerciseKey,
) -> Option<&'a PreviousExerciseCandidate> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate.sets.iter().any(|set| {
                set.exercise_id == key.exercise_id
                    && set.grip == key.grip
                    && set.attachment == key.attachment
            })
        })
        .max_by(|left, right| {
            left.finished_at
                .cmp(&right.finished_at)
                .then_with(|| left.training_id.cmp(&right.training_id))
        })
}

/// トレーニング全体の前回の候補。
#[derive(Debug, Clone, PartialEq)]
pub struct PreviousTrainingCandidate {
    /// 候補のトレーニングの ID。
    pub training_id: String,
    /// 終了した時刻の ISO 8601 の UTC の文字列。
    pub finished_at: String,
    /// 開始元の種別。
    pub source_kind: SessionSource,
    /// 開始元のルーティンの UUID v4。
    pub routine_id: Option<String>,
    /// 開始元のプログラムの日の UUID v4。
    pub program_day_id: Option<String>,
    /// 開始元の独自のプログラムの日の UUID v4。
    pub custom_program_day_id: Option<String>,
    /// 時間で記録する種目を除いた総重量 (kg)。
    pub total_weight_kg: f64,
}

/// トレーニング全体の前回を選ぶ、現在のトレーニングの開始元。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousTrainingSource {
    /// 開始元のルーティンの UUID v4。
    pub routine_id: Option<String>,
    /// 開始元のプログラムの日の UUID v4。
    pub program_day_id: Option<String>,
    /// 開始元の独自のプログラムの日の UUID v4。
    pub custom_program_day_id: Option<String>,
}

impl PreviousTrainingSource {
    /// 現在のトレーニングの開始元を、候補を選ぶ条件にする。
    ///
    /// プログラム、独自のプログラム、ルーティンの順に優先する。どれも無ければ、
    /// 空の状態から開始したトレーニングを選ぶ (PRD の「前回」)。
    pub fn new(
        routine_id: Option<String>,
        program_day_id: Option<String>,
        custom_program_day_id: Option<String>,
    ) -> Self {
        Self {
            routine_id,
            program_day_id,
            custom_program_day_id,
        }
    }
}

/// トレーニング全体の前回を選ぶ。
///
/// 現在のトレーニングにプログラムの日があれば同じプログラムの日、無ければ同じ独自の
/// プログラムの日、無ければ同じルーティンの候補を選ぶ。どれも無ければ、空の状態から
/// 開始した候補を選ぶ。候補の中で終了した時刻が最も新しいものを返し、同じ時刻は
/// トレーニングの ID の大きい方を選ぶ。
pub fn select_training_previous<'a>(
    candidates: &'a [PreviousTrainingCandidate],
    source: &PreviousTrainingSource,
) -> Option<&'a PreviousTrainingCandidate> {
    candidates
        .iter()
        .filter(|candidate| training_source_matches(candidate, source))
        .max_by(|left, right| {
            left.finished_at
                .cmp(&right.finished_at)
                .then_with(|| left.training_id.cmp(&right.training_id))
        })
}

fn training_source_matches(
    candidate: &PreviousTrainingCandidate,
    source: &PreviousTrainingSource,
) -> bool {
    if let Some(program_day_id) = &source.program_day_id {
        return candidate.program_day_id.as_deref() == Some(program_day_id.as_str());
    }
    if let Some(custom_program_day_id) = &source.custom_program_day_id {
        return candidate.custom_program_day_id.as_deref() == Some(custom_program_day_id.as_str());
    }
    if let Some(routine_id) = &source.routine_id {
        return candidate.routine_id.as_deref() == Some(routine_id.as_str());
    }
    candidate.source_kind == SessionSource::Empty
}

/// 総重量の差。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TotalWeightDifference {
    /// 今回の総重量 (kg)。
    pub current_kg: f64,
    /// 前回の総重量 (kg)。
    pub previous_kg: f64,
    /// 今回から前回を引いた値 (kg)。
    pub difference_kg: f64,
}

/// 総重量の差を計算する。前回が無ければ None を返す。
pub fn total_weight_difference(
    current_kg: f64,
    previous_kg: Option<f64>,
) -> Option<TotalWeightDifference> {
    previous_kg.map(|previous_kg| TotalWeightDifference {
        current_kg,
        previous_kg,
        difference_kg: current_kg - previous_kg,
    })
}

/// 時間で記録する種目の、合計時間と最長時間の差。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DurationDifference {
    /// 今回の合計時間 (秒)。
    pub current_total_seconds: u32,
    /// 前回の合計時間 (秒)。
    pub previous_total_seconds: u32,
    /// 今回から前回を引いた合計時間 (秒)。
    pub total_difference_seconds: i64,
    /// 今回の最長時間 (秒)。
    pub current_longest_seconds: u32,
    /// 前回の最長時間 (秒)。
    pub previous_longest_seconds: u32,
    /// 今回から前回を引いた最長時間 (秒)。
    pub longest_difference_seconds: i64,
}

/// 時間で記録する種目の差を計算する。前回が無ければ None を返す。
pub fn duration_difference(
    current_total_seconds: u32,
    current_longest_seconds: u32,
    previous: Option<&PreviousExercise>,
) -> Option<DurationDifference> {
    previous.map(|previous| DurationDifference {
        current_total_seconds,
        previous_total_seconds: previous.total_duration_seconds,
        total_difference_seconds: i64::from(current_total_seconds)
            - i64::from(previous.total_duration_seconds),
        current_longest_seconds,
        previous_longest_seconds: previous.max_duration_seconds,
        longest_difference_seconds: i64::from(current_longest_seconds)
            - i64::from(previous.max_duration_seconds),
    })
}

/// 種目ごとの差。記録方式で総重量と時間を切り替える。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExerciseDifference {
    /// 重量と回数で記録する種目の総重量の差。
    Weight(TotalWeightDifference),
    /// 時間で記録する種目の合計時間と最長時間の差。
    Time(DurationDifference),
}

/// 種目ごとの差を計算する。前回が無ければ None を返す。
pub fn exercise_difference(
    mode: RecordMode,
    current_total_weight_kg: f64,
    current_total_duration_seconds: u32,
    current_longest_duration_seconds: u32,
    previous: Option<&PreviousExercise>,
) -> Option<ExerciseDifference> {
    match mode {
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
            total_weight_difference(current_total_weight_kg, previous.map(|p| p.total_weight_kg))
                .map(ExerciseDifference::Weight)
        }
        RecordMode::Time => duration_difference(
            current_total_duration_seconds,
            current_longest_duration_seconds,
            previous,
        )
        .map(ExerciseDifference::Time),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        duration_difference, exercise_difference, select_exercise_previous,
        select_training_previous, total_weight_difference, ExerciseDifference, PreviousExercise,
        PreviousExerciseCandidate, PreviousExerciseKey, PreviousSetKey, PreviousTrainingCandidate,
        PreviousTrainingSource,
    };
    use crate::set_calc::RecordMode;
    use crate::training_session::SessionSource;

    fn exercise_candidate(
        training_id: &str,
        finished_at: &str,
        sets: Vec<(&str, Option<&str>, Option<&str>)>,
    ) -> PreviousExerciseCandidate {
        PreviousExerciseCandidate {
            training_id: training_id.to_owned(),
            finished_at: finished_at.to_owned(),
            sets: sets
                .into_iter()
                .map(|(exercise_id, grip, attachment)| PreviousSetKey {
                    exercise_id: exercise_id.to_owned(),
                    grip: grip.map(str::to_owned),
                    attachment: attachment.map(str::to_owned),
                })
                .collect(),
        }
    }

    fn training_candidate(
        training_id: &str,
        finished_at: &str,
        source_kind: SessionSource,
        routine_id: Option<&str>,
        program_day_id: Option<&str>,
        custom_program_day_id: Option<&str>,
        total_weight_kg: f64,
    ) -> PreviousTrainingCandidate {
        PreviousTrainingCandidate {
            training_id: training_id.to_owned(),
            finished_at: finished_at.to_owned(),
            source_kind,
            routine_id: routine_id.map(str::to_owned),
            program_day_id: program_day_id.map(str::to_owned),
            custom_program_day_id: custom_program_day_id.map(str::to_owned),
            total_weight_kg,
        }
    }

    #[test]
    fn the_exercise_previous_is_the_latest_matching_set() {
        let candidates = vec![
            exercise_candidate(
                "training-a",
                "2026-11-10T10:00:00Z",
                vec![("bench", Some("pronated_medium"), None)],
            ),
            exercise_candidate(
                "training-b",
                "2026-11-12T10:00:00Z",
                vec![("bench", None, None)],
            ),
            exercise_candidate(
                "training-c",
                "2026-11-11T10:00:00Z",
                vec![("bench", Some("pronated_medium"), None)],
            ),
        ];
        let key = PreviousExerciseKey {
            exercise_id: "bench".to_owned(),
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
        };
        assert_eq!(
            select_exercise_previous(&candidates, &key)
                .map(|candidate| candidate.training_id.as_str()),
            Some("training-c")
        );
    }

    #[test]
    fn a_different_grip_is_not_the_previous() {
        let candidates = vec![exercise_candidate(
            "training-a",
            "2026-11-12T10:00:00Z",
            vec![("bench", Some("supinated_medium"), None)],
        )];
        let key = PreviousExerciseKey {
            exercise_id: "bench".to_owned(),
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
        };
        assert_eq!(select_exercise_previous(&candidates, &key), None);
    }

    #[test]
    fn the_program_day_wins_over_a_newer_routine() {
        let candidates = vec![
            training_candidate(
                "training-routine",
                "2026-11-12T10:00:00Z",
                SessionSource::Routine,
                Some("routine-1"),
                None,
                None,
                1_000.0,
            ),
            training_candidate(
                "training-program",
                "2026-11-10T10:00:00Z",
                SessionSource::Program,
                None,
                Some("day-1"),
                None,
                900.0,
            ),
        ];
        let source = PreviousTrainingSource::new(
            Some("routine-1".to_owned()),
            Some("day-1".to_owned()),
            None,
        );
        assert_eq!(
            select_training_previous(&candidates, &source)
                .map(|candidate| candidate.training_id.as_str()),
            Some("training-program")
        );
    }

    #[test]
    fn an_empty_source_matches_only_empty_sources() {
        let candidates = vec![
            training_candidate(
                "training-routine",
                "2026-11-12T10:00:00Z",
                SessionSource::Routine,
                Some("routine-1"),
                None,
                None,
                1_000.0,
            ),
            training_candidate(
                "training-empty",
                "2026-11-11T10:00:00Z",
                SessionSource::Empty,
                None,
                None,
                None,
                800.0,
            ),
        ];
        let source = PreviousTrainingSource::new(None, None, None);
        assert_eq!(
            select_training_previous(&candidates, &source)
                .map(|candidate| candidate.training_id.as_str()),
            Some("training-empty")
        );
    }

    #[test]
    fn the_total_weight_difference_is_current_minus_previous() {
        assert_eq!(
            total_weight_difference(1_100.0, Some(900.0)),
            Some(super::TotalWeightDifference {
                current_kg: 1_100.0,
                previous_kg: 900.0,
                difference_kg: 200.0,
            })
        );
        assert_eq!(total_weight_difference(1_100.0, None), None);
    }

    #[test]
    fn the_time_difference_has_the_total_and_the_longest() {
        let previous = PreviousExercise {
            sets: Vec::new(),
            total_weight_kg: 0.0,
            total_duration_seconds: 120,
            max_duration_seconds: 90,
        };
        assert_eq!(
            duration_difference(150, 100, Some(&previous)),
            Some(super::DurationDifference {
                current_total_seconds: 150,
                previous_total_seconds: 120,
                total_difference_seconds: 30,
                current_longest_seconds: 100,
                previous_longest_seconds: 90,
                longest_difference_seconds: 10,
            })
        );
        assert_eq!(duration_difference(150, 100, None), None);
        assert_eq!(
            exercise_difference(RecordMode::Time, 0.0, 150, 100, Some(&previous)),
            Some(ExerciseDifference::Time(super::DurationDifference {
                current_total_seconds: 150,
                previous_total_seconds: 120,
                total_difference_seconds: 30,
                current_longest_seconds: 100,
                previous_longest_seconds: 90,
                longest_difference_seconds: 10,
            }))
        );
    }
}
