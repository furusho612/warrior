//! 種目のブラウザテストが共有する種目。

use warrior_core::exercise::{
    Attachment, BodyPart, Equipment, Exercise, GripMode, GripSelection, GripStyle, Laterality,
    MuscleAssignment, MuscleRole, MuscleUnit, RecordMode,
};
use warrior_frontend::body_figure::SEATED_ROW_EXERCISE_ID;

/// シーテッドロウの種目。ブラウザテストに必要なグリップの選択肢を持つ。
pub fn seated_row() -> Exercise {
    Exercise {
        id: SEATED_ROW_EXERCISE_ID.to_owned(),
        name: "シーテッドロウ".to_owned(),
        name_kana: "しーてっどろう".to_owned(),
        body_part: BodyPart::Back,
        equipment: Equipment::Cable,
        record_mode: RecordMode::WeightAndReps,
        laterality: Laterality::Bilateral,
        grip_mode: GripMode::Attachment,
        other_scope: None,
        instructions: String::new(),
        cautions: String::new(),
        tips: String::new(),
        sort_order: 27,
        muscles: vec![
            MuscleAssignment {
                muscle: MuscleUnit::LatissimusDorsi,
                role: MuscleRole::Primary,
            },
            MuscleAssignment {
                muscle: MuscleUnit::TrapeziusMiddleLower,
                role: MuscleRole::Primary,
            },
            MuscleAssignment {
                muscle: MuscleUnit::BicepsBrachii,
                role: MuscleRole::Secondary,
            },
            MuscleAssignment {
                muscle: MuscleUnit::PosteriorDeltoid,
                role: MuscleRole::Secondary,
            },
        ],
        grip_options: vec![
            attachment(Attachment::WideBar, Some(GripStyle::Pronated)),
            attachment(Attachment::ParallelGrip, None),
        ],
    }
}

fn attachment(attachment: Attachment, style: Option<GripStyle>) -> GripSelection {
    GripSelection::Attachment { attachment, style }
}
