//! 代わりの種目の候補の選択と並び順の性質テスト。
//!
//! 対応する要求: `REQ-ALT-001`、`REQ-ALT-002`、`REQ-ALT-003`。
//! 候補が元の種目と重ならず、同じ部位区分で主働筋が重なる種目だけを含むこと、
//! 並びが主働筋の重なり、器具の違い、過去の記録の有無、種目ライブラリの表示順の
//! 比較と一致することを確かめる。

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use warrior_core::alternative::{select_alternatives, AlternativeExercise, AlternativeOrigin};
use warrior_core::exercise::{
    BodyPart, Equipment, Exercise, GripMode, Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
    RecordMode,
};
use warrior_core::library::kana_sort_key;

fn body_part() -> impl Strategy<Value = BodyPart> {
    prop::sample::select(BodyPart::ALL.to_vec())
}

fn equipment() -> impl Strategy<Value = Equipment> {
    prop::sample::select(Equipment::ALL.to_vec())
}

fn muscle() -> impl Strategy<Value = MuscleUnit> {
    prop::sample::select(MuscleUnit::ALL.to_vec())
}

fn kana() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::char::range('ぁ', 'ん'), 0..6)
        .prop_map(|characters| characters.into_iter().collect())
}

fn muscles() -> impl Strategy<Value = Vec<MuscleAssignment>> {
    proptest::collection::vec((muscle(), any::<bool>()), 0..5).prop_map(|items| {
        let mut seen = Vec::new();
        items
            .into_iter()
            .filter_map(|(muscle, primary)| {
                if seen.contains(&muscle) {
                    return None;
                }
                seen.push(muscle);
                Some(MuscleAssignment {
                    muscle,
                    role: if primary {
                        MuscleRole::Primary
                    } else {
                        MuscleRole::Secondary
                    },
                })
            })
            .collect()
    })
}

fn exercise_fields() -> impl Strategy<Value = Exercise> {
    (kana(), kana(), body_part(), equipment(), muscles()).prop_map(
        |(name, name_kana, body_part, equipment, muscles)| Exercise {
            id: String::new(),
            name,
            name_kana,
            body_part,
            equipment,
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode: GripMode::None,
            other_scope: None,
            instructions: String::new(),
            cautions: String::new(),
            tips: String::new(),
            sort_order: 0,
            muscles,
            grip_options: Vec::new(),
        },
    )
}

fn library_and_recorded() -> impl Strategy<Value = (Vec<Exercise>, HashSet<String>)> {
    proptest::collection::vec((exercise_fields(), any::<bool>()), 0..20).prop_map(|items| {
        let mut library = Vec::new();
        let mut recorded = HashSet::new();
        for (index, (mut exercise, is_recorded)) in items.into_iter().enumerate() {
            exercise.id = format!("exercise-{index}");
            if is_recorded {
                recorded.insert(exercise.id.clone());
            }
            library.push(exercise);
        }
        (library, recorded)
    })
}

fn shared_primary_muscles(origin: &AlternativeOrigin, exercise: &Exercise) -> usize {
    exercise
        .muscles
        .iter()
        .filter(|assignment| assignment.role == MuscleRole::Primary)
        .filter(|assignment| origin.primary_muscles.contains(&assignment.muscle))
        .count()
}

fn order_key(
    origin: &AlternativeOrigin,
    alternative: &AlternativeExercise,
    name_kana: &str,
) -> (Reverse<usize>, u8, bool, u8, u8, String, String) {
    let equipment_rank = if alternative.equipment == origin.equipment {
        1
    } else {
        0
    };
    (
        Reverse(alternative.shared_primary_muscles),
        equipment_rank,
        !alternative.recorded,
        alternative.body_part.order(),
        alternative.equipment.order(),
        kana_sort_key(name_kana),
        alternative.exercise_id.clone(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn candidates_are_exactly_the_exercises_with_shared_primary_muscles(
        origin_exercise in exercise_fields(),
        (library, recorded) in library_and_recorded(),
    ) {
        let origin = AlternativeOrigin::from(&origin_exercise);
        let alternatives = select_alternatives(&origin, &library, &recorded);
        for alternative in &alternatives {
            prop_assert_ne!(&alternative.exercise_id, &origin.exercise_id);
            prop_assert_eq!(alternative.body_part, origin.body_part);
            prop_assert!(alternative.shared_primary_muscles > 0);
            prop_assert_eq!(
                alternative.recorded,
                recorded.contains(&alternative.exercise_id)
            );
        }
        let mut expected = library
            .iter()
            .filter(|exercise| exercise.id.as_str() != origin.exercise_id.as_str())
            .filter(|exercise| exercise.body_part == origin.body_part)
            .filter(|exercise| shared_primary_muscles(&origin, exercise) > 0)
            .map(|exercise| exercise.id.clone())
            .collect::<Vec<_>>();
        expected.sort();
        let mut actual = alternatives
            .iter()
            .map(|alternative| alternative.exercise_id.clone())
            .collect::<Vec<_>>();
        actual.sort();
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn the_order_matches_the_comparison_of_the_overlap_equipment_record_and_library_order(
        origin_exercise in exercise_fields(),
        (library, recorded) in library_and_recorded(),
    ) {
        let origin = AlternativeOrigin::from(&origin_exercise);
        let alternatives = select_alternatives(&origin, &library, &recorded);
        let name_kanas = library
            .iter()
            .map(|exercise| (exercise.id.clone(), exercise.name_kana.clone()))
            .collect::<HashMap<_, _>>();
        for pair in alternatives.windows(2) {
            let left = order_key(
                &origin,
                &pair[0],
                name_kanas.get(&pair[0].exercise_id).map_or("", String::as_str),
            );
            let right = order_key(
                &origin,
                &pair[1],
                name_kanas.get(&pair[1].exercise_id).map_or("", String::as_str),
            );
            prop_assert!(left <= right);
        }
        let mut expected = alternatives.clone();
        expected.sort_by_key(|alternative| {
            order_key(
                &origin,
                alternative,
                name_kanas.get(&alternative.exercise_id).map_or("", String::as_str),
            )
        });
        prop_assert_eq!(alternatives, expected);
    }
}
