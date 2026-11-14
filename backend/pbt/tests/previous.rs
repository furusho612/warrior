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

fn latest_matching<'a>(
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

fn latest_training_matching<'a>(
    candidates: &'a [PreviousTrainingCandidate],
    source: &PreviousTrainingSource,
) -> Option<&'a PreviousTrainingCandidate> {
    let matching = candidates.iter().filter(|candidate| {
        if let Some(program_day_id) = &source.program_day_id {
            return candidate.program_day_id.as_deref() == Some(program_day_id.as_str());
        }
        if let Some(custom_program_day_id) = &source.custom_program_day_id {
            return candidate.custom_program_day_id.as_deref()
                == Some(custom_program_day_id.as_str());
        }
        if let Some(routine_id) = &source.routine_id {
            return candidate.routine_id.as_deref() == Some(routine_id.as_str());
        }
        candidate.source_kind == SessionSource::Empty
    });
    matching.max_by(|left, right| {
        left.finished_at
            .cmp(&right.finished_at)
            .then_with(|| left.training_id.cmp(&right.training_id))
    })
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
        let expected = latest_matching(&candidates, &key);
        prop_assert_eq!(
            selected.map(|candidate| candidate.training_id.as_str()),
            expected.map(|candidate| candidate.training_id.as_str())
        );
        if let Some(selected) = selected {
            let selected_matches = selected.sets.iter().any(|set| {
                set.exercise_id == key.exercise_id
                    && set.grip == key.grip
                    && set.attachment == key.attachment
            });
            prop_assert!(selected_matches);
            for candidate in &candidates {
                let matches = candidate.sets.iter().any(|set| {
                    set.exercise_id == key.exercise_id
                        && set.grip == key.grip
                        && set.attachment == key.attachment
                });
                if matches {
                    prop_assert!(candidate.finished_at <= selected.finished_at);
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
        let expected = latest_training_matching(&candidates, &source);
        prop_assert_eq!(
            selected.map(|candidate| candidate.training_id.as_str()),
            expected.map(|candidate| candidate.training_id.as_str())
        );
        if let Some(program_day_id) = &source.program_day_id {
            let has_program = candidates
                .iter()
                .any(|candidate| candidate.program_day_id.as_deref() == Some(program_day_id.as_str()));
            if has_program {
                prop_assert_eq!(
                    selected.and_then(|candidate| candidate.program_day_id.as_deref()),
                    Some(program_day_id.as_str())
                );
            }
        } else if let Some(custom_program_day_id) = &source.custom_program_day_id {
            let has_custom = candidates.iter().any(|candidate| {
                candidate.custom_program_day_id.as_deref() == Some(custom_program_day_id.as_str())
            });
            if has_custom {
                prop_assert_eq!(
                    selected.and_then(|candidate| candidate.custom_program_day_id.as_deref()),
                    Some(custom_program_day_id.as_str())
                );
            }
        } else if let Some(routine_id) = &source.routine_id {
            let has_routine = candidates
                .iter()
                .any(|candidate| candidate.routine_id.as_deref() == Some(routine_id.as_str()));
            if has_routine {
                prop_assert_eq!(
                    selected.and_then(|candidate| candidate.routine_id.as_deref()),
                    Some(routine_id.as_str())
                );
            }
        } else {
            prop_assert!(
                selected.is_none_or(|candidate| candidate.source_kind == SessionSource::Empty)
            );
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
        prop_assert!(matches!(weight, Some(ExerciseDifference::Weight(_))));
        let time = exercise_difference(
            RecordMode::Time,
            current_weight,
            current_total,
            current_longest,
            Some(&previous),
        );
        prop_assert!(matches!(time, Some(ExerciseDifference::Time(_))));
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
