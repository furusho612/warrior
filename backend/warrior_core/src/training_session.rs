//! トレーニングの開始元と、終了時のサマリーの計算 (PRD の「トレーニング記録」)。
//!
//! サマリーの総重量、セット数、更新した自己ベストは `set_calc` の計算だけを使う。
//! 自己ベストの比較の前の値は今回のトレーニングのセットだけであり、過去の記録との比較は行わない。

use serde::{Deserialize, Serialize};

use crate::jst::JstDate;
use crate::set_calc::{BestKey, BestUpdates, PersonalBest, RecordMode, SetRecord};

/// トレーニングの開始元。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    /// 空の状態から開始した。
    #[default]
    Empty,
    /// ルーティンから開始した。
    Routine,
    /// 用意したプログラムの当日メニューから開始した。
    Program,
    /// 独自のプログラムの当日メニューから開始した。
    CustomProgram,
}

impl SessionSource {
    /// D1 と API に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Routine => "routine",
            Self::Program => "program",
            Self::CustomProgram => "custom_program",
        }
    }

    /// 文字列を開始元に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "empty" => Some(Self::Empty),
            "routine" => Some(Self::Routine),
            "program" => Some(Self::Program),
            "custom_program" => Some(Self::CustomProgram),
            _ => None,
        }
    }
}

/// 終了時に更新した自己ベストの 1 項目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonalBestItem {
    /// 種目の ID。
    pub exercise_id: String,
    /// 自己ベストの種類。`set_calc` の `PersonalBest` の項目名を使う。
    pub kind: String,
    /// 値。時間で記録する種目は秒を表す。
    pub value: f64,
}

/// 終了したトレーニングのサマリー。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingSessionSummary {
    /// 開始時刻から終了時刻までの秒数。
    pub duration_seconds: i64,
    /// 総重量 (kg)。
    pub total_weight_kg: f64,
    /// セットを持つ種目の数。
    pub exercise_count: u32,
    /// トレーニングのセット数。
    pub set_count: u32,
    /// 今回のトレーニングで更新した自己ベスト。
    pub personal_bests: Vec<PersonalBestItem>,
}

/// セットの記録からサマリーを計算する。
pub fn summary_from_sets(duration_seconds: i64, sets: &[SetRecord]) -> TrainingSessionSummary {
    TrainingSessionSummary {
        duration_seconds,
        total_weight_kg: sets.iter().map(SetRecord::total_weight_kg).sum(),
        exercise_count: exercise_count(sets),
        set_count: sets.iter().map(|set| set.counted_sets() as u32).sum(),
        personal_bests: personal_best_items(sets),
    }
}

fn exercise_count(sets: &[SetRecord]) -> u32 {
    let mut exercise_ids = Vec::new();
    for set in sets {
        let exercise_id = &set.key().exercise_id;
        if !exercise_ids.contains(&exercise_id) {
            exercise_ids.push(exercise_id);
        }
    }
    u32::try_from(exercise_ids.len()).unwrap_or(u32::MAX)
}

fn personal_best_items(sets: &[SetRecord]) -> Vec<PersonalBestItem> {
    let mut groups = Vec::<BestGroup>::new();
    for set in sets {
        let position = groups
            .iter()
            .position(|group| group.best.key() == set.key() && group.mode == set.mode());
        let index = match position {
            Some(index) => index,
            None => {
                groups.push(BestGroup {
                    best: PersonalBest::new(set.key().clone()),
                    updates: BestUpdates::default(),
                    mode: set.mode(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        let updates = group.best.update(set, comparison_date());
        group.updates.max_weight |= updates.max_weight;
        group.updates.estimated_one_rep_max |= updates.estimated_one_rep_max;
        group.updates.total_weight |= updates.total_weight;
        group.updates.longest_duration |= updates.longest_duration;
    }

    let mut items = Vec::new();
    for group in &groups {
        push_best(&mut items, group.best.key(), &group.best, group.updates);
    }
    items
}

struct BestGroup {
    best: PersonalBest,
    updates: BestUpdates,
    mode: RecordMode,
}

fn push_best(
    items: &mut Vec<PersonalBestItem>,
    key: &BestKey,
    best: &PersonalBest,
    updates: BestUpdates,
) {
    if updates.max_weight {
        if let Some(value) = best.max_weight() {
            items.push(item(key, "max_weight", value.value));
        }
    }
    if updates.estimated_one_rep_max {
        if let Some(value) = best.estimated_one_rep_max() {
            items.push(item(key, "estimated_one_rep_max", value.value));
        }
    }
    if updates.total_weight {
        if let Some(value) = best.total_weight() {
            items.push(item(key, "total_weight", value.value));
        }
    }
    if updates.longest_duration {
        if let Some(value) = best.longest_duration() {
            items.push(item(key, "longest_duration", f64::from(value.value)));
        }
    }
}

fn item(key: &BestKey, kind: &str, value: f64) -> PersonalBestItem {
    PersonalBestItem {
        exercise_id: key.exercise_id.clone(),
        kind: kind.to_owned(),
        value,
    }
}

/// サマリーは自己ベストの達成日を返さないため、比較だけに使う固定の日付。
fn comparison_date() -> JstDate {
    JstDate::new(2000, 1, 1).expect("the comparison date is a valid date")
}

#[cfg(test)]
mod tests {
    use super::{summary_from_sets, PersonalBestItem, SessionSource, TrainingSessionSummary};
    use crate::set_calc::{
        estimated_one_rep_max, BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue,
    };

    /// 対応する要求: `REQ-REC-005`。
    /// ウォームアップ、ドロップセット、左右別、時間で記録する種目を含む固定の記録で、
    /// サマリーの 5 項目を確かめる。
    #[test]
    fn req_rec_005_summary_counts_the_fixed_record() {
        let summary = summary_from_sets(3_600, &fixed_sets());

        assert_eq!(summary.duration_seconds, 3_600);
        assert!((summary.total_weight_kg - 1_878.0).abs() < 1e-9);
        assert_eq!(summary.exercise_count, 3);
        assert_eq!(summary.set_count, 4);
        assert_eq!(
            summary.personal_bests,
            vec![
                best("bench-press", "max_weight", 100.0),
                best(
                    "bench-press",
                    "estimated_one_rep_max",
                    estimated_one_rep_max(100.0, 8).expect("a value"),
                ),
                best("bench-press", "total_weight", 1_280.0),
                best("curl", "max_weight", 12.0),
                best(
                    "curl",
                    "estimated_one_rep_max",
                    estimated_one_rep_max(12.0, 4).expect("a value"),
                ),
                best("curl", "total_weight", 98.0),
                best("plank", "longest_duration", 90.0),
            ]
        );
    }

    #[test]
    fn summary_of_no_sets_is_all_zero() {
        assert_eq!(
            summary_from_sets(0, &[]),
            TrainingSessionSummary {
                duration_seconds: 0,
                total_weight_kg: 0.0,
                exercise_count: 0,
                set_count: 0,
                personal_bests: Vec::new(),
            }
        );
    }

    /// 対応する要求: `REQ-REC-005`。
    /// ウォームアップだけの種目も、セットを持つ種目として数える。
    #[test]
    fn req_rec_005_an_exercise_with_only_warmup_sets_counts_as_an_exercise() {
        let summary = summary_from_sets(60, &[record("squat", SetKind::Warmup)]);
        assert_eq!(summary.exercise_count, 1);
        assert_eq!(summary.set_count, 0);
        assert!(summary.personal_bests.is_empty());
    }

    #[test]
    fn session_source_round_trips_through_the_stored_string() {
        for source in [
            SessionSource::Empty,
            SessionSource::Routine,
            SessionSource::Program,
            SessionSource::CustomProgram,
        ] {
            assert_eq!(SessionSource::parse(source.as_str()), Some(source));
        }
        assert_eq!(SessionSource::parse("unknown"), None);
        assert_eq!(SessionSource::default(), SessionSource::Empty);
    }

    fn best(exercise_id: &str, kind: &str, value: f64) -> PersonalBestItem {
        PersonalBestItem {
            exercise_id: exercise_id.to_owned(),
            kind: kind.to_owned(),
            value,
        }
    }

    fn key(exercise_id: &str) -> BestKey {
        BestKey {
            exercise_id: exercise_id.to_owned(),
            grip: None,
            attachment: None,
        }
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

    fn record(exercise_id: &str, kind: SetKind) -> SetRecord {
        SetRecord::new(
            key(exercise_id),
            kind,
            vec![segment(40.0, 10)],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid record")
    }

    fn fixed_sets() -> Vec<SetRecord> {
        let bench = key("bench-press");
        let curl = key("curl");
        let plank = key("plank");
        vec![
            SetRecord::new(
                bench.clone(),
                SetKind::Warmup,
                vec![segment(60.0, 10)],
                None,
                None,
                RecordMode::WeightAndReps,
            )
            .expect("a valid warmup"),
            SetRecord::new(
                bench.clone(),
                SetKind::Normal,
                vec![segment(100.0, 5)],
                Some(8.0),
                None,
                RecordMode::WeightAndReps,
            )
            .expect("a valid normal set"),
            SetRecord::new(
                bench,
                SetKind::Drop,
                vec![segment(100.0, 8), segment(80.0, 6)],
                None,
                None,
                RecordMode::WeightAndReps,
            )
            .expect("a valid drop set"),
            SetRecord::new(
                curl,
                SetKind::Normal,
                vec![SetSegment {
                    weight_kg: 0.0,
                    reps: 0,
                    momentum_reps: 0,
                    assisted_reps: 0,
                    left: Some(SideValue {
                        weight_kg: 10.0,
                        reps: 5,
                        duration_seconds: None,
                    }),
                    right: Some(SideValue {
                        weight_kg: 12.0,
                        reps: 4,
                        duration_seconds: None,
                    }),
                    duration_seconds: None,
                }],
                None,
                None,
                RecordMode::WeightAndReps,
            )
            .expect("a valid unilateral set"),
            SetRecord::new(
                plank,
                SetKind::Normal,
                Vec::new(),
                Some(9.0),
                Some(90),
                RecordMode::Time,
            )
            .expect("a valid time set"),
        ]
    }
}
