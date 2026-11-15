//! マスターの種目への上書きの性質テスト。
//!
//! 対応する要求: `REQ-LIB-013`。
//! 上書きが設定した項目だけを置き換え、設定していない項目をマスターの値のままにすること、
//! 編集した内容の差を取って再度適用すると同じ内容になること、初期化でマスターへ戻ることを
//! 確かめる。

use proptest::prelude::*;
use warrior_core::exercise::{
    BodyPart, Equipment, Exercise, GripMode, GripSelection, GripStyle, GripWidth, Laterality,
    MuscleAssignment, MuscleRole, MuscleUnit, OtherScope, RecordMode,
};
use warrior_core::exercise_override::{apply_override, ExerciseOverride};

fn body_part() -> impl Strategy<Value = BodyPart> {
    prop::sample::select(BodyPart::ALL.to_vec())
}

fn equipment() -> impl Strategy<Value = Equipment> {
    prop::sample::select(Equipment::ALL.to_vec())
}

fn record_mode() -> impl Strategy<Value = RecordMode> {
    prop::sample::select(vec![
        RecordMode::WeightAndReps,
        RecordMode::BodyweightAndReps,
        RecordMode::Time,
    ])
}

fn laterality() -> impl Strategy<Value = Laterality> {
    prop::sample::select(vec![Laterality::Bilateral, Laterality::Unilateral])
}

fn grip_mode() -> impl Strategy<Value = GripMode> {
    prop::sample::select(vec![GripMode::None, GripMode::Grip, GripMode::Attachment])
}

fn other_scope() -> impl Strategy<Value = OtherScope> {
    prop::sample::select(vec![OtherScope::FullBody, OtherScope::LowerBody])
}

fn muscle_assignment() -> impl Strategy<Value = MuscleAssignment> {
    (
        prop::sample::select(MuscleUnit::ALL.to_vec()),
        prop::sample::select(vec![MuscleRole::Primary, MuscleRole::Secondary]),
    )
        .prop_map(|(muscle, role)| MuscleAssignment { muscle, role })
}

fn grip_option() -> impl Strategy<Value = GripSelection> {
    (
        prop::sample::select(vec![
            GripStyle::Pronated,
            GripStyle::Supinated,
            GripStyle::Parallel,
        ]),
        prop::sample::select(vec![GripWidth::Wide, GripWidth::Medium, GripWidth::Narrow]),
    )
        .prop_map(|(style, width)| GripSelection::Grip { style, width })
}

fn text() -> impl Strategy<Value = String> {
    "[a-z]{0,8}"
}

fn exercise() -> impl Strategy<Value = Exercise> {
    (
        text(),
        text(),
        body_part(),
        equipment(),
        record_mode(),
        laterality(),
        grip_mode(),
        prop::option::of(other_scope()),
        text(),
        text(),
        text(),
        (
            any::<i32>(),
            proptest::collection::vec(muscle_assignment(), 0..4),
            proptest::collection::vec(grip_option(), 0..4),
        ),
    )
        .prop_map(
            |(
                name,
                name_kana,
                body_part,
                equipment,
                record_mode,
                laterality,
                grip_mode,
                other_scope,
                instructions,
                cautions,
                tips,
                (sort_order, muscles, grip_options),
            )| {
                let (other_scope, muscles) = if body_part.is_other() {
                    (
                        Some(other_scope.unwrap_or(OtherScope::FullBody)),
                        Vec::new(),
                    )
                } else {
                    (None, muscles)
                };
                Exercise {
                    id: "3f2504e0-4f89-41d3-9a0c-0305e82c3301".to_owned(),
                    name,
                    name_kana,
                    body_part,
                    equipment,
                    record_mode,
                    laterality,
                    grip_mode,
                    other_scope,
                    instructions,
                    cautions,
                    tips,
                    sort_order,
                    muscles,
                    grip_options,
                }
            },
        )
}

fn override_value() -> impl Strategy<Value = ExerciseOverride> {
    (
        prop::option::of(text()),
        prop::option::of(text()),
        prop::option::of(body_part()),
        prop::option::of(equipment()),
        prop::option::of(record_mode()),
        prop::option::of(laterality()),
        prop::option::of(grip_mode()),
        prop::option::of(other_scope()),
        prop::option::of(text()),
        prop::option::of(text()),
        prop::option::of(text()),
        (
            prop::option::of(any::<i32>()),
            prop::option::of(proptest::collection::vec(muscle_assignment(), 0..4)),
            prop::option::of(proptest::collection::vec(grip_option(), 0..4)),
            any::<bool>(),
        ),
    )
        .prop_map(
            |(
                name,
                name_kana,
                body_part,
                equipment,
                record_mode,
                laterality,
                grip_mode,
                other_scope,
                instructions,
                cautions,
                tips,
                (sort_order, muscles, grip_options, deleted),
            )| ExerciseOverride {
                name,
                name_kana,
                body_part,
                equipment,
                record_mode,
                laterality,
                grip_mode,
                other_scope,
                instructions,
                cautions,
                tips,
                sort_order,
                muscles,
                grip_options,
                deleted,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn req_lib_013_applying_an_override_replaces_only_the_set_fields(
        master in exercise(),
        value in override_value(),
    ) {
        if value.deleted {
            prop_assert_eq!(apply_override(&master, &value), None);
            return Ok(());
        }
        let result = apply_override(&master, &value).expect("a non-deleted override keeps the exercise");
        prop_assert_eq!(&result.name, value.name.as_ref().unwrap_or(&master.name));
        prop_assert_eq!(&result.name_kana, value.name_kana.as_ref().unwrap_or(&master.name_kana));
        prop_assert_eq!(result.body_part, value.body_part.unwrap_or(master.body_part));
        prop_assert_eq!(result.equipment, value.equipment.unwrap_or(master.equipment));
        prop_assert_eq!(result.record_mode, value.record_mode.unwrap_or(master.record_mode));
        prop_assert_eq!(result.laterality, value.laterality.unwrap_or(master.laterality));
        prop_assert_eq!(result.grip_mode, value.grip_mode.unwrap_or(master.grip_mode));
        prop_assert_eq!(&result.instructions, value.instructions.as_ref().unwrap_or(&master.instructions));
        prop_assert_eq!(&result.cautions, value.cautions.as_ref().unwrap_or(&master.cautions));
        prop_assert_eq!(&result.tips, value.tips.as_ref().unwrap_or(&master.tips));
        prop_assert_eq!(result.sort_order, value.sort_order.unwrap_or(master.sort_order));
        prop_assert_eq!(&result.grip_options, value.grip_options.as_ref().unwrap_or(&master.grip_options));
        if result.body_part.is_other() {
            prop_assert!(result.muscles.is_empty());
            prop_assert_eq!(result.other_scope, value.other_scope.or(master.other_scope));
        } else {
            prop_assert_eq!(result.other_scope, None);
            prop_assert_eq!(&result.muscles, value.muscles.as_ref().unwrap_or(&master.muscles));
        }
    }

    #[test]
    fn req_lib_013_editing_and_resetting_returns_to_the_master(
        master in exercise(),
        value in override_value(),
    ) {
        let Some(edited) = apply_override(&master, &value) else {
            return Ok(());
        };
        let difference = ExerciseOverride::between(&master, &edited);
        prop_assert_eq!(apply_override(&master, &difference), Some(edited));
        prop_assert_eq!(apply_override(&master, &ExerciseOverride::default()), Some(master));
    }

    #[test]
    fn req_lib_013_an_override_without_changes_is_empty(master in exercise()) {
        prop_assert!(ExerciseOverride::between(&master, &master).is_empty());
    }
}
