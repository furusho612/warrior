//! 代わりの種目の候補の選択と並び順 (PRD の「代わりの種目」)。
//!
//! 候補は種目ライブラリのうち、元の種目と同じ部位区分を持ち、主働筋が 1 つ以上
//! 重なる種目とする。並び順は、主働筋の重なりが多い順、使う器具が違う順、過去に
//! 記録のある順とし、同じ順位は種目ライブラリの表示順で決める。
//! 元の種目と同じ種目は候補に含めない。候補は AI を呼ばず、この関数だけで決める。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::exercise::{BodyPart, Equipment, Exercise, MuscleAssignment, MuscleRole, MuscleUnit};
use crate::library::kana_sort_key;

/// 候補の並びの基準になる、記録中の種目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternativeOrigin {
    pub exercise_id: String,
    pub body_part: BodyPart,
    pub equipment: Equipment,
    pub primary_muscles: Vec<MuscleUnit>,
}

impl From<&Exercise> for AlternativeOrigin {
    fn from(exercise: &Exercise) -> Self {
        Self {
            exercise_id: exercise.id.clone(),
            body_part: exercise.body_part,
            equipment: exercise.equipment,
            primary_muscles: primary_muscles(&exercise.muscles),
        }
    }
}

/// 代わりの種目の候補の 1 件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlternativeExercise {
    /// 種目の ID。
    pub exercise_id: String,
    /// 種目名。
    pub name: String,
    /// 部位区分。
    pub body_part: BodyPart,
    /// 使用器具。
    pub equipment: Equipment,
    /// 元の種目と重なる主働筋の数。
    pub shared_primary_muscles: usize,
    /// 過去に記録があるか。
    pub recorded: bool,
}

/// 代わりの種目の候補の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlternativeList {
    pub exercises: Vec<AlternativeExercise>,
}

/// 過去に記録した種目の ID の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedExercises {
    pub exercise_ids: Vec<String>,
}

/// 種目の主働筋の単位を返す。
pub fn primary_muscles(muscles: &[MuscleAssignment]) -> Vec<MuscleUnit> {
    muscles
        .iter()
        .filter(|assignment| assignment.role == MuscleRole::Primary)
        .map(|assignment| assignment.muscle)
        .collect()
}

/// 候補を選び、並べる。
///
/// `recorded` は過去に記録した種目の ID の集合である。同じ部位区分で主働筋が
/// 重ならない種目と、元の種目自身は候補にしない。
pub fn select_alternatives(
    origin: &AlternativeOrigin,
    library: &[Exercise],
    recorded: &HashSet<String>,
) -> Vec<AlternativeExercise> {
    let mut ranked = library
        .iter()
        .filter(|candidate| candidate.id != origin.exercise_id)
        .filter(|candidate| candidate.body_part == origin.body_part)
        .filter_map(|candidate| {
            let shared_primary_muscles = shared_primary_muscle_count(origin, &candidate.muscles);
            (shared_primary_muscles > 0).then(|| RankedAlternative {
                exercise: AlternativeExercise {
                    exercise_id: candidate.id.clone(),
                    name: candidate.name.clone(),
                    body_part: candidate.body_part,
                    equipment: candidate.equipment,
                    shared_primary_muscles,
                    recorded: recorded.contains(&candidate.id),
                },
                name_kana: candidate.name_kana.clone(),
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| compare(origin, left, right));
    ranked.into_iter().map(|ranked| ranked.exercise).collect()
}

/// 元の種目の主働筋と候補の主働筋が重なる数を返す。
fn shared_primary_muscle_count(origin: &AlternativeOrigin, muscles: &[MuscleAssignment]) -> usize {
    primary_muscles(muscles)
        .iter()
        .filter(|muscle| origin.primary_muscles.contains(muscle))
        .count()
}

struct RankedAlternative {
    exercise: AlternativeExercise,
    name_kana: String,
}

fn compare(
    origin: &AlternativeOrigin,
    left: &RankedAlternative,
    right: &RankedAlternative,
) -> std::cmp::Ordering {
    right
        .exercise
        .shared_primary_muscles
        .cmp(&left.exercise.shared_primary_muscles)
        .then_with(|| {
            equipment_difference_rank(origin, &left.exercise)
                .cmp(&equipment_difference_rank(origin, &right.exercise))
        })
        .then_with(|| right.exercise.recorded.cmp(&left.exercise.recorded))
        .then_with(|| {
            left.exercise
                .body_part
                .order()
                .cmp(&right.exercise.body_part.order())
        })
        .then_with(|| {
            left.exercise
                .equipment
                .order()
                .cmp(&right.exercise.equipment.order())
        })
        .then_with(|| kana_sort_key(&left.name_kana).cmp(&kana_sort_key(&right.name_kana)))
        .then_with(|| left.exercise.exercise_id.cmp(&right.exercise.exercise_id))
}

/// 器具が元の種目と違えば 0、同じなら 1 を返す。
fn equipment_difference_rank(origin: &AlternativeOrigin, candidate: &AlternativeExercise) -> u8 {
    if candidate.equipment == origin.equipment {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    //! 代わりの種目の候補の選択と並び順。
    //!
    //! 対応する要求: `REQ-ALT-001`、`REQ-ALT-002`、`REQ-ALT-003`。

    use std::collections::HashSet;

    use crate::exercise::{
        BodyPart, Equipment, Exercise, GripMode, Laterality, MuscleAssignment, MuscleRole,
        MuscleUnit, RecordMode,
    };

    use super::{select_alternatives, AlternativeExercise, AlternativeOrigin};

    fn exercise(
        id: &str,
        name_kana: &str,
        equipment: Equipment,
        body_part: BodyPart,
        muscles: &[(MuscleUnit, MuscleRole)],
    ) -> Exercise {
        Exercise {
            id: id.to_owned(),
            name: id.to_owned(),
            name_kana: name_kana.to_owned(),
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
            muscles: muscles
                .iter()
                .map(|(muscle, role)| MuscleAssignment {
                    muscle: *muscle,
                    role: *role,
                })
                .collect(),
            grip_options: Vec::new(),
        }
    }

    fn bench_press() -> Exercise {
        exercise(
            "bench",
            "べんちぷれす",
            Equipment::Barbell,
            BodyPart::Chest,
            &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
        )
    }

    fn ids(alternatives: &[AlternativeExercise]) -> Vec<&str> {
        alternatives
            .iter()
            .map(|alternative| alternative.exercise_id.as_str())
            .collect()
    }

    #[test]
    fn candidates_need_a_shared_primary_muscle_in_the_same_body_part() {
        let origin = AlternativeOrigin::from(&bench_press());
        let library = vec![
            bench_press(),
            exercise(
                "full-overlap",
                "ふる",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "partial-overlap",
                "ぱーしゃる",
                Equipment::Cable,
                BodyPart::Chest,
                &[
                    (MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary),
                    (MuscleUnit::TricepsBrachii, MuscleRole::Primary),
                ],
            ),
            exercise(
                "secondary-only",
                "せかんど",
                Equipment::Machine,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Secondary)],
            ),
            exercise(
                "other-body-part",
                "あざー",
                Equipment::Dumbbell,
                BodyPart::Back,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
        ];
        let alternatives = select_alternatives(&origin, &library, &HashSet::new());
        assert_eq!(ids(&alternatives), vec!["full-overlap", "partial-overlap"]);
    }

    #[test]
    fn candidates_with_different_equipment_come_before_the_same_equipment() {
        let origin = AlternativeOrigin::from(&bench_press());
        let library = vec![
            bench_press(),
            exercise(
                "same-equipment",
                "あ",
                Equipment::Barbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "different-equipment",
                "い",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
        ];
        let alternatives = select_alternatives(&origin, &library, &HashSet::new());
        assert_eq!(
            ids(&alternatives),
            vec!["different-equipment", "same-equipment"]
        );
    }

    #[test]
    fn recorded_candidates_come_before_unrecorded_ones_with_the_same_rank() {
        let origin = AlternativeOrigin::from(&bench_press());
        let library = vec![
            bench_press(),
            exercise(
                "unrecorded",
                "あ",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "recorded",
                "い",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
        ];
        let recorded = HashSet::from(["recorded".to_owned()]);
        let alternatives = select_alternatives(&origin, &library, &recorded);
        assert_eq!(ids(&alternatives), vec!["recorded", "unrecorded"]);
    }

    #[test]
    fn candidates_with_the_same_rank_follow_the_library_order() {
        let origin = AlternativeOrigin::from(&bench_press());
        let library = vec![
            bench_press(),
            exercise(
                "cable",
                "けーぶる",
                Equipment::Cable,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "dumbbell-b",
                "だんべるぷれす",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "dumbbell-a",
                "だんべるふらい",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "barbell",
                "ばーべる",
                Equipment::Barbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
        ];
        let alternatives = select_alternatives(&origin, &library, &HashSet::new());
        assert_eq!(
            ids(&alternatives),
            vec!["dumbbell-a", "dumbbell-b", "cable", "barbell"]
        );
    }

    #[test]
    fn a_candidate_with_more_shared_primary_muscles_comes_first() {
        let origin = AlternativeOrigin::from(&exercise(
            "origin",
            "おりじん",
            Equipment::Barbell,
            BodyPart::Chest,
            &[
                (MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary),
                (MuscleUnit::PectoralisMajorUpper, MuscleRole::Primary),
            ],
        ));
        let library = vec![
            exercise(
                "one",
                "あ",
                Equipment::Dumbbell,
                BodyPart::Chest,
                &[(MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary)],
            ),
            exercise(
                "two",
                "い",
                Equipment::Cable,
                BodyPart::Chest,
                &[
                    (MuscleUnit::PectoralisMajorMiddle, MuscleRole::Primary),
                    (MuscleUnit::PectoralisMajorUpper, MuscleRole::Primary),
                ],
            ),
        ];
        let alternatives = select_alternatives(&origin, &library, &HashSet::new());
        assert_eq!(ids(&alternatives), vec!["two", "one"]);
        assert_eq!(alternatives[0].shared_primary_muscles, 2);
        assert_eq!(alternatives[1].shared_primary_muscles, 1);
    }
}
