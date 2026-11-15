//! 目標重量と現在の記録の性質テスト。
//!
//! 対応する要求: `REQ-GOAL-001`、`REQ-GOAL-002`、`REQ-GOAL-004`、`REQ-GOAL-005`。
//! 現在の記録が直近 7 日間のトレーニングごとの最大重量の平均になること、記録が無ければ
//! 自己ベストの最大重量になること、差が目標重量から現在の記録を引いた値になること、
//! 途中の目標が現在の記録より大きく最終の目標より小さい値だけになることを確かめる。

use std::collections::BTreeMap;

use proptest::prelude::*;
use warrior_core::goal::{
    current_weight_kg, valid_intermediate_targets, ExerciseGoal, GoalError, GoalProgress, GoalSet,
    MAX_TARGET_WEIGHT_KG,
};
use warrior_core::jst::JstDate;
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};

const EXERCISE_ID: &str = "property-exercise";

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// 現在の記録が、直近 7 日間のトレーニングごとの最大重量の平均になる。
    #[test]
    fn req_goal_002_the_current_record_is_the_average_of_the_training_maxima(
        today in date(),
        entries in prop::collection::vec(
            (0usize..3, -30i64..=0, 0.5f64..=300.0),
            1..=30,
        ),
        best in prop::option::of(0.5f64..=300.0),
    ) {
        let records = entries
            .iter()
            .map(|(_, _, weight)| record(*weight))
            .collect::<Vec<_>>();
        let sets = entries
            .iter()
            .zip(records.iter())
            .map(|((training, offset, _), record)| GoalSet {
                training_id: training_id(*training),
                date: today.add_days(*offset).expect("a day around today"),
                record,
            })
            .collect::<Vec<_>>();
        let current = current_weight_kg(today, &sets, best);
        let expected = expected_current(today, &entries, best);
        match (current, expected) {
            (Some(actual), Some(expected)) => {
                prop_assert!((actual - expected).abs() <= 1e-9, "{actual} != {expected}");
            }
            (left, right) => prop_assert_eq!(left, right),
        }
    }

    /// 直近 7 日間に記録が無ければ、同じ組の自己ベストの最大重量を使う。
    #[test]
    fn req_goal_002_the_current_record_falls_back_to_the_personal_best(
        today in date(),
        offsets in prop::collection::vec(-60i64..=-7, 0..=20),
        weights in prop::collection::vec(0.5f64..=300.0, 20),
        best in 0.5f64..=300.0,
    ) {
        let records = offsets
            .iter()
            .zip(weights.iter())
            .map(|(_, weight)| record(*weight))
            .collect::<Vec<_>>();
        let sets = offsets
            .iter()
            .zip(records.iter())
            .enumerate()
            .map(|(index, (offset, record))| GoalSet {
                training_id: training_id(index % 3),
                date: today.add_days(*offset).expect("a past day"),
                record,
            })
            .collect::<Vec<_>>();
        prop_assert_eq!(
            current_weight_kg(today, &sets, Some(best)),
            Some(best)
        );
        prop_assert_eq!(current_weight_kg(today, &sets, None), None);
    }

    /// 差が目標重量から現在の記録を引いた値になる。
    #[test]
    fn req_goal_002_the_difference_is_the_target_minus_the_current_record(
        target in 0.1f64..=MAX_TARGET_WEIGHT_KG,
        current in 0.1f64..=MAX_TARGET_WEIGHT_KG,
    ) {
        let progress = GoalProgress::new(goal(target), Some(current));
        prop_assert_eq!(progress.current_weight_kg, Some(current));
        prop_assert!(
            (progress.difference_kg.expect("a difference") - (target - current)).abs()
                <= 1e-9
        );
    }

    /// 途中の目標が、現在の記録より大きく最終の目標より小さい値だけになる。
    #[test]
    fn req_goal_004_the_intermediate_targets_stay_inside_the_range(
        (current, target) in (0.1f64..=300.0)
            .prop_flat_map(|current| (Just(current), (current + 0.1)..=MAX_TARGET_WEIGHT_KG)),
        values in prop::collection::vec(-100.0f64..=1100.0, 0..=20),
    ) {
        let accepted = valid_intermediate_targets(Some(current), target, &values);
        let expected = values
            .iter()
            .copied()
            .filter(|value| *value > current && *value < target)
            .collect::<Vec<_>>();
        prop_assert_eq!(accepted, expected);
    }

    /// 目標重量が範囲内で、時期が設定の日より後なら受け付ける。
    #[test]
    fn req_goal_001_a_weight_inside_the_range_and_a_future_date_are_accepted(
        today in date(),
        weight in 0.1f64..=MAX_TARGET_WEIGHT_KG,
        days in 1i64..=3650,
    ) {
        let target_date = today.add_days(days).expect("a future day");
        prop_assert_eq!(
            goal_with(weight, Some(target_date)).validate(today),
            Ok(())
        );
    }

    /// 範囲外の目標重量と、設定の日以前の時期を拒否する。
    #[test]
    fn req_goal_001_an_out_of_range_weight_or_a_past_date_is_rejected(
        today in date(),
        weight in prop::sample::select(vec![0.0, -1.0, f64::NAN, f64::INFINITY, 1000.1]),
        days in -3650i64..=0,
    ) {
        prop_assert_eq!(
            goal_with(weight, None).validate(today),
            Err(GoalError::InvalidTargetWeight)
        );
        let target_date = today.add_days(days).expect("a past day");
        prop_assert_eq!(
            goal_with(100.0, Some(target_date)).validate(today),
            Err(GoalError::TargetDateNotAfterToday)
        );
    }

    /// 現在の記録が無ければ差を出さない。
    #[test]
    fn req_goal_002_a_missing_current_record_has_no_difference(
        target in 0.1f64..=MAX_TARGET_WEIGHT_KG,
    ) {
        let progress = GoalProgress::new(goal(target), None);
        prop_assert_eq!(progress.current_weight_kg, None);
        prop_assert_eq!(progress.difference_kg, None);
    }
}

fn expected_current(
    today: JstDate,
    entries: &[(usize, i64, f64)],
    best: Option<f64>,
) -> Option<f64> {
    let start = today.add_days(-6).expect("the start of the window");
    let mut maxima = BTreeMap::<usize, f64>::new();
    for (training, offset, weight) in entries {
        let date = today.add_days(*offset).expect("a day around today");
        if date < start || date > today {
            continue;
        }
        maxima
            .entry(*training)
            .and_modify(|current| *current = current.max(*weight))
            .or_insert(*weight);
    }
    if maxima.is_empty() {
        return best;
    }
    Some(maxima.values().sum::<f64>() / maxima.len() as f64)
}

fn date() -> impl Strategy<Value = JstDate> {
    (2000i64..=2100, 1u32..=12, 1u32..=28)
        .prop_map(|(year, month, day)| JstDate::new(year, month, day).expect("a valid date"))
}

fn training_id(index: usize) -> &'static str {
    match index {
        0 => "training-0",
        1 => "training-1",
        _ => "training-2",
    }
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

fn record(weight_kg: f64) -> SetRecord {
    SetRecord::new(
        key(),
        SetKind::Normal,
        vec![segment(weight_kg, 5)],
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .expect("a valid set")
}

fn goal(target_weight_kg: f64) -> ExerciseGoal {
    goal_with(target_weight_kg, None)
}

fn goal_with(target_weight_kg: f64, target_date: Option<JstDate>) -> ExerciseGoal {
    ExerciseGoal {
        exercise_id: EXERCISE_ID.to_owned(),
        grip: None,
        attachment: None,
        target_weight_kg,
        target_date,
        current_weight_kg: None,
    }
}
