//! 種目とスナップショットの性質テスト。
//!
//! 対応する要求: `REQ-LIB-002`、`REQ-LIB-004`、`REQ-LIB-011`、`REQ-LIB-012`。

use proptest::prelude::*;
use warrior_core::exercise::{
    Attachment, BodyPart, ExerciseSnapshot, GripMode, GripSelection, GripStyle, GripWidth,
    Laterality, MuscleAssignment, MuscleRole, MuscleUnit, RecordMode,
};

fn body_part() -> impl Strategy<Value = BodyPart> {
    prop_oneof![
        Just(BodyPart::Chest),
        Just(BodyPart::Back),
        Just(BodyPart::Shoulders),
        Just(BodyPart::BicepsForearm),
        Just(BodyPart::Triceps),
        Just(BodyPart::Legs),
        Just(BodyPart::Abs),
        Just(BodyPart::Other),
    ]
}

fn record_mode() -> impl Strategy<Value = RecordMode> {
    prop_oneof![
        Just(RecordMode::WeightAndReps),
        Just(RecordMode::BodyweightAndReps),
        Just(RecordMode::Time),
    ]
}

fn muscle() -> impl Strategy<Value = MuscleUnit> {
    prop::sample::select(MuscleUnit::ALL.to_vec())
}

fn grip_selection() -> impl Strategy<Value = GripSelection> {
    prop_oneof![
        Just(GripSelection::None),
        (
            prop_oneof![
                Just(GripStyle::Pronated),
                Just(GripStyle::Supinated),
                Just(GripStyle::Parallel)
            ],
            prop_oneof![
                Just(GripWidth::Wide),
                Just(GripWidth::Medium),
                Just(GripWidth::Narrow)
            ]
        )
            .prop_map(|(style, width)| GripSelection::Grip { style, width }),
        (
            prop_oneof![
                Just(Attachment::WideBar),
                Just(Attachment::StraightBar),
                Just(Attachment::ParallelGrip),
                Just(Attachment::MagNarrowParallel),
                Just(Attachment::MagNarrowOver),
                Just(Attachment::MagNarrowUnder),
                Just(Attachment::MagMediumParallel),
                Just(Attachment::MagMediumOver),
                Just(Attachment::MagMediumUnder),
                Just(Attachment::MagWideOver)
            ],
            prop_oneof![
                Just(None),
                Just(Some(GripStyle::Pronated)),
                Just(Some(GripStyle::Supinated)),
                Just(Some(GripStyle::Parallel))
            ]
        )
            .prop_map(|(attachment, style)| GripSelection::Attachment { attachment, style }),
    ]
}

proptest! {
    #[test]
    fn exercise_snapshots_round_trip(
        name in ".{0,64}",
        body_part in body_part(),
        muscles in proptest::collection::vec((muscle(), any::<bool>()), 0..8),
        record_mode in record_mode(),
        unilateral in any::<bool>(),
        grip_mode in prop_oneof![Just(GripMode::None), Just(GripMode::Grip), Just(GripMode::Attachment)],
        options in proptest::collection::vec(grip_selection(), 0..4),
    ) {
        let snapshot = ExerciseSnapshot {
            exercise_id: "exercise".to_owned(),
            name,
            body_part,
            muscles: muscles
                .into_iter()
                .map(|(muscle, primary)| MuscleAssignment {
                    muscle,
                    role: if primary { MuscleRole::Primary } else { MuscleRole::Secondary },
                })
                .collect(),
            record_mode,
            laterality: if unilateral { Laterality::Unilateral } else { Laterality::Bilateral },
            grip_mode,
            grip_options: options,
        };
        let json = serde_json::to_string(&snapshot).expect("serializable");
        prop_assert_eq!(serde_json::from_str::<ExerciseSnapshot>(&json).expect("deserializable"), snapshot);
    }

    #[test]
    fn every_muscle_unit_belongs_to_a_non_other_body_part(unit in muscle()) {
        prop_assert_ne!(unit.body_part(), BodyPart::Other);
        prop_assert!(!unit.id().is_empty());
    }

    #[test]
    fn grip_selections_only_match_their_mode(
        mode in prop_oneof![Just(GripMode::None), Just(GripMode::Grip), Just(GripMode::Attachment)],
        selection in grip_selection(),
    ) {
        let valid = selection.validate_for(mode).is_ok();
        let expected = match (mode, selection) {
            (GripMode::None, GripSelection::None) => true,
            (GripMode::Grip, GripSelection::Grip { .. }) => true,
            (GripMode::Attachment, GripSelection::Attachment { attachment, style }) => {
                attachment.requires_style() == style.is_some()
            }
            _ => false,
        };
        prop_assert_eq!(valid, expected);
    }
}
