//! 種目別の履歴の並びと、推移のグラフの性質テスト。
//!
//! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`。
//! 期間の選択が期間の始まり以降の記録だけを返すこと、グラフの点が `set_calc` の計算と
//! 一致し、同じ日の複数の記録を指標に従って最大値または合計にまとめること、
//! 履歴が日付と終了の時刻の新しい順に並ぶことを確かめる。

use proptest::prelude::*;
use warrior_core::datetime;
use warrior_core::history::{
    graph_points, sort_trainings, GraphMetric, GraphPeriod, GraphSet, HistoryTraining,
    MetricAggregation,
};
use warrior_core::jst::{JstDate, UtcInstant};
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};

const EXERCISE_ID: &str = "property-exercise";

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// 期間の選択が、期間の始まり以降、表示の日までの記録だけを返す。
    #[test]
    fn req_anl_003_the_period_returns_only_the_records_inside_it(
        today in date(),
        period in period(),
        offsets in prop::collection::vec(-400i64..=400, 1..=40),
        weights in prop::collection::vec(0.5f64..=300.0, 40),
    ) {
        let sets = offsets
            .iter()
            .zip(weights.iter())
            .map(|(offset, weight)| {
                let date = today.add_days(*offset).expect("a day around today");
                let record = SetRecord::new(
                    key(),
                    SetKind::Normal,
                    vec![segment(*weight, 1)],
                    None,
                    None,
                    RecordMode::WeightAndReps,
                )
                .expect("a valid set");
                (date, record)
            })
            .collect::<Vec<_>>();
        let dated = sets
            .iter()
            .map(|(date, record)| GraphSet {
                date: *date,
                record,
            })
            .collect::<Vec<_>>();
        let points = graph_points(period, today, GraphMetric::MaxWeight, &dated);

        let mut expected = sets
            .iter()
            .filter(|(date, _)| period.contains(today, *date))
            .map(|(date, _)| *date)
            .collect::<Vec<_>>();
        expected.sort_unstable();
        expected.dedup();
        let actual = points
            .iter()
            .map(|point| point.date)
            .collect::<Vec<_>>();
        prop_assert_eq!(actual.clone(), expected.clone());
        if let Some(start) = period.start(today) {
            prop_assert!(actual.iter().all(|date| *date >= start));
        }
        prop_assert!(actual.iter().all(|date| *date <= today));
        prop_assert!(
            sets.iter().all(|(date, _)| !period.contains(today, *date)
                || actual.contains(date)),
            "every record inside the period has a point"
        );
    }

    /// グラフの点が `set_calc` の計算と一致し、同じ日の記録を指標に従ってまとめる。
    #[test]
    fn req_anl_002_the_points_follow_the_set_calculation_and_the_aggregation(
        today in date(),
        entries in prop::collection::vec((-30i64..=0, record()), 1..=30),
    ) {
        let sets = entries
            .iter()
            .map(|(offset, record)| GraphSet {
                date: today.add_days(*offset).expect("a day around today"),
                record,
            })
            .collect::<Vec<_>>();
        for metric in GraphMetric::WEIGHT_METRICS
            .into_iter()
            .chain(GraphMetric::TIME_METRICS)
        {
            let points = graph_points(GraphPeriod::All, today, metric, &sets);
            let expected = expected_points(&sets, metric);
            prop_assert_eq!(
                points.len(),
                expected.len(),
                "the point count of {:?}",
                metric
            );
            for (point, (date, value)) in points.iter().zip(expected.iter()) {
                prop_assert_eq!(point.date, *date, "{:?}", metric);
                prop_assert!(
                    (point.value - value).abs() <= 1e-6 * point.value.abs().max(1.0),
                    "{:?} on {} must be {} but was {}",
                    metric,
                    date,
                    value,
                    point.value
                );
            }
        }
    }

    /// 履歴が日付の新しい順、同じ日は終了の時刻の新しい順に並ぶ。
    #[test]
    fn req_anl_001_the_history_is_ordered_by_the_date_and_the_finished_time(
        entries in prop::collection::vec(
            (date(), 0i64..=2_000_000_000, 0u32..=999),
            0..=30,
        ),
    ) {
        let mut trainings = entries
            .iter()
            .map(|(date, seconds, suffix)| HistoryTraining {
                training_id: format!("training-{suffix}"),
                date: *date,
                finished_at: datetime::format_utc(*seconds),
                sets: Vec::new(),
            })
            .collect::<Vec<_>>();
        sort_trainings(&mut trainings);
        for pair in trainings.windows(2) {
            let left = (&pair[0].date, finished(&pair[0].finished_at), &pair[0].training_id);
            let right = (&pair[1].date, finished(&pair[1].finished_at), &pair[1].training_id);
            prop_assert!(
                left >= right,
                "{} must not come before {}",
                pair[0].training_id,
                pair[1].training_id
            );
        }
    }
}

fn date() -> impl Strategy<Value = JstDate> {
    (1900i64..=2100, 1u32..=12, 1u32..=28)
        .prop_map(|(year, month, day)| JstDate::new(year, month, day).expect("a valid date"))
}

fn period() -> impl Strategy<Value = GraphPeriod> {
    prop::sample::select(GraphPeriod::ALL.to_vec())
}

fn key() -> BestKey {
    BestKey {
        exercise_id: EXERCISE_ID.to_owned(),
        grip: None,
        attachment: None,
    }
}

fn weight_segments() -> impl Strategy<Value = Vec<SetSegment>> {
    (1usize..=3)
        .prop_flat_map(|count| prop::collection::vec((0.5f64..=300.0, 1u32..=15, 0u32..=15), count))
        .prop_map(|values| {
            values
                .into_iter()
                .map(|(weight_kg, reps, breakdown)| {
                    let momentum_reps = breakdown % (reps + 1);
                    let assisted_reps = breakdown % (reps + 1 - momentum_reps);
                    SetSegment {
                        weight_kg,
                        reps,
                        momentum_reps,
                        assisted_reps,
                        left: None,
                        right: None,
                        duration_seconds: None,
                    }
                })
                .collect()
        })
}

fn time_segment() -> impl Strategy<Value = SetSegment> {
    (
        prop_oneof![Just(false), Just(true)],
        1u32..=86_400,
        1u32..=86_400,
    )
        .prop_map(|(lateral, first, second)| {
            if lateral {
                SetSegment {
                    weight_kg: 0.0,
                    reps: 0,
                    momentum_reps: 0,
                    assisted_reps: 0,
                    left: Some(SideValue {
                        weight_kg: 0.0,
                        reps: 0,
                        duration_seconds: Some(first),
                    }),
                    right: Some(SideValue {
                        weight_kg: 0.0,
                        reps: 0,
                        duration_seconds: Some(second),
                    }),
                    duration_seconds: None,
                }
            } else {
                SetSegment {
                    weight_kg: 0.0,
                    reps: 0,
                    momentum_reps: 0,
                    assisted_reps: 0,
                    left: None,
                    right: None,
                    duration_seconds: Some(first),
                }
            }
        })
}

fn record() -> impl Strategy<Value = SetRecord> {
    (
        prop::sample::select(vec![SetKind::Normal, SetKind::Warmup, SetKind::Drop]),
        prop_oneof![
            (
                weight_segments(),
                prop_oneof![
                    Just(RecordMode::WeightAndReps),
                    Just(RecordMode::BodyweightAndReps),
                ]
            )
                .prop_map(|(segments, mode)| (segments, mode)),
            time_segment().prop_map(|segment| (vec![segment], RecordMode::Time)),
        ],
    )
        .prop_map(|(kind, (segments, mode))| {
            SetRecord::new(key(), kind, segments, None, None, mode)
                .expect("the generated set is valid")
        })
}

fn expected_points(sets: &[GraphSet<'_>], metric: GraphMetric) -> Vec<(JstDate, f64)> {
    let mut days = Vec::<(JstDate, f64)>::new();
    for set in sets {
        if set.record.kind() == SetKind::Warmup {
            continue;
        }
        let Some(value) = expected_value(set.record, metric) else {
            continue;
        };
        match days.iter().position(|(date, _)| *date == set.date) {
            Some(index) => {
                let current = &mut days[index];
                current.1 = match metric.aggregation() {
                    MetricAggregation::Max => current.1.max(value),
                    MetricAggregation::Sum => current.1 + value,
                };
            }
            None => days.push((set.date, value)),
        }
    }
    days.sort_by_key(|(date, _)| *date);
    days
}

fn expected_value(record: &SetRecord, metric: GraphMetric) -> Option<f64> {
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

fn finished(text: &str) -> UtcInstant {
    UtcInstant::parse(text).expect("a formatted instant")
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
