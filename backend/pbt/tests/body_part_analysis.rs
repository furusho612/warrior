//! 部位別の分析の集計と増減の性質テスト。
//!
//! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
//! 週単位と月単位の集計が `set_calc` の計算と一致し、期間の外の記録を含まないこと、
//! 増減が表示中の期間と直前の期間の値の差と一致することを確かめる。

use proptest::prelude::*;
use warrior_core::body_part_analysis::{
    body_part_analysis, AnalysisMetric, AnalysisPeriod, AnalysisRange, BodyPartSet,
};
use warrior_core::exercise::BodyPart;
use warrior_core::jst::JstDate;
use warrior_core::set_calc::{
    BestKey, PartRole, RecordMode, SetKind, SetRecord, SetSegment, SideValue,
};

#[path = "support/set.rs"]
mod set_fixture;

const EXERCISE_ID: &str = "property-exercise";

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// 週単位と月単位の集計が、期間に含まれる記録のセット数と総重量に一致する。
    #[test]
    fn req_anl_005_the_aggregation_matches_the_records_in_the_period(
        today in date(),
        period in period(),
        entries in prop::collection::vec((-90i64..=90, body_part(), record()), 0..=40),
    ) {
        let sets = entries
            .iter()
            .map(|(offset, body_part, record)| BodyPartSet {
                date: today.add_days(*offset).expect("a day around today"),
                body_part: *body_part,
                record,
            })
            .collect::<Vec<_>>();
        let analysis = body_part_analysis(period, AnalysisMetric::Sets, today, &sets);
        let range = period.range(today);
        let previous_range = period.previous_range(today);

        prop_assert_eq!(analysis.parts.len(), BodyPart::ALL.len());
        for (entry, body_part) in analysis.parts.iter().zip(BodyPart::ALL.iter()) {
            prop_assert_eq!(entry.body_part, *body_part);
            let current = expected_totals(&sets, *body_part, range);
            let past = expected_totals(&sets, *body_part, previous_range);
            prop_assert_eq!(entry.sets, current.sets, "{:?}", body_part);
            prop_assert!(
                (entry.weight_kg - current.weight_kg).abs() <= 1e-9 * current.weight_kg.abs().max(1.0),
                "{:?} must weigh {} but was {}",
                body_part,
                current.weight_kg,
                entry.weight_kg
            );
            prop_assert_eq!(entry.previous_sets, past.sets, "{:?}", body_part);
            prop_assert!(
                (entry.previous_weight_kg - past.weight_kg).abs()
                    <= 1e-9 * past.weight_kg.abs().max(1.0),
                "{:?} must weigh {} in the previous period but was {}",
                body_part,
                past.weight_kg,
                entry.previous_weight_kg
            );
        }
    }

    /// 増減が、表示中の期間の値から直前の同じ単位の期間の値を引いた値に一致する。
    #[test]
    fn req_anl_007_the_change_is_the_current_value_minus_the_previous_value(
        today in date(),
        period in period(),
        metric in metric(),
        entries in prop::collection::vec((-90i64..=90, body_part(), record()), 0..=40),
    ) {
        let sets = entries
            .iter()
            .map(|(offset, body_part, record)| BodyPartSet {
                date: today.add_days(*offset).expect("a day around today"),
                body_part: *body_part,
                record,
            })
            .collect::<Vec<_>>();
        let analysis = body_part_analysis(period, metric, today, &sets);
        let range = period.range(today);
        let previous = period.previous_range(today);

        for entry in &analysis.parts {
            let current = expected_totals(&sets, entry.body_part, range);
            let past = expected_totals(&sets, entry.body_part, previous);
            prop_assert_eq!(
                entry.sets_change,
                i64::from(current.sets) - i64::from(past.sets),
                "{:?}",
                entry.body_part
            );
            prop_assert!(
                (entry.weight_change_kg - (current.weight_kg - past.weight_kg)).abs()
                    <= 1e-9 * current.weight_kg.abs().max(past.weight_kg.abs()).max(1.0),
                "{:?}",
                entry.body_part
            );
            prop_assert_eq!(entry.value(AnalysisMetric::Sets), f64::from(current.sets));
            prop_assert_eq!(entry.value(AnalysisMetric::Weight), entry.weight_kg);
        }
    }

    /// 週の範囲が月曜 0 時から日曜まで、月の範囲が 1 日から末日までになる。
    #[test]
    fn req_anl_005_the_range_is_the_japan_week_or_month(today in date(), period in period()) {
        let range = period.range(today);
        let previous = period.previous_range(today);
        match period {
            AnalysisPeriod::Week => {
                prop_assert_eq!(range.start, today.week().start());
                prop_assert_eq!(range.end, today.week().end());
                prop_assert_eq!(previous.start, today.week().start().add_days(-7).expect("a day"));
                prop_assert_eq!(previous.end, today.week().start().add_days(-1).expect("a day"));
            }
            AnalysisPeriod::Month => {
                prop_assert_eq!(range.start, today.month_range().start());
                prop_assert_eq!(range.end, today.month_range().end());
                prop_assert_eq!(previous.start, today.month_range().start().subtract_months(1).expect("a month"));
                prop_assert_eq!(previous.end, today.month_range().start().add_days(-1).expect("a day"));
            }
        }
        prop_assert!(range.contains(today));
        prop_assert!(!previous.contains(today));
        prop_assert!(previous.end < range.start);
        for offset in [-100i64, -8, -1, 1, 31, 100] {
            let date = today.add_days(offset).expect("a day around today");
            let in_range = range.contains(date);
            let expected = match period {
                AnalysisPeriod::Week => date.week() == today.week(),
                AnalysisPeriod::Month => date.month_range() == today.month_range(),
            };
            prop_assert_eq!(in_range, expected, "{} at offset {}", date, offset);
        }
    }
}

fn date() -> impl Strategy<Value = JstDate> {
    (1900i64..=2100, 1u32..=12, 1u32..=28)
        .prop_map(|(year, month, day)| JstDate::new(year, month, day).expect("a valid date"))
}

fn period() -> impl Strategy<Value = AnalysisPeriod> {
    prop::sample::select(AnalysisPeriod::ALL.to_vec())
}

fn metric() -> impl Strategy<Value = AnalysisMetric> {
    prop::sample::select(AnalysisMetric::ALL.to_vec())
}

fn body_part() -> impl Strategy<Value = BodyPart> {
    prop::sample::select(BodyPart::ALL.to_vec())
}

fn record() -> impl Strategy<Value = SetRecord> {
    (
        prop::sample::select(vec![SetKind::Normal, SetKind::Warmup, SetKind::Drop]),
        prop_oneof![
            weight_segment().prop_map(|segment| (vec![segment], RecordMode::WeightAndReps)),
            time_segment().prop_map(|segment| (vec![segment], RecordMode::Time)),
        ],
    )
        .prop_map(|(kind, (segments, mode))| {
            SetRecord::new(key(), kind, segments, None, None, mode)
                .expect("the generated set is valid")
        })
}

fn weight_segment() -> impl Strategy<Value = SetSegment> {
    (set_fixture::weight(), set_fixture::valid_breakdown()).prop_map(
        |(weight_kg, (reps, momentum_reps, assisted_reps))| SetSegment {
            weight_kg,
            reps,
            momentum_reps,
            assisted_reps,
            left: None,
            right: None,
            duration_seconds: None,
        },
    )
}

fn time_segment() -> impl Strategy<Value = SetSegment> {
    (
        prop_oneof![Just(false), Just(true)],
        1u32..=86_400,
        1u32..=86_400,
    )
        .prop_map(|(lateral, first, second)| SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: lateral.then_some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(first),
            }),
            right: lateral.then_some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(second),
            }),
            duration_seconds: (!lateral).then_some(first),
        })
}

fn key() -> BestKey {
    BestKey {
        exercise_id: EXERCISE_ID.to_owned(),
        grip: None,
        attachment: None,
    }
}

#[derive(Debug, Clone, Copy)]
struct Totals {
    sets: u32,
    weight_kg: f64,
}

fn expected_totals(sets: &[BodyPartSet<'_>], body_part: BodyPart, range: AnalysisRange) -> Totals {
    sets.iter()
        .filter(|set| set.body_part == body_part && range.contains(set.date))
        .fold(
            Totals {
                sets: 0,
                weight_kg: 0.0,
            },
            |mut total, set| {
                total.sets += set.record.part_set_count(PartRole::Primary) as u32;
                total.weight_kg += set.record.total_weight_kg();
                total
            },
        )
}
