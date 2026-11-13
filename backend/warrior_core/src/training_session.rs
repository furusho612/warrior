//! トレーニングの開始元と、終了時のサマリーの計算 (PRD の「トレーニング記録」)。
//!
//! サマリーの総重量、セット数、今回のセッションのベストは `set_calc` の計算だけを使う。
//! 今回のセッションのベストと過去の終了済みトレーニングのベストを比較し、
//! 更新したものだけを画面に返す。

use serde::{Deserialize, Serialize};

use crate::exercise::Laterality;
use crate::jst::JstDate;
use crate::set_calc::{BestKey, PersonalBest, RecordMode, SetRecord};

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

/// 自己ベストの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BestKind {
    /// 最大重量。
    MaxWeight,
    /// 推定 1RM。
    EstimatedOneRepMax,
    /// 種目別の総重量。
    TotalWeight,
    /// 時間で記録する種目の最長時間。
    LongestDuration,
}

impl BestKind {
    /// 4 種類。
    pub const ALL: [Self; 4] = [
        Self::MaxWeight,
        Self::EstimatedOneRepMax,
        Self::TotalWeight,
        Self::LongestDuration,
    ];

    /// D1 の JSON と API に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxWeight => "max_weight",
            Self::EstimatedOneRepMax => "estimated_one_rep_max",
            Self::TotalWeight => "total_weight",
            Self::LongestDuration => "longest_duration",
        }
    }
}

/// 終了時に更新した自己ベストの 1 項目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonalBestItem {
    /// 種目の ID。
    pub exercise_id: String,
    /// 種目名。記録時点のスナップショットの名前を持つ。
    #[serde(default)]
    pub exercise_name: Option<String>,
    /// 記録方式。比較のキーに含める。
    pub record_mode: RecordMode,
    /// 左右別に行うか。比較のキーに含める。
    pub laterality: Laterality,
    /// グリップ。比較のキーに含める。
    #[serde(default)]
    pub grip: Option<String>,
    /// アタッチメント。比較のキーに含める。
    #[serde(default)]
    pub attachment: Option<String>,
    /// 自己ベストの種類。
    pub kind: BestKind,
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
    /// 今回のセッションの自己ベスト。API はこのうち更新したものだけを返す。
    pub personal_bests: Vec<PersonalBestItem>,
}

/// サマリーの計算に使う 1 セットと、種目の表示と比較の情報。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSet {
    /// セットの記録。
    pub record: SetRecord,
    /// 記録時点の種目名。
    pub exercise_name: String,
    /// 記録時点の左右別の区分。
    pub laterality: Laterality,
}

/// セットの記録から、今回のセッションのサマリーを計算する。
pub fn summary_from_sets(
    achieved_on: JstDate,
    duration_seconds: i64,
    sets: &[SessionSet],
) -> TrainingSessionSummary {
    TrainingSessionSummary {
        duration_seconds,
        total_weight_kg: sets.iter().map(|set| set.record.total_weight_kg()).sum(),
        exercise_count: exercise_count(sets),
        set_count: sets
            .iter()
            .map(|set| set.record.counted_sets() as u32)
            .sum(),
        personal_bests: session_bests(achieved_on, sets),
    }
}

/// 今回のセッションのベストと比べて、更新した自己ベストだけを返す。
pub fn updated_personal_bests(
    session: &[PersonalBestItem],
    past: &[PersonalBestItem],
) -> Vec<PersonalBestItem> {
    session
        .iter()
        .filter(|item| {
            let previous = past
                .iter()
                .filter(|candidate| same_best_key(candidate, item))
                .map(|candidate| candidate.value)
                .fold(f64::NEG_INFINITY, f64::max);
            item.value > previous
        })
        .cloned()
        .collect()
}

fn same_best_key(left: &PersonalBestItem, right: &PersonalBestItem) -> bool {
    left.exercise_id == right.exercise_id
        && left.record_mode == right.record_mode
        && left.laterality == right.laterality
        && left.grip == right.grip
        && left.attachment == right.attachment
        && left.kind == right.kind
}

fn exercise_count(sets: &[SessionSet]) -> u32 {
    let mut exercise_ids = Vec::new();
    for set in sets {
        let exercise_id = &set.record.key().exercise_id;
        if !exercise_ids.contains(&exercise_id) {
            exercise_ids.push(exercise_id);
        }
    }
    u32::try_from(exercise_ids.len()).unwrap_or(u32::MAX)
}

fn session_bests(achieved_on: JstDate, sets: &[SessionSet]) -> Vec<PersonalBestItem> {
    let mut groups = Vec::<BestGroup>::new();
    for set in sets {
        let position = groups.iter().position(|group| group.matches(set));
        let index = match position {
            Some(index) => index,
            None => {
                groups.push(BestGroup::new(set));
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        group.best.update(&set.record, achieved_on);
        group.total_weight += set.record.strict_total_weight_kg();
    }

    let mut items = Vec::new();
    for group in &groups {
        if let Some(value) = group.best.max_weight() {
            items.push(group.item(BestKind::MaxWeight, value.value));
        }
        if let Some(value) = group.best.estimated_one_rep_max() {
            items.push(group.item(BestKind::EstimatedOneRepMax, value.value));
        }
        if group.total_weight > 0.0 {
            items.push(group.item(BestKind::TotalWeight, group.total_weight));
        }
        if let Some(value) = group.best.longest_duration() {
            items.push(group.item(BestKind::LongestDuration, f64::from(value.value)));
        }
    }
    items
}

struct BestGroup {
    key: BestKey,
    mode: RecordMode,
    laterality: Laterality,
    exercise_name: Option<String>,
    best: PersonalBest,
    total_weight: f64,
}

impl BestGroup {
    fn new(set: &SessionSet) -> Self {
        let key = set.record.key().clone();
        let mode = set.record.mode();
        Self {
            best: PersonalBest::new(key.clone()),
            key,
            mode,
            laterality: set.laterality,
            exercise_name: Some(set.exercise_name.clone()),
            total_weight: 0.0,
        }
    }

    fn matches(&self, set: &SessionSet) -> bool {
        self.key == *set.record.key()
            && self.mode == set.record.mode()
            && self.laterality == set.laterality
    }

    fn item(&self, kind: BestKind, value: f64) -> PersonalBestItem {
        PersonalBestItem {
            exercise_id: self.key.exercise_id.clone(),
            exercise_name: self.exercise_name.clone(),
            record_mode: self.mode,
            laterality: self.laterality,
            grip: self.key.grip.clone(),
            attachment: self.key.attachment.clone(),
            kind,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        summary_from_sets, updated_personal_bests, BestKind, PersonalBestItem, SessionSet,
        SessionSource, TrainingSessionSummary,
    };
    use crate::exercise::Laterality;
    use crate::jst::JstDate;
    use crate::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};

    /// 対応する要求: `REQ-REC-005`。
    /// ウォームアップ、ドロップセット、左右別、時間で記録する種目を含む固定の記録で、
    /// サマリーの 5 項目と、今回のセッションのベストを確かめる。
    #[test]
    fn req_rec_005_summary_counts_the_fixed_record() {
        let summary = summary_from_sets(date(), 3_600, &fixed_sets());

        assert_eq!(summary.duration_seconds, 3_600);
        assert!((summary.total_weight_kg - 1_878.0).abs() < 1e-9);
        assert_eq!(summary.exercise_count, 3);
        assert_eq!(summary.set_count, 4);
        let bests = summary.personal_bests;
        assert_eq!(bests.len(), 7);
        assert_best(
            &bests[0],
            "bench-press",
            "ベンチプレス",
            BestKind::MaxWeight,
            100.0,
        );
        assert_best(
            &bests[1],
            "bench-press",
            "ベンチプレス",
            BestKind::EstimatedOneRepMax,
            127.67141908045373,
        );
        assert_best(
            &bests[2],
            "bench-press",
            "ベンチプレス",
            BestKind::TotalWeight,
            1_780.0,
        );
        assert_best(
            &bests[3],
            "curl",
            "ダンベルカール",
            BestKind::MaxWeight,
            12.0,
        );
        assert_best(
            &bests[4],
            "curl",
            "ダンベルカール",
            BestKind::EstimatedOneRepMax,
            13.535459817722467,
        );
        assert_best(
            &bests[5],
            "curl",
            "ダンベルカール",
            BestKind::TotalWeight,
            98.0,
        );
        assert_best(
            &bests[6],
            "plank",
            "プランク",
            BestKind::LongestDuration,
            90.0,
        );
    }

    #[test]
    fn summary_of_no_sets_is_all_zero() {
        assert_eq!(
            summary_from_sets(date(), 0, &[]),
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
        let summary = summary_from_sets(
            date(),
            60,
            &[session_set("スクワット", warmup_record("squat"))],
        );
        assert_eq!(summary.exercise_count, 1);
        assert_eq!(summary.set_count, 0);
        assert!(summary.personal_bests.is_empty());
    }

    /// 対応する要求: `REQ-REC-005`。
    /// ウォームアップの時間セットは、最長時間の自己ベストの対象にしない。
    #[test]
    fn req_rec_005_a_warmup_time_set_does_not_create_a_session_best() {
        let record = SetRecord::new(
            key("plank"),
            SetKind::Warmup,
            Vec::new(),
            None,
            Some(90),
            RecordMode::Time,
        )
        .expect("a valid warmup time set");
        let summary = summary_from_sets(date(), 60, &[session_set("プランク", record)]);
        assert_eq!(summary.exercise_count, 1);
        assert_eq!(summary.set_count, 0);
        assert!(summary.personal_bests.is_empty());
    }

    #[test]
    fn an_updated_best_needs_to_exceed_the_past_value() {
        let session = vec![
            best("bench", BestKind::MaxWeight, 100.0),
            best("bench", BestKind::TotalWeight, 1_640.0),
            best("plank", BestKind::LongestDuration, 60.0),
        ];
        let past = vec![
            best("bench", BestKind::MaxWeight, 105.0),
            best("bench", BestKind::TotalWeight, 1_500.0),
        ];

        assert_eq!(
            updated_personal_bests(&session, &past),
            vec![
                best("bench", BestKind::TotalWeight, 1_640.0),
                best("plank", BestKind::LongestDuration, 60.0),
            ]
        );
    }

    #[test]
    fn an_updated_best_ignores_a_different_grip() {
        let session = vec![grip_best(
            "bench",
            Some("pronated_medium"),
            BestKind::MaxWeight,
            100.0,
        )];
        let past = vec![
            grip_best(
                "bench",
                Some("supinated_medium"),
                BestKind::MaxWeight,
                120.0,
            ),
            grip_best("bench", None, BestKind::MaxWeight, 130.0),
        ];

        assert_eq!(
            updated_personal_bests(&session, &past),
            vec![grip_best(
                "bench",
                Some("pronated_medium"),
                BestKind::MaxWeight,
                100.0
            )]
        );
    }

    #[test]
    fn the_best_key_includes_laterality_and_record_mode() {
        let base = best("bench", BestKind::MaxWeight, 100.0);
        let mut unilateral = base.clone();
        unilateral.laterality = Laterality::Unilateral;
        let mut time = base.clone();
        time.record_mode = RecordMode::Time;

        assert!(super::same_best_key(&base, &base));
        assert!(!super::same_best_key(&base, &unilateral));
        assert!(!super::same_best_key(&base, &time));
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

    #[test]
    fn best_kinds_round_trip_through_the_stored_string() {
        for kind in BestKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
            assert_eq!(
                serde_json::from_value::<BestKind>(serde_json::json!(kind.as_str())).unwrap(),
                kind
            );
        }
    }

    fn date() -> JstDate {
        JstDate::new(2026, 11, 12).unwrap()
    }

    fn best(exercise_id: &str, kind: BestKind, value: f64) -> PersonalBestItem {
        PersonalBestItem {
            exercise_id: exercise_id.to_owned(),
            exercise_name: Some(format!("{exercise_id}の種目")),
            record_mode: if kind == BestKind::LongestDuration {
                RecordMode::Time
            } else {
                RecordMode::WeightAndReps
            },
            laterality: Laterality::Bilateral,
            grip: None,
            attachment: None,
            kind,
            value,
        }
    }

    fn grip_best(
        exercise_id: &str,
        grip: Option<&str>,
        kind: BestKind,
        value: f64,
    ) -> PersonalBestItem {
        PersonalBestItem {
            grip: grip.map(str::to_owned),
            ..best(exercise_id, kind, value)
        }
    }

    fn assert_best(
        item: &PersonalBestItem,
        exercise_id: &str,
        exercise_name: &str,
        kind: BestKind,
        value: f64,
    ) {
        assert_eq!(item.exercise_id, exercise_id);
        assert_eq!(item.exercise_name.as_deref(), Some(exercise_name));
        assert_eq!(item.kind, kind);
        assert_eq!(item.laterality, Laterality::Bilateral);
        assert_eq!(item.grip, None);
        assert_eq!(item.attachment, None);
        assert!(
            (item.value - value).abs() < 1e-9,
            "{kind:?} must be {value} but was {}",
            item.value
        );
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

    fn warmup_record(exercise_id: &str) -> SetRecord {
        SetRecord::new(
            key(exercise_id),
            SetKind::Warmup,
            vec![segment(40.0, 10)],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid record")
    }

    fn session_set(name: &str, record: SetRecord) -> SessionSet {
        SessionSet {
            record,
            exercise_name: name.to_owned(),
            laterality: Laterality::Bilateral,
        }
    }

    fn fixed_sets() -> Vec<SessionSet> {
        let bench = key("bench-press");
        let curl = key("curl");
        let plank = key("plank");
        vec![
            session_set(
                "ベンチプレス",
                SetRecord::new(
                    bench.clone(),
                    SetKind::Warmup,
                    vec![segment(60.0, 10)],
                    None,
                    None,
                    RecordMode::WeightAndReps,
                )
                .expect("a valid warmup"),
            ),
            session_set(
                "ベンチプレス",
                SetRecord::new(
                    bench.clone(),
                    SetKind::Normal,
                    vec![segment(100.0, 5)],
                    Some(8.0),
                    None,
                    RecordMode::WeightAndReps,
                )
                .expect("a valid normal set"),
            ),
            session_set(
                "ベンチプレス",
                SetRecord::new(
                    bench,
                    SetKind::Drop,
                    vec![segment(100.0, 8), segment(80.0, 6)],
                    None,
                    None,
                    RecordMode::WeightAndReps,
                )
                .expect("a valid drop set"),
            ),
            session_set(
                "ダンベルカール",
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
            ),
            session_set(
                "プランク",
                SetRecord::new(
                    plank,
                    SetKind::Normal,
                    Vec::new(),
                    Some(9.0),
                    Some(90),
                    RecordMode::Time,
                )
                .expect("a valid time set"),
            ),
        ]
    }
}
