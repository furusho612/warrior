//! 前回の選択と、今回との差の性質テスト。
//!
//! 対応する要求: `REQ-REC-003`、`REQ-REC-004`。
//! 任意の候補について、種目ごとの前回が同じ種目、同じグリップ、同じアタッチメントの
//! 最も新しい終了済みであり、トレーニング全体の前回がプログラム、独自のプログラム、
//! ルーティン、空の状態の順の規則に従うことを確かめる。
//! 差は今回から前回を引いた値になり、前回が無ければ差を返さないことを確かめる。

use proptest::prelude::*;
use warrior_core::previous::{
    duration_difference, exercise_difference, select_exercise_previous, select_training_previous,
    total_weight_difference, ExerciseDifference, PreviousExercise, PreviousExerciseCandidate,
    PreviousExerciseKey, PreviousSetKey, PreviousTrainingCandidate, PreviousTrainingSource,
};
use warrior_core::set_calc::RecordMode;
use warrior_core::training_session::SessionSource;

fn finished_at() -> impl Strategy<Value = String> {
    (1u8..=9).prop_map(|day| format!("2026-11-{day:02}T00:00:00Z"))
}

fn exercise_id() -> impl Strategy<Value = String> {
    prop_oneof![Just("bench".to_owned()), Just("squat".to_owned())]
}

fn grip_key() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some("pronated_medium".to_owned())),
        Just(Some("supinated_wide".to_owned())),
    ]
}

fn attachment_key() -> impl Strategy<Value = Option<String>> {
    prop_oneof![Just(None), Just(Some("wide_bar".to_owned()))]
}

fn set_key() -> impl Strategy<Value = (String, Option<String>, Option<String>)> {
    (exercise_id(), grip_key(), attachment_key())
}

fn exercise_candidate() -> impl Strategy<Value = PreviousExerciseCandidate> {
    (
        0u8..6,
        finished_at(),
        prop::collection::vec(set_key(), 0..=3),
    )
        .prop_map(|(index, finished_at, sets)| PreviousExerciseCandidate {
            training_id: format!("training-{index}"),
            finished_at,
            sets: sets
                .into_iter()
                .map(|(exercise_id, grip, attachment)| PreviousSetKey {
                    exercise_id,
                    grip,
                    attachment,
                })
                .collect(),
        })
}

fn source_kind() -> impl Strategy<Value = SessionSource> {
    prop_oneof![
        Just(SessionSource::Empty),
        Just(SessionSource::Routine),
        Just(SessionSource::Program),
        Just(SessionSource::CustomProgram),
    ]
}

fn optional_id() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        (0u8..3).prop_map(|index| Some(format!("day-{index}")))
    ]
}

fn training_candidate() -> impl Strategy<Value = PreviousTrainingCandidate> {
    (
        0u8..6,
        finished_at(),
        source_kind(),
        optional_id(),
        optional_id(),
        optional_id(),
        -20_000i64..=20_000,
    )
        .prop_map(
            |(index, finished_at, source_kind, routine, program, custom, weight)| {
                PreviousTrainingCandidate {
                    training_id: format!("training-{index}"),
                    finished_at,
                    source_kind,
                    routine_id: routine,
                    program_day_id: program,
                    custom_program_day_id: custom,
                    total_weight_kg: weight as f64 / 10.0,
                }
            },
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrainingRule {
    Program,
    Custom,
    Routine,
    Empty,
}

fn matches_rule(
    candidate: &PreviousTrainingCandidate,
    source: &PreviousTrainingSource,
    rule: TrainingRule,
) -> bool {
    match rule {
        TrainingRule::Program => {
            let Some(program_day_id) = &source.program_day_id else {
                return false;
            };
            candidate.program_day_id.as_deref() == Some(program_day_id.as_str())
        }
        TrainingRule::Custom => {
            let Some(custom_program_day_id) = &source.custom_program_day_id else {
                return false;
            };
            candidate.custom_program_day_id.as_deref() == Some(custom_program_day_id.as_str())
        }
        TrainingRule::Routine => {
            let Some(routine_id) = &source.routine_id else {
                return false;
            };
            candidate.routine_id.as_deref() == Some(routine_id.as_str())
        }
        TrainingRule::Empty => candidate.source_kind == SessionSource::Empty,
    }
}

fn first_rule_with_a_candidate(
    candidates: &[PreviousTrainingCandidate],
    source: &PreviousTrainingSource,
) -> Option<TrainingRule> {
    for rule in [
        TrainingRule::Program,
        TrainingRule::Custom,
        TrainingRule::Routine,
    ] {
        if candidates
            .iter()
            .any(|candidate| matches_rule(candidate, source, rule))
        {
            return Some(rule);
        }
    }
    candidates
        .iter()
        .any(|candidate| matches_rule(candidate, source, TrainingRule::Empty))
        .then_some(TrainingRule::Empty)
}

fn difference_kg() -> impl Strategy<Value = i64> {
    -20_000i64..=20_000i64
}

fn seconds() -> impl Strategy<Value = u32> {
    0u32..=86_400
}

fn previous_exercise() -> impl Strategy<Value = PreviousExercise> {
    (seconds(), seconds(), difference_kg()).prop_map(|(total, longest, weight)| PreviousExercise {
        sets: Vec::new(),
        total_weight_kg: weight as f64 / 10.0,
        total_duration_seconds: total,
        max_duration_seconds: longest,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_exercise_previous_is_the_latest_matching_candidate(
        candidates in prop::collection::vec(exercise_candidate(), 0..=8),
        exercise_id in exercise_id(),
        grip in grip_key(),
        attachment in attachment_key(),
    ) {
        let key = PreviousExerciseKey {
            exercise_id,
            grip,
            attachment,
        };
        let selected = select_exercise_previous(&candidates, &key);
        let matches = |candidate: &PreviousExerciseCandidate| {
            candidate.sets.iter().any(|set| {
                set.exercise_id == key.exercise_id
                    && set.grip == key.grip
                    && set.attachment == key.attachment
            })
        };
        match selected {
            None => prop_assert!(!candidates.iter().any(&matches)),
            Some(selected) => {
                prop_assert!(matches(selected));
                for candidate in &candidates {
                    if matches(candidate) {
                        prop_assert!(
                            candidate.finished_at < selected.finished_at
                                || (candidate.finished_at == selected.finished_at
                                    && candidate.training_id <= selected.training_id)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_training_previous_follows_the_source_priority(
        candidates in prop::collection::vec(training_candidate(), 0..=8),
        program in optional_id(),
        custom in optional_id(),
        routine in optional_id(),
    ) {
        let source = PreviousTrainingSource::new(routine, program, custom);
        let selected = select_training_previous(&candidates, &source);
        match first_rule_with_a_candidate(&candidates, &source) {
            None => prop_assert!(selected.is_none()),
            Some(rule) => {
                let selected = selected.expect("a candidate for the first rule");
                prop_assert!(matches_rule(selected, &source, rule));
                for candidate in &candidates {
                    if matches_rule(candidate, &source, rule) {
                        prop_assert!(
                            candidate.finished_at < selected.finished_at
                                || (candidate.finished_at == selected.finished_at
                                    && candidate.training_id <= selected.training_id)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_matching_program_or_custom_program_day_beats_a_newer_routine(
        program_index in 0u8..3,
        custom_index in 0u8..3,
        routine_index in 0u8..3,
    ) {
        let program_id = format!("program-{program_index}");
        let custom_id = format!("custom-{custom_index}");
        let routine_id = format!("routine-{routine_index}");
        let candidates = vec![
            PreviousTrainingCandidate {
                training_id: "routine-latest".to_owned(),
                finished_at: "2026-11-20T00:00:00Z".to_owned(),
                source_kind: SessionSource::Routine,
                routine_id: Some(routine_id.clone()),
                program_day_id: None,
                custom_program_day_id: None,
                total_weight_kg: 1_000.0,
            },
            PreviousTrainingCandidate {
                training_id: "program".to_owned(),
                finished_at: "2026-11-10T00:00:00Z".to_owned(),
                source_kind: SessionSource::Program,
                routine_id: None,
                program_day_id: Some(program_id.clone()),
                custom_program_day_id: None,
                total_weight_kg: 900.0,
            },
            PreviousTrainingCandidate {
                training_id: "custom".to_owned(),
                finished_at: "2026-11-11T00:00:00Z".to_owned(),
                source_kind: SessionSource::CustomProgram,
                routine_id: None,
                program_day_id: None,
                custom_program_day_id: Some(custom_id.clone()),
                total_weight_kg: 950.0,
            },
        ];
        let program_source = PreviousTrainingSource::new(
            Some(routine_id.clone()),
            Some(program_id),
            Some(custom_id.clone()),
        );
        prop_assert_eq!(
            select_training_previous(&candidates, &program_source)
                .map(|candidate| candidate.training_id.as_str()),
            Some("program")
        );
        let custom_source =
            PreviousTrainingSource::new(Some(routine_id), None, Some(custom_id));
        prop_assert_eq!(
            select_training_previous(&candidates, &custom_source)
                .map(|candidate| candidate.training_id.as_str()),
            Some("custom")
        );
    }

    #[test]
    fn the_total_weight_difference_is_current_minus_previous(
        current in difference_kg(),
        previous in prop::option::of(difference_kg()),
    ) {
        let current = current as f64 / 10.0;
        let previous = previous.map(|value| value as f64 / 10.0);
        let difference = total_weight_difference(current, previous);
        match previous {
            None => prop_assert_eq!(difference, None),
            Some(previous) => {
                let difference = difference.expect("a difference with a previous value");
                prop_assert_eq!(difference.current_kg, current);
                prop_assert_eq!(difference.previous_kg, previous);
                prop_assert!((difference.difference_kg - (current - previous)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn the_time_difference_has_the_total_and_the_longest(
        current_total in seconds(),
        current_longest in seconds(),
        previous in prop::option::of(previous_exercise()),
    ) {
        let difference = duration_difference(current_total, current_longest, previous.as_ref());
        match previous {
            None => prop_assert_eq!(difference, None),
            Some(previous) => {
                let difference = difference.expect("a difference with a previous value");
                prop_assert_eq!(difference.current_total_seconds, current_total);
                prop_assert_eq!(difference.previous_total_seconds, previous.total_duration_seconds);
                prop_assert_eq!(
                    difference.total_difference_seconds,
                    i64::from(current_total) - i64::from(previous.total_duration_seconds)
                );
                prop_assert_eq!(difference.current_longest_seconds, current_longest);
                prop_assert_eq!(
                    difference.longest_difference_seconds,
                    i64::from(current_longest) - i64::from(previous.max_duration_seconds)
                );
            }
        }
    }

    #[test]
    fn the_exercise_difference_uses_the_record_mode(
        current_weight in difference_kg(),
        current_total in seconds(),
        current_longest in seconds(),
        previous in previous_exercise(),
    ) {
        let current_weight = current_weight as f64 / 10.0;
        let weight = exercise_difference(
            RecordMode::WeightAndReps,
            current_weight,
            current_total,
            current_longest,
            Some(&previous),
        );
        match weight {
            Some(ExerciseDifference::Weight(diff)) => {
                prop_assert_eq!(diff.current_kg, current_weight);
                prop_assert_eq!(diff.previous_kg, previous.total_weight_kg);
                prop_assert!(
                    (diff.difference_kg - (current_weight - previous.total_weight_kg)).abs()
                        < 1e-9
                );
            }
            _ => prop_assert!(false, "a weight exercise must return the weight difference"),
        }
        let time = exercise_difference(
            RecordMode::Time,
            current_weight,
            current_total,
            current_longest,
            Some(&previous),
        );
        match time {
            Some(ExerciseDifference::Time(diff)) => {
                prop_assert_eq!(diff.current_total_seconds, current_total);
                prop_assert_eq!(diff.previous_total_seconds, previous.total_duration_seconds);
                prop_assert_eq!(diff.current_longest_seconds, current_longest);
                prop_assert_eq!(diff.previous_longest_seconds, previous.max_duration_seconds);
            }
            _ => prop_assert!(false, "a time exercise must return the duration difference"),
        }
        prop_assert_eq!(
            exercise_difference(
                RecordMode::WeightAndReps,
                current_weight,
                current_total,
                current_longest,
                None
            ),
            None
        );
        prop_assert_eq!(
            exercise_difference(
                RecordMode::Time,
                current_weight,
                current_total,
                current_longest,
                None
            ),
            None
        );
    }
}
