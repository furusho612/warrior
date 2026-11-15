//! 自己ベストの判定の性質テスト。
//!
//! 対応する要求: `REQ-SET-009`、`REQ-SET-011`。
//! セットを完了するたびに候補を作って保存済みの値と比べた結果が、最後に全ての記録から
//! `recompute` で計算し直した値と一致することを確かめる。比較の単位ごとに 1 行だけを
//! 持つことも確かめる。

use proptest::prelude::*;
use warrior_core::exercise::Laterality;
use warrior_core::jst::JstDate;
use warrior_core::personal_best::{
    apply, candidates, recompute, PersonalBestEntry, PersonalBestKey, TrainingSets,
};
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue};
use warrior_core::training_session::SessionSet;

fn exercise_name(index: u8) -> String {
    format!("exercise-{index}")
}

fn key(exercise: u8, grip: u8, attachment: u8) -> BestKey {
    let grip = match grip {
        0 => None,
        1 => Some("pronated_medium".to_owned()),
        _ => Some("parallel".to_owned()),
    };
    let attachment = match attachment {
        0 => None,
        _ => Some("wide_bar".to_owned()),
    };
    BestKey {
        exercise_id: exercise_name(exercise),
        grip,
        attachment,
    }
}

fn segment() -> impl Strategy<Value = SetSegment> {
    (0.5f64..=300.0, 1u32..=30).prop_map(|(weight_kg, reps)| SetSegment {
        weight_kg,
        reps,
        momentum_reps: 0,
        assisted_reps: 0,
        left: None,
        right: None,
        duration_seconds: None,
    })
}

fn unilateral_segment() -> impl Strategy<Value = SetSegment> {
    (0.5f64..=100.0, 0.5f64..=100.0, 1u32..=20, 1u32..=20).prop_map(
        |(left_weight, right_weight, left_reps, right_reps)| SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: Some(SideValue {
                weight_kg: left_weight,
                reps: left_reps,
                duration_seconds: None,
            }),
            right: Some(SideValue {
                weight_kg: right_weight,
                reps: right_reps,
                duration_seconds: None,
            }),
            duration_seconds: None,
        },
    )
}

fn weight_record() -> impl Strategy<Value = SetRecord> {
    (
        0u8..=2,
        0u8..=2,
        0u8..=2,
        0u8..=2,
        0u8..=2,
        prop_oneof![
            prop::collection::vec(segment(), 1..=3),
            unilateral_segment().prop_map(|segment| vec![segment]),
        ],
    )
        .prop_map(|(exercise, grip, attachment, kind, mode, segments)| {
            let kind = match kind {
                0 => SetKind::Normal,
                1 => SetKind::Warmup,
                _ => SetKind::Drop,
            };
            let mode = if mode == 0 {
                RecordMode::WeightAndReps
            } else {
                RecordMode::BodyweightAndReps
            };
            SetRecord::new(
                key(exercise, grip, attachment),
                kind,
                segments,
                None,
                None,
                mode,
            )
            .expect("the generated weight set is valid")
        })
}

fn time_record() -> impl Strategy<Value = SetRecord> {
    (
        0u8..=2,
        0u8..=2,
        (1u32..=86_400, 1u32..=86_400),
        0.0f64..=100.0,
        1u32..=20,
        prop_oneof![Just(false), Just(true)],
    )
        .prop_map(
            |(exercise, grip, (duration, other_duration), side_weight, side_reps, lateral)| {
                let segment = if lateral {
                    SetSegment {
                        weight_kg: 0.0,
                        reps: 0,
                        momentum_reps: 0,
                        assisted_reps: 0,
                        left: Some(SideValue {
                            weight_kg: side_weight,
                            reps: side_reps,
                            duration_seconds: Some(duration),
                        }),
                        right: Some(SideValue {
                            weight_kg: side_weight,
                            reps: side_reps,
                            duration_seconds: Some(other_duration),
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
                        duration_seconds: Some(duration),
                    }
                };
                SetRecord::new(
                    key(exercise, grip, 0),
                    SetKind::Normal,
                    vec![segment],
                    None,
                    None,
                    RecordMode::Time,
                )
                .expect("the generated time set is valid")
            },
        )
}

fn session_set() -> impl Strategy<Value = SessionSet> {
    prop_oneof![weight_record(), time_record()].prop_map(|record| {
        let laterality = if record.mode() == RecordMode::Time
            && record
                .segments()
                .first()
                .is_some_and(|segment| segment.left.is_some())
        {
            Laterality::Unilateral
        } else {
            Laterality::Bilateral
        };
        SessionSet {
            record,
            exercise_name: "種目".to_owned(),
            laterality,
        }
    })
}

fn sorted(entries: &[PersonalBestEntry]) -> Vec<String> {
    let mut values = entries
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}",
                entry.key.exercise_id,
                entry.key.grip.as_deref().unwrap_or(""),
                entry.key.attachment.as_deref().unwrap_or(""),
                entry.key.kind.as_str(),
                entry.value,
                entry.achieved_on,
                entry.training_id
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn unique_keys(entries: &[PersonalBestEntry]) -> usize {
    let mut keys = entries
        .iter()
        .map(|entry| entry.key.clone())
        .collect::<Vec<PersonalBestKey>>();
    keys.sort_by(|left, right| {
        left.exercise_id
            .cmp(&right.exercise_id)
            .then_with(|| left.grip.cmp(&right.grip))
            .then_with(|| left.attachment.cmp(&right.attachment))
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
    });
    keys.dedup();
    keys.len()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn incremental_updates_equal_recomputing_from_all_records(
        trainings in prop::collection::vec(prop::collection::vec(session_set(), 0..=4), 0..=4),
    ) {
        let ids = (0..trainings.len())
            .map(|index| format!("training-{index}"))
            .collect::<Vec<_>>();
        let dates = (0..trainings.len())
            .map(|index| JstDate::new(2026, 11, 1 + u32::try_from(index).unwrap_or(0)).unwrap())
            .collect::<Vec<_>>();
        let mut entries = Vec::<PersonalBestEntry>::new();
        for (index, sets) in trainings.iter().enumerate() {
            for prefix in 1..=sets.len() {
                let candidates = candidates(dates[index], &sets[..prefix]);
                apply(&mut entries, dates[index], &ids[index], &candidates);
            }
        }
        let snapshots = trainings
            .iter()
            .enumerate()
            .map(|(index, sets)| TrainingSets {
                training_id: &ids[index],
                achieved_on: dates[index],
                sets,
            })
            .collect::<Vec<_>>();
        let expected = recompute(&snapshots);

        prop_assert_eq!(sorted(&entries), sorted(&expected));
        prop_assert_eq!(unique_keys(&entries), entries.len());
    }
}
