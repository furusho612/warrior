//! 種目別の履歴の並び、推移のグラフの期間と指標、同じ日の点のまとめ (PRD の「分析」)。
//!
//! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`、
//! `REQ-GOAL-002`。
//! グラフの値は `set_calc` の `SetRecord` の計算だけを使い、この module で再実装しない。
//! 最大重量と推定 1RM はドロップセットの最初の段、総重量は全段という規則も
//! `SetRecord` が持つ。この module は期間の選択と、同じ日の記録のまとめだけを担う。
//! D1 からの読み込みと応答の組み立ては Worker の `history` が行う。

use serde::{Deserialize, Serialize};

use crate::calendar::CalendarSet;
use crate::exercise::{GripMode, GripSelection, Laterality};
use crate::goal::GoalProgress;
use crate::jst::{JstDate, UtcInstant};
use crate::set_calc::{RecordMode, SetKind, SetRecord};
use crate::training_session::BestKind;

/// グラフに含める記録の期間。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphPeriod {
    /// 表示の日から 1 か月。
    OneMonth,
    /// 表示の日から 3 か月。
    ThreeMonths,
    /// 表示の日から 6 か月。
    SixMonths,
    /// 表示の日から 1 年。
    OneYear,
    /// 全ての記録。
    All,
}

impl GraphPeriod {
    /// 5 種類。画面の選択肢の並びである。
    pub const ALL: [Self; 5] = [
        Self::OneMonth,
        Self::ThreeMonths,
        Self::SixMonths,
        Self::OneYear,
        Self::All,
    ];

    /// API のクエリに載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneMonth => "1m",
            Self::ThreeMonths => "3m",
            Self::SixMonths => "6m",
            Self::OneYear => "1y",
            Self::All => "all",
        }
    }

    /// API のクエリの値を期間に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|period| period.as_str() == text)
    }

    /// その日から期間を引いた始まりの日。全期間は始まりを持たない。
    ///
    /// 引いた先の月に同じ日が無い場合は、その月の末日を始まりにする。
    pub fn start(self, today: JstDate) -> Option<JstDate> {
        match self {
            Self::OneMonth => today.subtract_months(1),
            Self::ThreeMonths => today.subtract_months(3),
            Self::SixMonths => today.subtract_months(6),
            Self::OneYear => today.subtract_months(12),
            Self::All => None,
        }
    }

    /// 期間に含まれるか。期間の終わりは表示の日である。
    pub fn contains(self, today: JstDate, date: JstDate) -> bool {
        if date > today {
            return false;
        }
        self.start(today).is_none_or(|start| date >= start)
    }
}

/// グラフの指標。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphMetric {
    /// その日のセットの最大重量。
    MaxWeight,
    /// その日のセットの推定 1RM の最大値。
    EstimatedOneRepMax,
    /// その日の種目別の総重量の合計。
    TotalWeight,
    /// その日のセットの最長時間。
    LongestDuration,
    /// その日の合計時間。
    TotalDuration,
}

impl GraphMetric {
    /// 重量と回数の種目で選べる指標。
    pub const WEIGHT_METRICS: [Self; 3] =
        [Self::MaxWeight, Self::EstimatedOneRepMax, Self::TotalWeight];

    /// 時間で記録する種目で選べる指標。
    pub const TIME_METRICS: [Self; 2] = [Self::LongestDuration, Self::TotalDuration];

    /// API のクエリに載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxWeight => "max_weight",
            Self::EstimatedOneRepMax => "estimated_one_rep_max",
            Self::TotalWeight => "total_weight",
            Self::LongestDuration => "longest_duration",
            Self::TotalDuration => "total_duration",
        }
    }

    /// API のクエリの値を指標に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        Self::WEIGHT_METRICS
            .into_iter()
            .chain(Self::TIME_METRICS)
            .find(|metric| metric.as_str() == text)
    }

    /// 記録方式ごとに選べる指標。
    pub fn allowed_for(mode: RecordMode) -> &'static [Self] {
        match mode {
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => &Self::WEIGHT_METRICS,
            RecordMode::Time => &Self::TIME_METRICS,
        }
    }

    /// その記録方式で選べる指標か。
    pub fn is_allowed_for(self, mode: RecordMode) -> bool {
        Self::allowed_for(mode).contains(&self)
    }

    /// 同じ日の複数の記録を 1 点にまとめる方法。
    pub fn aggregation(self) -> MetricAggregation {
        match self {
            Self::MaxWeight | Self::EstimatedOneRepMax | Self::LongestDuration => {
                MetricAggregation::Max
            }
            Self::TotalWeight | Self::TotalDuration => MetricAggregation::Sum,
        }
    }
}

/// 同じ日の複数の記録を 1 点にまとめる方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricAggregation {
    /// 記録の最大値を取る。
    Max,
    /// 記録の合計を取る。
    Sum,
}

/// グラフの 1 点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPoint {
    /// 日本時間の日付。
    pub date: JstDate,
    /// その日の指標の値。時間の指標は秒を表す。
    pub value: f64,
}

/// グラフに渡す、日付を持つ 1 セットの記録。
#[derive(Debug, Clone, Copy)]
pub struct GraphSet<'a> {
    /// トレーニングの日本時間の日付。
    pub date: JstDate,
    /// 完了したセットの記録。
    pub record: &'a SetRecord,
}

/// 種目別のグラフの応答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExerciseGraph {
    /// 種目の ID。
    pub exercise_id: String,
    /// 選んだ期間。
    pub period: GraphPeriod,
    /// 選んだ指標。
    pub metric: GraphMetric,
    /// 期間の始まり。全期間は None。
    pub start: Option<JstDate>,
    /// 表示の日。期間の終わりである。
    pub end: JstDate,
    /// 日付の古い順の点。記録が無い日は含まない。
    pub points: Vec<GraphPoint>,
    /// 目標重量と現在の記録との差。絞り込みに一致する組だけを持つ。
    pub goals: Vec<GoalProgress>,
}

/// 種目別の履歴の 1 トレーニング。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryTraining {
    /// トレーニングの ID。
    pub training_id: String,
    /// 日本時間の日付。
    pub date: JstDate,
    /// 終了した時刻の ISO 8601 の UTC の文字列。
    pub finished_at: String,
    /// このトレーニングで記録したセット。記録した順である。
    pub sets: Vec<CalendarSet>,
}

/// 種目別の自己ベストの 1 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExerciseBest {
    /// 自己ベストの種類。
    pub kind: BestKind,
    /// 値。時間で記録する種目は秒を表す。
    pub value: f64,
    /// 達成日 (日本時間)。
    pub achieved_on: JstDate,
    /// グリップの比較のキー。選ばない種目は None。
    pub grip: Option<String>,
    /// アタッチメントの比較のキー。選ばない種目は None。
    pub attachment: Option<String>,
}

/// 種目別の履歴の応答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExerciseHistory {
    /// 種目の ID。
    pub exercise_id: String,
    /// 表示する種目名。
    pub name: String,
    /// 記録方式。
    pub record_mode: RecordMode,
    /// 左右別に行うか。
    pub laterality: Laterality,
    /// グリップの指定の種類。
    pub grip_mode: GripMode,
    /// 種目が持つグリップの選択肢。
    pub grip_options: Vec<GripSelection>,
    /// 自己ベスト。指定したグリップとアタッチメントで絞り込む。
    pub bests: Vec<ExerciseBest>,
    /// 目標重量と現在の記録との差。指定したグリップとアタッチメントで絞り込む。
    pub goals: Vec<GoalProgress>,
    /// トレーニング。日付の新しい順で、同じ日は終了の時刻の新しい順である。
    pub trainings: Vec<HistoryTraining>,
}

/// 履歴とグラフを絞り込むグリップとアタッチメント。
///
/// 値は `GripSelection::comparison_key` の組である。指定の無い項目は None にする。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GripFilter {
    /// グリップの比較のキー。アタッチメントを選ぶ種目では順手か逆手か。
    pub grip: Option<String>,
    /// アタッチメントの比較のキー。
    pub attachment: Option<String>,
}

impl GripFilter {
    /// 空の文字列を指定なしとして組み立てる。
    pub fn new(grip: Option<String>, attachment: Option<String>) -> Self {
        Self {
            grip: non_empty(grip),
            attachment: non_empty(attachment),
        }
    }

    /// 指定が 1 つも無いか。
    pub fn is_empty(&self) -> bool {
        self.grip.is_none() && self.attachment.is_none()
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

/// セットまたは自己ベストが、種目のグリップの種類と指定に一致するか。
///
/// グリップを選ばない種目は、指定があっても条件に加えない。握り方と手幅を選ぶ種目は
/// グリップの比較のキーだけを、アタッチメントを選ぶ種目はグリップとアタッチメントの
/// 指定された方を条件にする (REQ-SET-011)。
pub fn matches_grip(
    mode: GripMode,
    filter: &GripFilter,
    grip: Option<&str>,
    attachment: Option<&str>,
) -> bool {
    match mode {
        GripMode::None => true,
        GripMode::Grip => filter.grip.as_deref().is_none_or(|key| grip == Some(key)),
        GripMode::Attachment => {
            filter.grip.as_deref().is_none_or(|key| grip == Some(key))
                && filter
                    .attachment
                    .as_deref()
                    .is_none_or(|key| attachment == Some(key))
        }
    }
}

/// 期間に含まれる記録から、日付ごとのグラフの点を作る。
///
/// 同じ日の複数の記録は、最大値を取る指標では最大値、合計を取る指標では合計にまとめる。
/// ウォームアップセットはどの指標にも含めない。時間で記録する種目の重量の指標と、
/// 重量と回数の種目の時間の指標は値を取らない。点は日付の古い順に並べる。
pub fn graph_points(
    period: GraphPeriod,
    today: JstDate,
    metric: GraphMetric,
    sets: &[GraphSet<'_>],
) -> Vec<GraphPoint> {
    let mut days = Vec::<(JstDate, f64)>::new();
    for set in sets {
        if !period.contains(today, set.date) {
            continue;
        }
        let Some(value) = metric_value(metric, set.record) else {
            continue;
        };
        let index = match days.iter().position(|(date, _)| *date == set.date) {
            Some(index) => index,
            None => {
                days.push((set.date, value));
                continue;
            }
        };
        let current = &mut days[index].1;
        *current = match metric.aggregation() {
            MetricAggregation::Max => current.max(value),
            MetricAggregation::Sum => *current + value,
        };
    }
    days.sort_by_key(|(date, _)| *date);
    days.into_iter()
        .map(|(date, value)| GraphPoint { date, value })
        .collect()
}

/// セット 1 つが指標に与える値。対象外のセットは None。
fn metric_value(metric: GraphMetric, record: &SetRecord) -> Option<f64> {
    if record.kind() == SetKind::Warmup {
        return None;
    }
    match metric {
        GraphMetric::MaxWeight => record.max_weight_kg(),
        GraphMetric::EstimatedOneRepMax => record.estimated_one_rep_max(),
        GraphMetric::TotalWeight => {
            (record.mode() != RecordMode::Time).then(|| record.total_weight_kg())
        }
        GraphMetric::LongestDuration => record.longest_duration_seconds().map(f64::from),
        GraphMetric::TotalDuration => {
            (record.mode() == RecordMode::Time).then(|| f64::from(record.total_duration_seconds()))
        }
    }
}

/// 履歴を、日付の新しい順、同じ日は終了の時刻の新しい順に並べる。
///
/// 終了の時刻もトレーニングの ID も同じ場合は、ID の大きい方を先にして順序を固定する。
pub fn sort_trainings(trainings: &mut [HistoryTraining]) {
    trainings.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| {
                finished_order(&right.finished_at).cmp(&finished_order(&left.finished_at))
            })
            .then_with(|| right.training_id.cmp(&left.training_id))
    });
}

fn finished_order(text: &str) -> UtcInstant {
    UtcInstant::parse(text).unwrap_or(UtcInstant::from_epoch_seconds(i64::MIN))
}

/// 記録方式ごとに表示と保存の対象にする自己ベストの種類。
///
/// 時間で記録する種目は、左右を区別しない最長時間だけを持つ (REQ-ANL-004)。
pub fn best_kinds(mode: RecordMode) -> &'static [BestKind] {
    match mode {
        RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => &[
            BestKind::MaxWeight,
            BestKind::EstimatedOneRepMax,
            BestKind::TotalWeight,
        ],
        RecordMode::Time => &[BestKind::LongestDuration],
    }
}

#[cfg(test)]
mod tests {
    //! 履歴の並びとグラフの集計の例示テスト。
    //!
    //! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`。
    //! 任意入力の性質は `backend/pbt/tests/history.rs` が担う。ここは 5 つの期間、
    //! 5 つの指標、時間で記録する種目、同じ日の複数の記録、記録がない期間を固定する。

    use super::{
        best_kinds, graph_points, matches_grip, sort_trainings, ExerciseBest, GraphMetric,
        GraphPeriod, GraphSet, GripFilter, HistoryTraining,
    };
    use crate::exercise::{GripMode, GripSelection, Laterality};
    use crate::jst::JstDate;
    use crate::set_calc::{
        estimated_one_rep_max, BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue,
    };
    use crate::training_session::BestKind;

    const EXERCISE_ID: &str = "bench";

    #[test]
    fn the_five_periods_start_at_the_month_before_the_displayed_day() {
        let today = date(2026, 11, 12);
        assert_eq!(GraphPeriod::OneMonth.start(today), Some(date(2026, 10, 12)));
        assert_eq!(
            GraphPeriod::ThreeMonths.start(today),
            Some(date(2026, 8, 12))
        );
        assert_eq!(GraphPeriod::SixMonths.start(today), Some(date(2026, 5, 12)));
        assert_eq!(GraphPeriod::OneYear.start(today), Some(date(2025, 11, 12)));
        assert_eq!(GraphPeriod::All.start(today), None);
        assert_eq!(GraphPeriod::ALL.len(), 5);
    }

    #[test]
    fn every_period_parses_from_its_query_value() {
        for period in GraphPeriod::ALL {
            assert_eq!(GraphPeriod::parse(period.as_str()), Some(period));
        }
        assert_eq!(GraphPeriod::parse("2m"), None);
        assert_eq!(GraphPeriod::parse(""), None);
    }

    #[test]
    fn a_period_excludes_the_days_before_its_start_and_after_today() {
        let today = date(2026, 11, 12);
        let first = date(2026, 10, 12);
        let before = date(2026, 10, 11);
        let after = date(2026, 11, 13);
        assert!(GraphPeriod::OneMonth.contains(today, first));
        assert!(GraphPeriod::OneMonth.contains(today, today));
        assert!(!GraphPeriod::OneMonth.contains(today, before));
        assert!(!GraphPeriod::OneMonth.contains(today, after));
        assert!(GraphPeriod::All.contains(today, date(2000, 1, 1)));
        assert!(!GraphPeriod::All.contains(today, after));
    }

    #[test]
    fn the_five_metrics_parse_from_their_query_values() {
        for metric in GraphMetric::WEIGHT_METRICS
            .into_iter()
            .chain(GraphMetric::TIME_METRICS)
        {
            assert_eq!(GraphMetric::parse(metric.as_str()), Some(metric));
        }
        assert_eq!(GraphMetric::parse("average_weight"), None);
        assert_eq!(
            GraphMetric::allowed_for(RecordMode::WeightAndReps),
            &GraphMetric::WEIGHT_METRICS
        );
        assert_eq!(
            GraphMetric::allowed_for(RecordMode::Time),
            &GraphMetric::TIME_METRICS
        );
        assert!(GraphMetric::MaxWeight.is_allowed_for(RecordMode::BodyweightAndReps));
        assert!(!GraphMetric::MaxWeight.is_allowed_for(RecordMode::Time));
        assert!(!GraphMetric::TotalDuration.is_allowed_for(RecordMode::WeightAndReps));
    }

    #[test]
    fn a_weight_exercise_point_uses_the_first_drop_segment_for_max_and_one_rep_max() {
        let today = date(2026, 11, 12);
        let record = weight_record(SetKind::Drop, vec![segment(100.0, 8), segment(120.0, 5)]);
        let sets = [dated(today, &record)];
        let max = graph_points(GraphPeriod::OneMonth, today, GraphMetric::MaxWeight, &sets);
        let one_rep_max = graph_points(
            GraphPeriod::OneMonth,
            today,
            GraphMetric::EstimatedOneRepMax,
            &sets,
        );
        let total = graph_points(
            GraphPeriod::OneMonth,
            today,
            GraphMetric::TotalWeight,
            &sets,
        );
        assert_eq!(
            max,
            vec![super::GraphPoint {
                date: today,
                value: 100.0
            }]
        );
        assert_eq!(
            one_rep_max[0].value,
            estimated_one_rep_max(100.0, 8).expect("the formula")
        );
        assert_eq!(total[0].value, 800.0 + 600.0);
    }

    #[test]
    fn the_breakdown_reps_change_the_one_rep_max_and_the_total() {
        let today = date(2026, 11, 12);
        let mut segment = segment(100.0, 5);
        segment.momentum_reps = 1;
        segment.assisted_reps = 1;
        let record = weight_record(SetKind::Normal, vec![segment]);
        let sets = [dated(today, &record)];
        let one_rep_max = graph_points(
            GraphPeriod::All,
            today,
            GraphMetric::EstimatedOneRepMax,
            &sets,
        );
        let total = graph_points(GraphPeriod::All, today, GraphMetric::TotalWeight, &sets);
        assert_eq!(
            one_rep_max[0].value,
            estimated_one_rep_max(100.0, 3).expect("the formula")
        );
        assert_eq!(total[0].value, 400.0);
    }

    #[test]
    fn a_unilateral_weight_point_uses_the_larger_side_and_the_side_total() {
        let today = date(2026, 11, 12);
        let record = weight_record(
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
        );
        let sets = [dated(today, &record)];
        let max = graph_points(GraphPeriod::All, today, GraphMetric::MaxWeight, &sets);
        let one_rep_max = graph_points(
            GraphPeriod::All,
            today,
            GraphMetric::EstimatedOneRepMax,
            &sets,
        );
        let total = graph_points(GraphPeriod::All, today, GraphMetric::TotalWeight, &sets);
        assert_eq!(max[0].value, 12.0);
        assert_eq!(
            one_rep_max[0].value,
            estimated_one_rep_max(12.0, 4).expect("the formula")
        );
        assert_eq!(total[0].value, 50.0 + 48.0);
    }

    #[test]
    fn warmup_sets_are_excluded_from_every_metric() {
        let today = date(2026, 11, 12);
        let warmup = weight_record(SetKind::Warmup, vec![segment(60.0, 10)]);
        let sets = [dated(today, &warmup)];
        for metric in GraphMetric::WEIGHT_METRICS {
            assert!(
                graph_points(GraphPeriod::All, today, metric, &sets).is_empty(),
                "{metric:?} must not use a warmup"
            );
        }
    }

    #[test]
    fn a_time_exercise_uses_the_longest_and_the_total() {
        let today = date(2026, 11, 12);
        let record = time_record(seconds_segment(60, 90));
        let sets = [dated(today, &record)];
        assert_eq!(
            graph_points(GraphPeriod::All, today, GraphMetric::LongestDuration, &sets),
            vec![super::GraphPoint {
                date: today,
                value: 90.0
            }]
        );
        assert_eq!(
            graph_points(GraphPeriod::All, today, GraphMetric::TotalDuration, &sets),
            vec![super::GraphPoint {
                date: today,
                value: 150.0
            }]
        );
        assert!(
            graph_points(GraphPeriod::All, today, GraphMetric::MaxWeight, &sets).is_empty(),
            "a time exercise has no weight"
        );
    }

    #[test]
    fn the_same_day_is_one_point_with_the_max_or_the_sum() {
        let first = date(2026, 11, 10);
        let later = date(2026, 11, 11);
        let hundred = weight_record(SetKind::Normal, vec![segment(100.0, 5)]);
        let eighty = weight_record(SetKind::Normal, vec![segment(80.0, 10)]);
        let sets = [
            dated(first, &hundred),
            dated(first, &eighty),
            dated(later, &hundred),
        ];
        let max = graph_points(GraphPeriod::All, later, GraphMetric::MaxWeight, &sets);
        let total = graph_points(GraphPeriod::All, later, GraphMetric::TotalWeight, &sets);
        assert_eq!(
            max,
            vec![
                super::GraphPoint {
                    date: first,
                    value: 100.0,
                },
                super::GraphPoint {
                    date: later,
                    value: 100.0,
                },
            ]
        );
        assert_eq!(
            total,
            vec![
                super::GraphPoint {
                    date: first,
                    value: 500.0 + 800.0,
                },
                super::GraphPoint {
                    date: later,
                    value: 500.0,
                },
            ]
        );
    }

    #[test]
    fn a_period_without_records_has_no_points() {
        let today = date(2026, 11, 12);
        let old = date(2026, 9, 1);
        let record = weight_record(SetKind::Normal, vec![segment(100.0, 5)]);
        let sets = [dated(old, &record)];
        assert!(
            graph_points(GraphPeriod::OneMonth, today, GraphMetric::MaxWeight, &sets).is_empty()
        );
        assert_eq!(
            graph_points(
                GraphPeriod::ThreeMonths,
                today,
                GraphMetric::MaxWeight,
                &sets
            )
            .len(),
            1
        );
    }

    #[test]
    fn the_history_is_ordered_by_the_date_then_the_finished_time() {
        let mut trainings = vec![
            training("training-a", date(2026, 11, 10), "2026-11-10T05:00:00Z"),
            training("training-b", date(2026, 11, 12), "2026-11-12T03:00:00Z"),
            training("training-c", date(2026, 11, 12), "2026-11-12T06:00:00Z"),
            training("training-d", date(2026, 11, 12), "2026-11-12T06:00:00Z"),
        ];
        sort_trainings(&mut trainings);
        assert_eq!(
            trainings
                .iter()
                .map(|training| training.training_id.as_str())
                .collect::<Vec<_>>(),
            vec!["training-d", "training-c", "training-b", "training-a"]
        );
    }

    #[test]
    fn a_gripless_exercise_ignores_the_filter() {
        let filter = GripFilter::new(Some("pronated_medium".to_owned()), None);
        assert!(matches_grip(GripMode::None, &filter, None, None));
        assert!(matches_grip(GripMode::None, &filter, Some("other"), None));
    }

    #[test]
    fn a_grip_exercise_filters_by_the_whole_grip_key() {
        let filter = GripFilter::new(
            Some("pronated_medium".to_owned()),
            Some("wide_bar".to_owned()),
        );
        assert!(matches_grip(
            GripMode::Grip,
            &filter,
            Some("pronated_medium"),
            None
        ));
        assert!(!matches_grip(
            GripMode::Grip,
            &filter,
            Some("pronated_wide"),
            None
        ));
        assert!(matches_grip(
            GripMode::Grip,
            &GripFilter::default(),
            None,
            None
        ));
    }

    #[test]
    fn an_attachment_exercise_filters_by_the_style_and_the_attachment() {
        let style = GripFilter::new(Some("supinated".to_owned()), None);
        let attachment = GripFilter::new(None, Some("wide_bar".to_owned()));
        let both = GripFilter::new(Some("supinated".to_owned()), Some("wide_bar".to_owned()));
        assert!(matches_grip(
            GripMode::Attachment,
            &style,
            Some("supinated"),
            Some("straight_bar")
        ));
        assert!(!matches_grip(
            GripMode::Attachment,
            &style,
            Some("pronated"),
            Some("wide_bar")
        ));
        assert!(matches_grip(
            GripMode::Attachment,
            &attachment,
            Some("supinated"),
            Some("wide_bar")
        ));
        assert!(!matches_grip(
            GripMode::Attachment,
            &attachment,
            Some("supinated"),
            Some("straight_bar")
        ));
        assert!(matches_grip(
            GripMode::Attachment,
            &both,
            Some("supinated"),
            Some("wide_bar")
        ));
        assert!(!matches_grip(
            GripMode::Attachment,
            &both,
            Some("pronated"),
            Some("wide_bar")
        ));
    }

    #[test]
    fn the_empty_filter_values_are_no_filter() {
        assert_eq!(
            GripFilter::new(Some(String::new()), None),
            GripFilter::default()
        );
        assert!(GripFilter::new(None, None).is_empty());
        assert!(!GripFilter::new(Some("pronated_medium".to_owned()), None).is_empty());
    }

    #[test]
    fn the_best_kinds_follow_the_record_mode() {
        assert_eq!(
            best_kinds(RecordMode::WeightAndReps),
            &[
                BestKind::MaxWeight,
                BestKind::EstimatedOneRepMax,
                BestKind::TotalWeight,
            ]
        );
        assert_eq!(best_kinds(RecordMode::Time), &[BestKind::LongestDuration]);
    }

    #[test]
    fn every_grip_selection_has_a_comparison_key_for_the_filter() {
        let selection = GripSelection::Grip {
            style: crate::exercise::GripStyle::Pronated,
            width: crate::exercise::GripWidth::Medium,
        };
        let (grip, attachment) = selection.comparison_key();
        let filter = GripFilter::new(grip.clone(), attachment);
        assert!(matches_grip(GripMode::Grip, &filter, grip.as_deref(), None));
    }

    #[test]
    fn a_best_carries_its_achievement_date_and_key() {
        let best = ExerciseBest {
            kind: BestKind::LongestDuration,
            value: 90.0,
            achieved_on: date(2026, 11, 12),
            grip: None,
            attachment: None,
        };
        let filter = GripFilter::default();
        assert!(matches_grip(
            GripMode::None,
            &filter,
            best.grip.as_deref(),
            best.attachment.as_deref()
        ));
        let json = serde_json::to_string(&best).expect("a serializable best");
        assert_eq!(
            serde_json::from_str::<ExerciseBest>(&json).expect("a readable best"),
            best
        );
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }

    fn key() -> BestKey {
        BestKey {
            exercise_id: EXERCISE_ID.to_owned(),
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

    fn seconds_segment(left: u32, right: u32) -> SetSegment {
        SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(left),
            }),
            right: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(right),
            }),
            duration_seconds: None,
        }
    }

    fn weight_record(kind: SetKind, segments: Vec<SetSegment>) -> SetRecord {
        SetRecord::new(key(), kind, segments, None, None, RecordMode::WeightAndReps)
            .expect("a valid weight set")
    }

    fn time_record(segment: SetSegment) -> SetRecord {
        SetRecord::new(
            key(),
            SetKind::Normal,
            vec![segment],
            None,
            None,
            RecordMode::Time,
        )
        .expect("a valid time set")
    }

    fn dated(date: JstDate, record: &SetRecord) -> GraphSet<'_> {
        GraphSet { date, record }
    }

    fn training(training_id: &str, date: JstDate, finished_at: &str) -> HistoryTraining {
        HistoryTraining {
            training_id: training_id.to_owned(),
            date,
            finished_at: finished_at.to_owned(),
            sets: Vec::new(),
        }
    }

    #[test]
    fn the_history_laterality_and_grip_options_round_trip_through_serde() {
        let history = super::ExerciseHistory {
            exercise_id: EXERCISE_ID.to_owned(),
            name: "ベンチプレス".to_owned(),
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode: GripMode::Grip,
            grip_options: vec![GripSelection::Grip {
                style: crate::exercise::GripStyle::Pronated,
                width: crate::exercise::GripWidth::Medium,
            }],
            bests: Vec::new(),
            goals: Vec::new(),
            trainings: vec![training(
                EXERCISE_ID,
                date(2026, 11, 12),
                "2026-11-12T03:00:00Z",
            )],
        };
        let json = serde_json::to_string(&history).expect("a serializable history");
        assert_eq!(
            serde_json::from_str::<super::ExerciseHistory>(&json).expect("a readable history"),
            history
        );
    }
}
