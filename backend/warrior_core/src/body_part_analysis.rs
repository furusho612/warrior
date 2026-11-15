//! 部位区分ごとのセット数と総重量の集計、前の週と前の月との増減 (PRD の「部位別の分析」)。
//!
//! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
//! 週と月の境界は `jst` の `JstWeek` と `JstMonth` に任せ、ADR-005 の規則をここで
//! 再実装しない。セット数と総重量は `set_calc` の `SetRecord` の計算だけを使う。
//! 部位区分は記録時の種目のスナップショットから渡し、現在の種目マスターを参照しない
//! (ADR-006)。D1 からの読み込みと応答の組み立ては Worker の `analysis` が行う。

use serde::{Deserialize, Serialize};

use crate::exercise::BodyPart;
use crate::jst::JstDate;
use crate::set_calc::{PartRole, SetRecord};

/// 分析の集計の単位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPeriod {
    /// 日本時間の月曜 0 時に始まる週。
    Week,
    /// 日本時間の 1 日 0 時に始まる月。
    Month,
}

impl AnalysisPeriod {
    /// 2 種類。画面の選択肢の並びである。
    pub const ALL: [Self; 2] = [Self::Week, Self::Month];

    /// API のクエリに載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Week => "week",
            Self::Month => "month",
        }
    }

    /// API のクエリの値を期間に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|period| period.as_str() == text)
    }

    /// 表示の日を含む期間。
    pub fn range(self, today: JstDate) -> AnalysisRange {
        match self {
            Self::Week => {
                let week = today.week();
                AnalysisRange {
                    start: week.start(),
                    end: week.end(),
                }
            }
            Self::Month => {
                let month = today.month_range();
                AnalysisRange {
                    start: month.start(),
                    end: month.end(),
                }
            }
        }
    }

    /// 表示の期間の直前にある同じ単位の期間。
    pub fn previous_range(self, today: JstDate) -> AnalysisRange {
        match self {
            Self::Week => previous_week(today),
            Self::Month => previous_month(today),
        }
    }
}

fn previous_week(today: JstDate) -> AnalysisRange {
    let start = today
        .week()
        .start()
        .add_days(-7)
        .expect("the previous week is representable");
    AnalysisRange {
        start,
        end: start
            .add_days(6)
            .expect("the previous week is representable"),
    }
}

fn previous_month(today: JstDate) -> AnalysisRange {
    let month = today
        .month_range()
        .start()
        .subtract_months(1)
        .expect("the previous month is representable")
        .month_range();
    AnalysisRange {
        start: month.start(),
        end: month.end(),
    }
}

/// 日本時間の日付の範囲。始まりと終わりを含む。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisRange {
    /// 期間の最初の日。
    pub start: JstDate,
    /// 期間の最後の日。
    pub end: JstDate,
}

impl AnalysisRange {
    /// 日付が範囲に含まれるか。
    pub fn contains(self, date: JstDate) -> bool {
        self.start <= date && date <= self.end
    }
}

/// グラフに表示する指標。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisMetric {
    /// 部位区分ごとのセット数。
    Sets,
    /// 部位区分ごとの総重量。
    Weight,
}

impl AnalysisMetric {
    /// 2 種類。画面の選択肢の並びである。
    pub const ALL: [Self; 2] = [Self::Sets, Self::Weight];

    /// API のクエリに載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sets => "sets",
            Self::Weight => "weight",
        }
    }

    /// API のクエリの値を指標に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|metric| metric.as_str() == text)
    }
}

/// 集計に渡す、日付と部位区分を持つ 1 セット。
#[derive(Debug, Clone, Copy)]
pub struct BodyPartSet<'a> {
    /// トレーニングの日本時間の日付。
    pub date: JstDate,
    /// 記録時の種目のスナップショットの部位区分。
    pub body_part: BodyPart,
    /// 完了したセットの記録。
    pub record: &'a SetRecord,
}

/// 部位区分ごとの集計の 1 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyPartAnalysisEntry {
    /// 部位区分。
    pub body_part: BodyPart,
    /// 表示中の期間のセット数。
    pub sets: u32,
    /// 表示中の期間の総重量 (kg)。
    pub weight_kg: f64,
    /// 直前の期間のセット数。
    pub previous_sets: u32,
    /// 直前の期間の総重量 (kg)。
    pub previous_weight_kg: f64,
    /// 表示中の期間のセット数から直前の期間のセット数を引いた値。
    pub sets_change: i64,
    /// 表示中の期間の総重量から直前の期間の総重量を引いた値 (kg)。
    pub weight_change_kg: f64,
}

impl BodyPartAnalysisEntry {
    /// 選んだ指標の値。
    pub fn value(&self, metric: AnalysisMetric) -> f64 {
        match metric {
            AnalysisMetric::Sets => f64::from(self.sets),
            AnalysisMetric::Weight => self.weight_kg,
        }
    }
}

/// 部位別の分析の応答。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BodyPartAnalysis {
    /// 選んだ集計の単位。
    pub period: AnalysisPeriod,
    /// 選んだグラフの指標。
    pub metric: AnalysisMetric,
    /// 表示中の期間の最初の日。
    pub start: JstDate,
    /// 表示中の期間の最後の日。
    pub end: JstDate,
    /// 直前の期間の最初の日。
    pub previous_start: JstDate,
    /// 直前の期間の最後の日。
    pub previous_end: JstDate,
    /// 部位区分ごとの集計。`BodyPart::ALL` と同じ並びで、記録の無い部位も含む。
    pub parts: Vec<BodyPartAnalysisEntry>,
}

/// 期間ごとのセット数と総重量を部位区分に集め、直前の期間との増減を付ける。
///
/// ウォームアップはセット数に数えず総重量にも含めない。ドロップセットは 1 セットと数え、
/// 総重量には全段を含める。時間で記録する種目は 1 セットと数え、総重量に含めない。
/// 左右別のセットは総重量を左右の合算とする。補助筋の部位には数えない。これらの規則は
/// `SetRecord` の `part_set_count` と `total_weight_kg` が持つ (0012)。
pub fn body_part_analysis(
    period: AnalysisPeriod,
    metric: AnalysisMetric,
    today: JstDate,
    sets: &[BodyPartSet<'_>],
) -> BodyPartAnalysis {
    let range = period.range(today);
    let previous = period.previous_range(today);
    let mut totals = BodyPart::ALL
        .iter()
        .map(|body_part| PartTotal::new(*body_part))
        .collect::<Vec<_>>();
    for set in sets {
        let Some(total) = totals
            .iter_mut()
            .find(|total| total.body_part == set.body_part)
        else {
            continue;
        };
        total.add(set, range, previous);
    }
    BodyPartAnalysis {
        period,
        metric,
        start: range.start,
        end: range.end,
        previous_start: previous.start,
        previous_end: previous.end,
        parts: totals.into_iter().map(PartTotal::entry).collect(),
    }
}

/// 1 つの部位区分の集計の途中の値。
struct PartTotal {
    body_part: BodyPart,
    sets: u32,
    weight_kg: f64,
    previous_sets: u32,
    previous_weight_kg: f64,
}

impl PartTotal {
    fn new(body_part: BodyPart) -> Self {
        Self {
            body_part,
            sets: 0,
            weight_kg: 0.0,
            previous_sets: 0,
            previous_weight_kg: 0.0,
        }
    }

    fn add(&mut self, set: &BodyPartSet<'_>, range: AnalysisRange, previous: AnalysisRange) {
        if range.contains(set.date) {
            self.sets += set.record.part_set_count(PartRole::Primary) as u32;
            self.weight_kg += set.record.total_weight_kg();
        }
        if previous.contains(set.date) {
            self.previous_sets += set.record.part_set_count(PartRole::Primary) as u32;
            self.previous_weight_kg += set.record.total_weight_kg();
        }
    }

    fn entry(self) -> BodyPartAnalysisEntry {
        BodyPartAnalysisEntry {
            body_part: self.body_part,
            sets: self.sets,
            weight_kg: self.weight_kg,
            previous_sets: self.previous_sets,
            previous_weight_kg: self.previous_weight_kg,
            sets_change: i64::from(self.sets) - i64::from(self.previous_sets),
            weight_change_kg: self.weight_kg - self.previous_weight_kg,
        }
    }
}

#[cfg(test)]
mod tests {
    //! 部位別の分析の期間と集計の例示テスト。
    //!
    //! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
    //! 任意入力の性質は `backend/pbt/tests/body_part_analysis.rs` が担う。ここは週と月の
    //! 境界、空の期間、複数の部位の日、ウォームアップ、ドロップセット、左右別、時間で
    //! 記録する種目、補助筋、部位区分「その他」を固定する。

    use super::{
        body_part_analysis, AnalysisMetric, AnalysisPeriod, AnalysisRange, BodyPartAnalysis,
        BodyPartSet,
    };
    use crate::exercise::BodyPart;
    use crate::jst::{JstDate, UtcInstant};
    use crate::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};

    const EXERCISE_ID: &str = "bench";

    #[test]
    fn the_week_and_the_month_follow_the_japan_boundaries() {
        let today = date(2026, 11, 12);
        assert_eq!(
            AnalysisPeriod::Week.range(today),
            range(2026, 11, 9, 2026, 11, 15)
        );
        assert_eq!(
            AnalysisPeriod::Week.previous_range(today),
            range(2026, 11, 2, 2026, 11, 8)
        );
        assert_eq!(
            AnalysisPeriod::Month.range(today),
            range(2026, 11, 1, 2026, 11, 30)
        );
        assert_eq!(
            AnalysisPeriod::Month.previous_range(today),
            range(2026, 10, 1, 2026, 10, 31)
        );
    }

    #[test]
    fn the_week_turns_at_monday_midnight_in_japan() {
        let sunday_last = instant("2026-11-08T14:59:59Z");
        let monday_first = instant("2026-11-08T15:00:00Z");
        let sunday = sunday_last.jst_date().expect("a Japan date");
        let monday = monday_first.jst_date().expect("a Japan date");
        assert_eq!(sunday.week().start(), date(2026, 11, 2));
        assert_eq!(sunday.week().end(), date(2026, 11, 8));
        assert_eq!(monday.week().start(), date(2026, 11, 9));
        assert_eq!(monday.week().end(), date(2026, 11, 15));

        let sunday_record = weight_set(SetKind::Normal, 100.0, 1);
        let monday_record = weight_set(SetKind::Normal, 200.0, 1);
        let sets = [
            BodyPartSet {
                date: sunday,
                body_part: BodyPart::Chest,
                record: &sunday_record,
            },
            BodyPartSet {
                date: monday,
                body_part: BodyPart::Chest,
                record: &monday_record,
            },
        ];
        let analysis =
            body_part_analysis(AnalysisPeriod::Week, AnalysisMetric::Sets, monday, &sets);
        let chest = entry(&analysis, BodyPart::Chest);
        assert_eq!(chest.sets, 1);
        assert_eq!(chest.weight_kg, 200.0);
        assert_eq!(chest.previous_sets, 1);
        assert_eq!(chest.previous_weight_kg, 100.0);
        assert_eq!(chest.sets_change, 0);
        assert_eq!(chest.weight_change_kg, 100.0);
    }

    #[test]
    fn the_month_turns_at_the_first_day_midnight_in_japan() {
        let january_last = instant("2026-01-31T14:59:59Z");
        let february_first = instant("2026-01-31T15:00:00Z");
        let january = january_last.jst_date().expect("a Japan date");
        let february = february_first.jst_date().expect("a Japan date");
        assert_eq!(january.month_range().start(), date(2026, 1, 1));
        assert_eq!(january.month_range().end(), date(2026, 1, 31));
        assert_eq!(february.month_range().start(), date(2026, 2, 1));
        assert_eq!(february.month_range().end(), date(2026, 2, 28));

        let january_record = weight_set(SetKind::Normal, 100.0, 1);
        let february_record = weight_set(SetKind::Normal, 200.0, 1);
        let sets = [
            BodyPartSet {
                date: january,
                body_part: BodyPart::Back,
                record: &january_record,
            },
            BodyPartSet {
                date: february,
                body_part: BodyPart::Back,
                record: &february_record,
            },
        ];
        let analysis =
            body_part_analysis(AnalysisPeriod::Month, AnalysisMetric::Sets, february, &sets);
        let back = entry(&analysis, BodyPart::Back);
        assert_eq!(back.sets, 1);
        assert_eq!(back.weight_kg, 200.0);
        assert_eq!(back.previous_sets, 1);
        assert_eq!(back.previous_weight_kg, 100.0);
    }

    #[test]
    fn the_month_end_and_the_leap_day_stay_in_the_previous_month() {
        let record = weight_set(SetKind::Normal, 100.0, 1);
        let february_end = date(2024, 2, 29);
        let sets = [BodyPartSet {
            date: february_end,
            body_part: BodyPart::Legs,
            record: &record,
        }];
        let analysis = body_part_analysis(
            AnalysisPeriod::Month,
            AnalysisMetric::Sets,
            date(2024, 3, 1),
            &sets,
        );
        assert_eq!(
            AnalysisPeriod::Month.previous_range(date(2024, 3, 1)),
            range(2024, 2, 1, 2024, 2, 29)
        );
        let legs = entry(&analysis, BodyPart::Legs);
        assert_eq!(legs.sets, 0);
        assert_eq!(legs.previous_sets, 1);
        assert_eq!(legs.sets_change, -1);
        assert_eq!(legs.previous_weight_kg, 100.0);
    }

    #[test]
    fn an_empty_period_keeps_every_body_part_at_zero() {
        let analysis = body_part_analysis(
            AnalysisPeriod::Week,
            AnalysisMetric::Sets,
            date(2026, 11, 12),
            &[],
        );
        assert_eq!(analysis.previous_start, date(2026, 11, 2));
        assert_eq!(analysis.previous_end, date(2026, 11, 8));
        assert_eq!(
            analysis
                .parts
                .iter()
                .map(|part| part.body_part)
                .collect::<Vec<_>>(),
            BodyPart::ALL.to_vec()
        );
        for part in &analysis.parts {
            assert_eq!(part.sets, 0, "{:?}", part.body_part);
            assert_eq!(part.weight_kg, 0.0, "{:?}", part.body_part);
            assert_eq!(part.previous_sets, 0, "{:?}", part.body_part);
            assert_eq!(part.previous_weight_kg, 0.0, "{:?}", part.body_part);
            assert_eq!(part.sets_change, 0, "{:?}", part.body_part);
            assert_eq!(part.weight_change_kg, 0.0, "{:?}", part.body_part);
        }
    }

    #[test]
    fn several_body_parts_on_the_same_day_are_counted_separately() {
        let day = date(2026, 11, 12);
        let chest = weight_set(SetKind::Normal, 100.0, 5);
        let back = weight_set(SetKind::Normal, 50.0, 10);
        let sets = [
            BodyPartSet {
                date: day,
                body_part: BodyPart::Chest,
                record: &chest,
            },
            BodyPartSet {
                date: day,
                body_part: BodyPart::Back,
                record: &back,
            },
        ];
        let analysis = body_part_analysis(AnalysisPeriod::Week, AnalysisMetric::Weight, day, &sets);
        let chest = entry(&analysis, BodyPart::Chest);
        assert_eq!(chest.sets, 1);
        assert_eq!(chest.weight_kg, 500.0);
        let back = entry(&analysis, BodyPart::Back);
        assert_eq!(back.sets, 1);
        assert_eq!(back.weight_kg, 500.0);
        assert_eq!(entry(&analysis, BodyPart::Legs).sets, 0);
    }

    #[test]
    fn the_set_rules_follow_the_calculation_definitions() {
        let day = date(2026, 11, 12);
        let warmup = weight_set(SetKind::Warmup, 60.0, 10);
        let drop =
            weight_set_with_segments(SetKind::Drop, vec![segment(100.0, 5), segment(80.0, 8)]);
        let assisted = weight_set_with_breakdown(100.0, 5, 1, 2);
        let unilateral = unilateral_set(10.0, 5, 12.0, 4);
        let time = time_set(Some((30, 40)));
        let other = weight_set(SetKind::Normal, 20.0, 10);
        let sets = [
            BodyPartSet {
                date: day,
                body_part: BodyPart::Chest,
                record: &warmup,
            },
            BodyPartSet {
                date: day,
                body_part: BodyPart::Chest,
                record: &drop,
            },
            BodyPartSet {
                date: day,
                body_part: BodyPart::Chest,
                record: &assisted,
            },
            BodyPartSet {
                date: day,
                body_part: BodyPart::Back,
                record: &unilateral,
            },
            BodyPartSet {
                date: day,
                body_part: BodyPart::Abs,
                record: &time,
            },
            BodyPartSet {
                date: day,
                body_part: BodyPart::Other,
                record: &other,
            },
        ];
        let analysis = body_part_analysis(AnalysisPeriod::Week, AnalysisMetric::Weight, day, &sets);
        let chest = entry(&analysis, BodyPart::Chest);
        assert_eq!(chest.sets, 2, "the warmup is not a set; the drop is one");
        assert_eq!(
            chest.weight_kg,
            500.0 + 640.0 + 300.0,
            "the warmup is excluded and the assisted reps are not counted"
        );
        let back = entry(&analysis, BodyPart::Back);
        assert_eq!(back.sets, 1);
        assert_eq!(back.weight_kg, 50.0 + 48.0, "the sides are added");
        let abs = entry(&analysis, BodyPart::Abs);
        assert_eq!(abs.sets, 1);
        assert_eq!(abs.weight_kg, 0.0, "a time exercise has no weight");
        let other = entry(&analysis, BodyPart::Other);
        assert_eq!(other.sets, 1);
        assert_eq!(other.weight_kg, 200.0);
    }

    #[test]
    fn a_record_outside_the_previous_period_does_not_change_the_values() {
        let today = date(2026, 11, 12);
        let record = weight_set(SetKind::Normal, 100.0, 5);
        let outside = date(2026, 10, 31);
        let sets = [BodyPartSet {
            date: outside,
            body_part: BodyPart::Chest,
            record: &record,
        }];
        let analysis = body_part_analysis(AnalysisPeriod::Week, AnalysisMetric::Sets, today, &sets);
        assert_eq!(entry(&analysis, BodyPart::Chest), zero(BodyPart::Chest));
    }

    #[test]
    fn the_two_periods_and_metrics_parse_from_their_query_values() {
        for period in AnalysisPeriod::ALL {
            assert_eq!(AnalysisPeriod::parse(period.as_str()), Some(period));
        }
        assert_eq!(AnalysisPeriod::parse("year"), None);
        for metric in AnalysisMetric::ALL {
            assert_eq!(AnalysisMetric::parse(metric.as_str()), Some(metric));
        }
        assert_eq!(AnalysisMetric::parse("max_weight"), None);
    }

    #[test]
    fn the_metric_value_selects_the_sets_or_the_weight() {
        let part = super::BodyPartAnalysisEntry {
            body_part: BodyPart::Chest,
            sets: 3,
            weight_kg: 900.0,
            previous_sets: 1,
            previous_weight_kg: 300.0,
            sets_change: 2,
            weight_change_kg: 600.0,
        };
        assert_eq!(part.value(AnalysisMetric::Sets), 3.0);
        assert_eq!(part.value(AnalysisMetric::Weight), 900.0);
    }

    #[test]
    fn the_analysis_round_trips_through_serde() {
        let analysis = body_part_analysis(
            AnalysisPeriod::Month,
            AnalysisMetric::Weight,
            date(2026, 11, 12),
            &[],
        );
        let json = serde_json::to_string(&analysis).expect("a serializable analysis");
        assert_eq!(
            serde_json::from_str::<BodyPartAnalysis>(&json).expect("a readable analysis"),
            analysis
        );
    }

    fn entry(analysis: &BodyPartAnalysis, body_part: BodyPart) -> super::BodyPartAnalysisEntry {
        analysis
            .parts
            .iter()
            .find(|part| part.body_part == body_part)
            .expect("every body part has an entry")
            .clone()
    }

    fn zero(body_part: BodyPart) -> super::BodyPartAnalysisEntry {
        super::BodyPartAnalysisEntry {
            body_part,
            sets: 0,
            weight_kg: 0.0,
            previous_sets: 0,
            previous_weight_kg: 0.0,
            sets_change: 0,
            weight_change_kg: 0.0,
        }
    }

    fn range(
        start_year: i64,
        start_month: u32,
        start_day: u32,
        end_year: i64,
        end_month: u32,
        end_day: u32,
    ) -> AnalysisRange {
        AnalysisRange {
            start: date(start_year, start_month, start_day),
            end: date(end_year, end_month, end_day),
        }
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }

    fn instant(text: &str) -> UtcInstant {
        UtcInstant::parse(text).expect("a valid instant")
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

    fn weight_set(kind: SetKind, weight_kg: f64, reps: u32) -> SetRecord {
        weight_set_with_segments(kind, vec![segment(weight_kg, reps)])
    }

    fn weight_set_with_segments(kind: SetKind, segments: Vec<SetSegment>) -> SetRecord {
        SetRecord::new(key(), kind, segments, None, None, RecordMode::WeightAndReps)
            .expect("a valid weight set")
    }

    fn weight_set_with_breakdown(
        weight_kg: f64,
        reps: u32,
        momentum_reps: u32,
        assisted_reps: u32,
    ) -> SetRecord {
        weight_set_with_segments(
            SetKind::Normal,
            vec![SetSegment {
                weight_kg,
                reps,
                momentum_reps,
                assisted_reps,
                left: None,
                right: None,
                duration_seconds: None,
            }],
        )
    }

    fn unilateral_set(left_kg: f64, left_reps: u32, right_kg: f64, right_reps: u32) -> SetRecord {
        weight_set_with_segments(
            SetKind::Normal,
            vec![SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps: 0,
                left: Some(SideValue {
                    weight_kg: left_kg,
                    reps: left_reps,
                    duration_seconds: None,
                }),
                right: Some(SideValue {
                    weight_kg: right_kg,
                    reps: right_reps,
                    duration_seconds: None,
                }),
                duration_seconds: None,
            }],
        )
    }

    fn time_set(sides: Option<(u32, u32)>) -> SetRecord {
        let segment = match sides {
            Some((left, right)) => SetSegment {
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
            },
            None => SetSegment {
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps: 0,
                left: None,
                right: None,
                duration_seconds: Some(60),
            },
        };
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
}
