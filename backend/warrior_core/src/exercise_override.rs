//! マスターの種目に適用する利用者ごとの上書き (REQ-LIB-013)。
//!
//! 上書きは、利用者が編集した項目だけを持つ。`None` の項目はマスターの値を使う。
//! 初期の内容に戻す操作は上書きを消すため、適用する上書きが無い種目はマスターのままになる。
//! 削除の上書きは種目を一覧と詳細から隠し、マスターの行は変えない。

use serde::{Deserialize, Serialize};

use crate::exercise::{
    BodyPart, Equipment, Exercise, GripMode, GripSelection, Laterality, MuscleAssignment,
    OtherScope, RecordMode,
};

/// マスターの種目への上書き。`None` の項目はマスターの値を使う。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExerciseOverride {
    pub name: Option<String>,
    pub name_kana: Option<String>,
    pub body_part: Option<BodyPart>,
    pub equipment: Option<Equipment>,
    pub record_mode: Option<RecordMode>,
    pub laterality: Option<Laterality>,
    pub grip_mode: Option<GripMode>,
    pub other_scope: Option<OtherScope>,
    pub instructions: Option<String>,
    pub cautions: Option<String>,
    pub tips: Option<String>,
    pub sort_order: Option<i32>,
    pub muscles: Option<Vec<MuscleAssignment>>,
    pub grip_options: Option<Vec<GripSelection>>,
    #[serde(default)]
    pub deleted: bool,
}

impl ExerciseOverride {
    /// マスターと編集後の種目の差を取る。値が同じ項目は上書きに含めない。
    pub fn between(master: &Exercise, edited: &Exercise) -> Self {
        Self {
            name: changed(&master.name, &edited.name),
            name_kana: changed(&master.name_kana, &edited.name_kana),
            body_part: changed(&master.body_part, &edited.body_part),
            equipment: changed(&master.equipment, &edited.equipment),
            record_mode: changed(&master.record_mode, &edited.record_mode),
            laterality: changed(&master.laterality, &edited.laterality),
            grip_mode: changed(&master.grip_mode, &edited.grip_mode),
            other_scope: edited
                .other_scope
                .filter(|scope| Some(*scope) != master.other_scope),
            instructions: changed(&master.instructions, &edited.instructions),
            cautions: changed(&master.cautions, &edited.cautions),
            tips: changed(&master.tips, &edited.tips),
            sort_order: changed(&master.sort_order, &edited.sort_order),
            muscles: changed(&master.muscles, &edited.muscles),
            grip_options: changed(&master.grip_options, &edited.grip_options),
            deleted: false,
        }
    }

    /// 削除の上書きを作る。
    pub fn deleted() -> Self {
        Self {
            deleted: true,
            ..Self::default()
        }
    }

    /// 項目を 1 つも持たないか。削除の上書きは項目を持たなくても意味がある。
    pub fn is_empty(&self) -> bool {
        !self.deleted
            && self.name.is_none()
            && self.name_kana.is_none()
            && self.body_part.is_none()
            && self.equipment.is_none()
            && self.record_mode.is_none()
            && self.laterality.is_none()
            && self.grip_mode.is_none()
            && self.other_scope.is_none()
            && self.instructions.is_none()
            && self.cautions.is_none()
            && self.tips.is_none()
            && self.sort_order.is_none()
            && self.muscles.is_none()
            && self.grip_options.is_none()
    }
}

/// マスターに上書きを適用する。削除の上書きなら `None` を返す。
///
/// 部位区分をその他へ変えたときは筋肉を持てないため、筋肉を空にする。
/// その他から別の部位区分へ変えたときは範囲を持てないため、範囲を空にする。
pub fn apply_override(master: &Exercise, value: &ExerciseOverride) -> Option<Exercise> {
    if value.deleted {
        return None;
    }
    let mut exercise = master.clone();
    if let Some(name) = &value.name {
        exercise.name = name.clone();
    }
    if let Some(name_kana) = &value.name_kana {
        exercise.name_kana = name_kana.clone();
    }
    if let Some(body_part) = value.body_part {
        exercise.body_part = body_part;
    }
    if let Some(equipment) = value.equipment {
        exercise.equipment = equipment;
    }
    if let Some(record_mode) = value.record_mode {
        exercise.record_mode = record_mode;
    }
    if let Some(laterality) = value.laterality {
        exercise.laterality = laterality;
    }
    if let Some(grip_mode) = value.grip_mode {
        exercise.grip_mode = grip_mode;
    }
    if let Some(other_scope) = value.other_scope {
        exercise.other_scope = Some(other_scope);
    }
    if let Some(instructions) = &value.instructions {
        exercise.instructions = instructions.clone();
    }
    if let Some(cautions) = &value.cautions {
        exercise.cautions = cautions.clone();
    }
    if let Some(tips) = &value.tips {
        exercise.tips = tips.clone();
    }
    if let Some(sort_order) = value.sort_order {
        exercise.sort_order = sort_order;
    }
    if let Some(muscles) = &value.muscles {
        exercise.muscles = muscles.clone();
    }
    if let Some(grip_options) = &value.grip_options {
        exercise.grip_options = grip_options.clone();
    }
    if exercise.body_part.is_other() {
        exercise.muscles.clear();
    } else {
        exercise.other_scope = None;
    }
    Some(exercise)
}

fn changed<T: PartialEq + Clone>(master: &T, edited: &T) -> Option<T> {
    (master != edited).then(|| edited.clone())
}

#[cfg(test)]
mod tests {
    //! 上書きの適用と差の例示テスト。
    //!
    //! 対応する要求: `REQ-LIB-013`。

    use super::{apply_override, ExerciseOverride};
    use crate::exercise::{
        BodyPart, Equipment, Exercise, GripMode, Laterality, MuscleAssignment, MuscleRole,
        MuscleUnit, OtherScope, RecordMode,
    };

    fn exercise() -> Exercise {
        Exercise {
            id: "3f2504e0-4f89-41d3-9a0c-0305e82c3301".to_owned(),
            name: "ベンチプレス".to_owned(),
            name_kana: "べんちぷれす".to_owned(),
            body_part: BodyPart::Chest,
            equipment: Equipment::Barbell,
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip_mode: GripMode::None,
            other_scope: None,
            instructions: "押す".to_owned(),
            cautions: "肩を守る".to_owned(),
            tips: "肩甲骨を寄せる".to_owned(),
            sort_order: 1,
            muscles: vec![MuscleAssignment {
                muscle: MuscleUnit::PectoralisMajorMiddle,
                role: MuscleRole::Primary,
            }],
            grip_options: Vec::new(),
        }
    }

    #[test]
    fn req_lib_013_an_edit_replaces_only_the_changed_fields() {
        let master = exercise();
        let mut edited = master.clone();
        edited.name = "ベンチプレス(ワイド)".to_owned();
        edited.tips = "肘を開く".to_owned();
        let value = ExerciseOverride::between(&master, &edited);
        let applied = apply_override(&master, &value).expect("an edited exercise");
        assert_eq!(applied.name, "ベンチプレス(ワイド)");
        assert_eq!(applied.tips, "肘を開く");
        assert_eq!(applied.name_kana, master.name_kana);
        assert_eq!(applied.instructions, master.instructions);
        assert_eq!(applied.muscles, master.muscles);
    }

    #[test]
    fn req_lib_013_resetting_returns_the_master() {
        let master = exercise();
        let mut edited = master.clone();
        edited.name = "別の名前".to_owned();
        let value = ExerciseOverride::between(&master, &edited);
        assert_eq!(
            apply_override(&master, &value)
                .expect("an edited exercise")
                .name,
            "別の名前"
        );
        assert_eq!(
            apply_override(&master, &ExerciseOverride::default()),
            Some(master)
        );
    }

    #[test]
    fn req_lib_013_a_deleted_override_hides_the_master() {
        let master = exercise();
        assert_eq!(apply_override(&master, &ExerciseOverride::deleted()), None);
    }

    #[test]
    fn req_lib_013_changing_to_other_clears_the_muscles_and_the_scope() {
        let master = exercise();
        let mut edited = master.clone();
        edited.body_part = BodyPart::Other;
        edited.other_scope = Some(OtherScope::FullBody);
        edited.muscles.clear();
        let value = ExerciseOverride::between(&master, &edited);
        let applied = apply_override(&master, &value).expect("an edited exercise");
        assert_eq!(applied.body_part, BodyPart::Other);
        assert_eq!(applied.other_scope, Some(OtherScope::FullBody));
        assert!(applied.muscles.is_empty());

        let mut other = master.clone();
        other.body_part = BodyPart::Other;
        other.other_scope = Some(OtherScope::LowerBody);
        other.muscles.clear();
        let mut edited = other.clone();
        edited.body_part = BodyPart::Chest;
        edited.other_scope = None;
        edited.muscles = master.muscles.clone();
        let value = ExerciseOverride::between(&other, &edited);
        let applied = apply_override(&other, &value).expect("an edited exercise");
        assert_eq!(applied.body_part, BodyPart::Chest);
        assert_eq!(applied.other_scope, None);
        assert_eq!(applied.muscles, master.muscles);
    }

    #[test]
    fn req_lib_013_an_equal_exercise_produces_an_empty_override() {
        let master = exercise();
        assert!(ExerciseOverride::between(&master, &master).is_empty());
    }
}
