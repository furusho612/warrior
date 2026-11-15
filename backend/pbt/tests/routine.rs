//! ルーティンの複製の性質テスト (PRD の「ルーティン」)。
//!
//! 対応する要求: `REQ-RTN-003`。
//! 任意の内容のルーティンを複製すると、名称、種目の順序、スナップショット、目標が
//! 元と一致し、新しい ID を持つ。複製を編集しても元の内容は変わらない。

use proptest::prelude::*;
use warrior_core::exercise::{
    BodyPart, ExerciseSnapshot, GripMode, Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
};
use warrior_core::routine::{Routine, RoutineExercise, RoutineTarget};
use warrior_core::set_calc::RecordMode;

fn routine_strategy() -> impl Strategy<Value = Routine> {
    (
        "[a-z]{1,20}",
        prop::collection::vec(exercise_strategy(), 0..5),
    )
        .prop_map(|(name, exercises)| Routine {
            id: "3f2504e0-4f89-41d3-9a0c-0305e82c5601".to_owned(),
            name,
            position: 3,
            exercises,
            created_at: "2026-11-12T00:00:00Z".to_owned(),
            updated_at: "2026-11-12T00:00:00Z".to_owned(),
        })
}

fn exercise_strategy() -> impl Strategy<Value = RoutineExercise> {
    (
        1i64..8,
        1u32..8,
        prop::option::of(0.0f64..300.0),
        0u32..20,
        prop::option::of(1u32..3_600),
    )
        .prop_map(
            |(position, set_count, weight_kg, reps, rest_seconds)| RoutineExercise {
                id: format!("3f2504e0-4f89-41d3-9a0c-0305e82c{:04x}", position),
                routine_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5601".to_owned(),
                exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5602".to_owned(),
                snapshot: snapshot(),
                position,
                target: RoutineTarget {
                    set_count,
                    weight_kg,
                    reps: Some(reps),
                    duration_seconds: None,
                    grip: None,
                    rest_seconds,
                },
                created_at: "2026-11-12T00:00:00Z".to_owned(),
                updated_at: "2026-11-12T00:00:00Z".to_owned(),
            },
        )
}

fn snapshot() -> ExerciseSnapshot {
    ExerciseSnapshot {
        exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5602".to_owned(),
        name: "ベンチプレス".to_owned(),
        body_part: BodyPart::Chest,
        muscles: vec![MuscleAssignment {
            muscle: MuscleUnit::PectoralisMajorMiddle,
            role: MuscleRole::Primary,
        }],
        record_mode: RecordMode::WeightAndReps,
        laterality: Laterality::Bilateral,
        grip_mode: GripMode::None,
        grip_options: Vec::new(),
    }
}

proptest! {
    #[test]
    fn req_rtn_003_a_duplicate_has_the_same_content_with_new_ids(source in routine_strategy()) {
        let new_ids = (0..source.exercises.len())
            .map(|index| format!("copy-exercise-{index}"))
            .collect::<Vec<_>>();
        let copy = source
            .duplicate(
                "3f2504e0-4f89-41d3-9a0c-0305e82c5610",
                4,
                &new_ids,
                "2026-11-13T00:00:00Z",
            )
            .expect("a duplicate");

        prop_assert_eq!(&copy.name, &source.name);
        prop_assert_eq!(copy.position, 4);
        prop_assert_eq!(copy.created_at.as_str(), "2026-11-13T00:00:00Z");
        prop_assert_eq!(copy.exercises.len(), source.exercises.len());
        for (source_exercise, copy_exercise) in source.exercises.iter().zip(&copy.exercises) {
            prop_assert_eq!(&copy_exercise.routine_id, "3f2504e0-4f89-41d3-9a0c-0305e82c5610");
            prop_assert_ne!(&copy_exercise.id, &source_exercise.id);
            prop_assert_eq!(&copy_exercise.exercise_id, &source_exercise.exercise_id);
            prop_assert_eq!(&copy_exercise.snapshot, &source_exercise.snapshot);
            prop_assert_eq!(copy_exercise.position, source_exercise.position);
            prop_assert_eq!(&copy_exercise.target, &source_exercise.target);
        }
    }

    #[test]
    fn req_rtn_003_editing_a_duplicate_keeps_the_source_unchanged(
        source in routine_strategy(),
        added_sets in 1u32..5,
    ) {
        let original_name = source.name.clone();
        let original_targets = source
            .exercises
            .iter()
            .map(|exercise| exercise.target.clone())
            .collect::<Vec<_>>();
        let original_positions = source
            .exercises
            .iter()
            .map(|exercise| exercise.position)
            .collect::<Vec<_>>();
        let new_ids = (0..source.exercises.len())
            .map(|index| format!("copy-exercise-{index}"))
            .collect::<Vec<_>>();
        let mut copy = source
            .duplicate(
                "3f2504e0-4f89-41d3-9a0c-0305e82c5610",
                4,
                &new_ids,
                "2026-11-13T00:00:00Z",
            )
            .expect("a duplicate");

        copy.name.push_str("-copy");
        for (index, exercise) in copy.exercises.iter_mut().enumerate() {
            exercise.position = 100 + index as i64;
            exercise.target.set_count = exercise.target.set_count.saturating_add(added_sets);
            exercise.snapshot.name = "複製した種目".to_owned();
        }

        prop_assert_eq!(source.name.as_str(), original_name.as_str());
        prop_assert_eq!(source.exercises.len(), original_targets.len());
        for ((exercise, target), position) in source
            .exercises
            .iter()
            .zip(&original_targets)
            .zip(&original_positions)
        {
            prop_assert_eq!(&exercise.target, target);
            prop_assert_eq!(exercise.position, *position);
            prop_assert_eq!(exercise.snapshot.name.as_str(), "ベンチプレス");
        }
    }
}
