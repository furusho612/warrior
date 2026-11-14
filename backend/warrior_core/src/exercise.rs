//! 種目マスターと筋肉の単位の型と検証 (PRD の「種目ライブラリ」「筋肉の単位」)。

use serde::{Deserialize, Serialize};

pub use crate::set_calc::RecordMode;

/// 部位区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyPart {
    Chest,
    Back,
    Shoulders,
    BicepsForearm,
    Triceps,
    Legs,
    Abs,
    Other,
}

impl BodyPart {
    /// 8 区分。表示と集計の並びを固定する。
    pub const ALL: [Self; 8] = [
        Self::Chest,
        Self::Back,
        Self::Shoulders,
        Self::BicepsForearm,
        Self::Triceps,
        Self::Legs,
        Self::Abs,
        Self::Other,
    ];

    /// その他か。
    pub fn is_other(self) -> bool {
        self == Self::Other
    }

    /// DB と API の文字列を部位区分に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|part| part.as_str() == text)
    }

    /// DB と API の文字列。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chest => "chest",
            Self::Back => "back",
            Self::Shoulders => "shoulders",
            Self::BicepsForearm => "biceps_forearm",
            Self::Triceps => "triceps",
            Self::Legs => "legs",
            Self::Abs => "abs",
            Self::Other => "other",
        }
    }
}

/// 使用器具。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Equipment {
    Barbell,
    Dumbbell,
    Machine,
    Cable,
    Bodyweight,
    Other,
}

impl Equipment {
    /// DB と API の文字列。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Barbell => "barbell",
            Self::Dumbbell => "dumbbell",
            Self::Machine => "machine",
            Self::Cable => "cable",
            Self::Bodyweight => "bodyweight",
            Self::Other => "other",
        }
    }
}

/// 左右別に行うか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Laterality {
    Bilateral,
    Unilateral,
}

/// グリップの指定の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GripMode {
    None,
    Grip,
    Attachment,
}

/// 握り方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GripStyle {
    Pronated,
    Supinated,
    Parallel,
}

impl GripStyle {
    /// 3 種類。
    pub const ALL: [Self; 3] = [Self::Pronated, Self::Supinated, Self::Parallel];

    /// DB と比較のキーに載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pronated => "pronated",
            Self::Supinated => "supinated",
            Self::Parallel => "parallel",
        }
    }
}

/// 手幅。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GripWidth {
    Wide,
    Medium,
    Narrow,
}

impl GripWidth {
    /// 3 種類。
    pub const ALL: [Self; 3] = [Self::Wide, Self::Medium, Self::Narrow];

    /// DB と比較のキーに載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Medium => "medium",
            Self::Narrow => "narrow",
        }
    }
}

/// アタッチメント。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attachment {
    WideBar,
    StraightBar,
    ParallelGrip,
    MagNarrowParallel,
    MagNarrowOver,
    MagNarrowUnder,
    MagMediumParallel,
    MagMediumOver,
    MagMediumUnder,
    MagWideOver,
}

impl Attachment {
    /// 10 種類。
    pub const ALL: [Self; 10] = [
        Self::WideBar,
        Self::StraightBar,
        Self::ParallelGrip,
        Self::MagNarrowParallel,
        Self::MagNarrowOver,
        Self::MagNarrowUnder,
        Self::MagMediumParallel,
        Self::MagMediumOver,
        Self::MagMediumUnder,
        Self::MagWideOver,
    ];

    /// 手幅が狭いアタッチメントか。
    pub fn is_narrow(self) -> bool {
        matches!(
            self,
            Self::ParallelGrip
                | Self::MagNarrowParallel
                | Self::MagNarrowOver
                | Self::MagNarrowUnder
        )
    }

    /// 順手か逆手かを選ぶアタッチメントか。
    pub fn requires_style(self) -> bool {
        matches!(self, Self::WideBar | Self::StraightBar)
    }

    /// DB と API の文字列。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WideBar => "wide_bar",
            Self::StraightBar => "straight_bar",
            Self::ParallelGrip => "parallel_grip",
            Self::MagNarrowParallel => "mag_narrow_parallel",
            Self::MagNarrowOver => "mag_narrow_over",
            Self::MagNarrowUnder => "mag_narrow_under",
            Self::MagMediumParallel => "mag_medium_parallel",
            Self::MagMediumOver => "mag_medium_over",
            Self::MagMediumUnder => "mag_medium_under",
            Self::MagWideOver => "mag_wide_over",
        }
    }
}

/// 記録時に選ぶグリップ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GripSelection {
    None,
    Grip {
        style: GripStyle,
        width: GripWidth,
    },
    Attachment {
        attachment: Attachment,
        style: Option<GripStyle>,
    },
}

impl GripSelection {
    /// 自己ベストと前回の比較に使う (グリップ、アタッチメント) のキー。
    pub fn comparison_key(self) -> (Option<String>, Option<String>) {
        match self {
            Self::None => (None, None),
            Self::Grip { style, width } => {
                (Some(format!("{}_{}", style.as_str(), width.as_str())), None)
            }
            Self::Attachment { attachment, style } => (
                style.map(|style| style.as_str().to_owned()),
                Some(attachment.as_str().to_owned()),
            ),
        }
    }

    /// 種目のグリップの種類に合うかを検証する。
    pub fn validate_for(self, mode: GripMode) -> Result<(), ExerciseError> {
        match (mode, self) {
            (GripMode::None, GripSelection::None) => Ok(()),
            (GripMode::Grip, GripSelection::Grip { .. }) => Ok(()),
            (GripMode::Attachment, GripSelection::Attachment { attachment, style }) => {
                if attachment.requires_style() && style.is_none() {
                    return Err(ExerciseError::AttachmentStyleMissing);
                }
                if !attachment.requires_style() && style.is_some() {
                    return Err(ExerciseError::UnexpectedAttachmentStyle);
                }
                Ok(())
            }
            _ => Err(ExerciseError::GripSelectionMismatch),
        }
    }
}

/// その他の種目の範囲。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtherScope {
    FullBody,
    LowerBody,
}

/// 筋肉の単位。PRD の 24 単位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MuscleUnit {
    PectoralisMajorUpper,
    PectoralisMajorMiddle,
    PectoralisMajorLower,
    LatissimusDorsi,
    TeresMajor,
    TrapeziusUpper,
    TrapeziusMiddleLower,
    ErectorSpinae,
    AnteriorDeltoid,
    LateralDeltoid,
    PosteriorDeltoid,
    BicepsBrachii,
    Forearm,
    TricepsBrachii,
    Quadriceps,
    Hamstrings,
    GluteusMaximus,
    GluteusMedius,
    Adductors,
    Gastrocnemius,
    Soleus,
    RectusAbdominisUpper,
    RectusAbdominisLower,
    Obliques,
}

impl MuscleUnit {
    /// 24 単位。
    pub const ALL: [Self; 24] = [
        Self::PectoralisMajorUpper,
        Self::PectoralisMajorMiddle,
        Self::PectoralisMajorLower,
        Self::LatissimusDorsi,
        Self::TeresMajor,
        Self::TrapeziusUpper,
        Self::TrapeziusMiddleLower,
        Self::ErectorSpinae,
        Self::AnteriorDeltoid,
        Self::LateralDeltoid,
        Self::PosteriorDeltoid,
        Self::BicepsBrachii,
        Self::Forearm,
        Self::TricepsBrachii,
        Self::Quadriceps,
        Self::Hamstrings,
        Self::GluteusMaximus,
        Self::GluteusMedius,
        Self::Adductors,
        Self::Gastrocnemius,
        Self::Soleus,
        Self::RectusAbdominisUpper,
        Self::RectusAbdominisLower,
        Self::Obliques,
    ];

    /// 対応する部位区分。
    pub fn body_part(self) -> BodyPart {
        match self {
            Self::PectoralisMajorUpper
            | Self::PectoralisMajorMiddle
            | Self::PectoralisMajorLower => BodyPart::Chest,
            Self::LatissimusDorsi
            | Self::TeresMajor
            | Self::TrapeziusUpper
            | Self::TrapeziusMiddleLower
            | Self::ErectorSpinae => BodyPart::Back,
            Self::AnteriorDeltoid | Self::LateralDeltoid | Self::PosteriorDeltoid => {
                BodyPart::Shoulders
            }
            Self::BicepsBrachii | Self::Forearm => BodyPart::BicepsForearm,
            Self::TricepsBrachii => BodyPart::Triceps,
            Self::Quadriceps
            | Self::Hamstrings
            | Self::GluteusMaximus
            | Self::GluteusMedius
            | Self::Adductors
            | Self::Gastrocnemius
            | Self::Soleus => BodyPart::Legs,
            Self::RectusAbdominisUpper | Self::RectusAbdominisLower | Self::Obliques => {
                BodyPart::Abs
            }
        }
    }

    /// 日本語の名前。
    pub fn name_ja(self) -> &'static str {
        match self {
            Self::PectoralisMajorUpper => "大胸筋上部",
            Self::PectoralisMajorMiddle => "大胸筋中部",
            Self::PectoralisMajorLower => "大胸筋下部",
            Self::LatissimusDorsi => "広背筋",
            Self::TeresMajor => "大円筋",
            Self::TrapeziusUpper => "僧帽筋上部",
            Self::TrapeziusMiddleLower => "僧帽筋中部・下部",
            Self::ErectorSpinae => "脊柱起立筋",
            Self::AnteriorDeltoid => "三角筋前部",
            Self::LateralDeltoid => "三角筋中部",
            Self::PosteriorDeltoid => "三角筋後部",
            Self::BicepsBrachii => "上腕二頭筋",
            Self::Forearm => "前腕",
            Self::TricepsBrachii => "上腕三頭筋",
            Self::Quadriceps => "大腿四頭筋",
            Self::Hamstrings => "ハムストリングス",
            Self::GluteusMaximus => "大臀筋",
            Self::GluteusMedius => "中臀筋",
            Self::Adductors => "内転筋",
            Self::Gastrocnemius => "腓腹筋",
            Self::Soleus => "ヒラメ筋",
            Self::RectusAbdominisUpper => "腹直筋上部",
            Self::RectusAbdominisLower => "腹直筋下部",
            Self::Obliques => "腹斜筋",
        }
    }

    /// DB と API の識別子。
    pub fn id(self) -> &'static str {
        match self {
            Self::PectoralisMajorUpper => "pectoralis_major_upper",
            Self::PectoralisMajorMiddle => "pectoralis_major_middle",
            Self::PectoralisMajorLower => "pectoralis_major_lower",
            Self::LatissimusDorsi => "latissimus_dorsi",
            Self::TeresMajor => "teres_major",
            Self::TrapeziusUpper => "trapezius_upper",
            Self::TrapeziusMiddleLower => "trapezius_middle_lower",
            Self::ErectorSpinae => "erector_spinae",
            Self::AnteriorDeltoid => "anterior_deltoid",
            Self::LateralDeltoid => "lateral_deltoid",
            Self::PosteriorDeltoid => "posterior_deltoid",
            Self::BicepsBrachii => "biceps_brachii",
            Self::Forearm => "forearm",
            Self::TricepsBrachii => "triceps_brachii",
            Self::Quadriceps => "quadriceps",
            Self::Hamstrings => "hamstrings",
            Self::GluteusMaximus => "gluteus_maximus",
            Self::GluteusMedius => "gluteus_medius",
            Self::Adductors => "adductors",
            Self::Gastrocnemius => "gastrocnemius",
            Self::Soleus => "soleus",
            Self::RectusAbdominisUpper => "rectus_abdominis_upper",
            Self::RectusAbdominisLower => "rectus_abdominis_lower",
            Self::Obliques => "obliques",
        }
    }
}

/// 筋肉の役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MuscleRole {
    Primary,
    Secondary,
}

/// 種目と筋肉の組。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuscleAssignment {
    pub muscle: MuscleUnit,
    pub role: MuscleRole,
}

/// 種目の検証の失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseError {
    EmptyName,
    EmptyKana,
    OtherHasMuscles,
    OtherScopeMissing,
    OtherScopeUnexpected,
    PrimaryMissing,
    MuscleBodyPartMismatch,
    TimeHasGrip,
    GripSelectionMismatch,
    AttachmentStyleMissing,
    UnexpectedAttachmentStyle,
    DuplicateMuscle,
}

/// 種目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exercise {
    pub id: String,
    pub name: String,
    pub name_kana: String,
    pub body_part: BodyPart,
    pub equipment: Equipment,
    pub record_mode: RecordMode,
    pub laterality: Laterality,
    pub grip_mode: GripMode,
    pub other_scope: Option<OtherScope>,
    pub instructions: String,
    pub cautions: String,
    pub tips: String,
    pub sort_order: i32,
    pub muscles: Vec<MuscleAssignment>,
    pub grip_options: Vec<GripSelection>,
}

impl Exercise {
    /// 種目の不変条件を検証する。
    pub fn validate(&self) -> Result<(), ExerciseError> {
        if self.name.trim().is_empty() {
            return Err(ExerciseError::EmptyName);
        }
        if self.name_kana.trim().is_empty() {
            return Err(ExerciseError::EmptyKana);
        }
        if self.body_part.is_other() {
            if self.other_scope.is_none() {
                return Err(ExerciseError::OtherScopeMissing);
            }
            if !self.muscles.is_empty() {
                return Err(ExerciseError::OtherHasMuscles);
            }
        } else {
            if self.other_scope.is_some() {
                return Err(ExerciseError::OtherScopeUnexpected);
            }
            if !self
                .muscles
                .iter()
                .any(|assignment| assignment.role == MuscleRole::Primary)
            {
                return Err(ExerciseError::PrimaryMissing);
            }
            if self
                .muscles
                .iter()
                .any(|assignment| assignment.muscle.body_part() != self.body_part)
            {
                return Err(ExerciseError::MuscleBodyPartMismatch);
            }
        }
        let mut seen = Vec::new();
        for assignment in &self.muscles {
            if seen.contains(&assignment.muscle) {
                return Err(ExerciseError::DuplicateMuscle);
            }
            seen.push(assignment.muscle);
        }
        if self.record_mode == RecordMode::Time
            && (self.grip_mode != GripMode::None || !self.grip_options.is_empty())
        {
            return Err(ExerciseError::TimeHasGrip);
        }
        for option in &self.grip_options {
            option.validate_for(self.grip_mode)?;
        }
        Ok(())
    }
}

/// 記録時点の種目の内容。マスターや独自種目を後から変えても過去の記録は変えない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExerciseSnapshot {
    pub exercise_id: String,
    pub name: String,
    pub body_part: BodyPart,
    pub muscles: Vec<MuscleAssignment>,
    pub record_mode: RecordMode,
    pub laterality: Laterality,
    pub grip_mode: GripMode,
    pub grip_options: Vec<GripSelection>,
}

impl From<&Exercise> for ExerciseSnapshot {
    fn from(exercise: &Exercise) -> Self {
        Self {
            exercise_id: exercise.id.clone(),
            name: exercise.name.clone(),
            body_part: exercise.body_part,
            muscles: exercise.muscles.clone(),
            record_mode: exercise.record_mode,
            laterality: exercise.laterality,
            grip_mode: exercise.grip_mode,
            grip_options: exercise.grip_options.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BodyPart, Equipment, Exercise, ExerciseError, ExerciseSnapshot, GripMode, GripSelection,
        GripStyle, GripWidth, Laterality, MuscleAssignment, MuscleRole, MuscleUnit, OtherScope,
        RecordMode,
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
    fn the_twenty_four_muscle_units_are_unique_and_classified() {
        assert_eq!(MuscleUnit::ALL.len(), 24);
        assert_eq!(super::GripStyle::ALL.len(), 3);
        assert_eq!(super::GripWidth::ALL.len(), 3);
        assert_eq!(super::Attachment::ALL.len(), 10);
        let mut ids = MuscleUnit::ALL
            .iter()
            .map(|unit| unit.id())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 24);
        for unit in MuscleUnit::ALL {
            assert!(!unit.name_ja().is_empty());
            assert_ne!(unit.body_part(), BodyPart::Other);
        }
    }

    #[test]
    fn a_valid_exercise_passes() {
        exercise().validate().unwrap();
    }

    #[test]
    fn empty_names_and_kana_are_rejected() {
        let mut value = exercise();
        value.name = " ".to_owned();
        assert_eq!(value.validate(), Err(ExerciseError::EmptyName));
        value = exercise();
        value.name_kana = String::new();
        assert_eq!(value.validate(), Err(ExerciseError::EmptyKana));
    }

    #[test]
    fn other_exercises_need_a_scope_and_have_no_muscles() {
        let mut value = exercise();
        value.body_part = BodyPart::Other;
        value.muscles.clear();
        assert_eq!(value.validate(), Err(ExerciseError::OtherScopeMissing));
        value.other_scope = Some(OtherScope::FullBody);
        value.muscles.push(MuscleAssignment {
            muscle: MuscleUnit::Quadriceps,
            role: MuscleRole::Primary,
        });
        assert_eq!(value.validate(), Err(ExerciseError::OtherHasMuscles));
    }

    #[test]
    fn muscle_body_parts_must_match() {
        let mut value = exercise();
        value.muscles[0].muscle = MuscleUnit::Quadriceps;
        assert_eq!(value.validate(), Err(ExerciseError::MuscleBodyPartMismatch));
    }

    #[test]
    fn time_exercises_cannot_have_grip_options() {
        let mut value = exercise();
        value.record_mode = RecordMode::Time;
        value.grip_mode = GripMode::Grip;
        value.grip_options = vec![GripSelection::Grip {
            style: GripStyle::Pronated,
            width: GripWidth::Medium,
        }];
        assert_eq!(value.validate(), Err(ExerciseError::TimeHasGrip));
    }

    #[test]
    fn attachment_style_is_checked() {
        let mut value = exercise();
        value.body_part = BodyPart::Back;
        value.muscles[0].muscle = MuscleUnit::LatissimusDorsi;
        value.grip_mode = GripMode::Attachment;
        value.grip_options = vec![GripSelection::Attachment {
            attachment: super::Attachment::WideBar,
            style: None,
        }];
        assert_eq!(value.validate(), Err(ExerciseError::AttachmentStyleMissing));
        value.grip_options[0] = GripSelection::Attachment {
            attachment: super::Attachment::MagWideOver,
            style: Some(GripStyle::Pronated),
        };
        assert_eq!(
            value.validate(),
            Err(ExerciseError::UnexpectedAttachmentStyle)
        );
    }

    #[test]
    fn the_grip_comparison_key_separates_every_selection() {
        assert_eq!(GripSelection::None.comparison_key(), (None, None));
        assert_eq!(
            GripSelection::Grip {
                style: GripStyle::Pronated,
                width: GripWidth::Medium,
            }
            .comparison_key(),
            (Some("pronated_medium".to_owned()), None)
        );
        assert_eq!(
            GripSelection::Attachment {
                attachment: super::Attachment::WideBar,
                style: Some(GripStyle::Supinated),
            }
            .comparison_key(),
            (Some("supinated".to_owned()), Some("wide_bar".to_owned()))
        );
        assert_eq!(
            GripSelection::Attachment {
                attachment: super::Attachment::ParallelGrip,
                style: None,
            }
            .comparison_key(),
            (None, Some("parallel_grip".to_owned()))
        );
    }

    #[test]
    fn the_snapshot_round_trips_through_serde() {
        let snapshot = ExerciseSnapshot::from(&exercise());
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<ExerciseSnapshot>(&json).unwrap(),
            snapshot
        );
    }
}
