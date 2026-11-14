//! 画面が読む種目とスーパーセットの組。

use warrior_core::exercise::ExerciseSnapshot;
use warrior_core::previous::PreviousExercise;
use warrior_core::training_record::TrainingExerciseRecord;

use super::super::sets::SetView;

/// 画面が読む 1 つの種目の組。
#[derive(Debug, Clone, PartialEq)]
pub(in crate::screens::record) struct SupersetGroupView {
    /// 組の ID。
    pub(in crate::screens::record) id: String,
    /// 組の種目の行の ID。並び順である。
    pub(in crate::screens::record) member_ids: Vec<String>,
}

/// 画面が読む 1 つの種目。
#[derive(Clone, PartialEq)]
pub(in crate::screens::record) struct ExerciseView {
    /// 種目の行。
    pub(in crate::screens::record) record: TrainingExerciseRecord,
    /// 記録した時点の種目。
    pub(in crate::screens::record) snapshot: ExerciseSnapshot,
    /// 前回の記録。
    pub(in crate::screens::record) previous: Option<PreviousExercise>,
    /// 前回の記録を読めなかったか。
    pub(in crate::screens::record) previous_error: bool,
    /// セットの行。
    pub(in crate::screens::record) sets: Vec<SetView>,
    /// 記録した時点の体重 (kg)。身体情報が無いときは None。
    pub(in crate::screens::record) body_weight_kg: Option<f64>,
    /// この種目を含むスーパーセットの組。
    pub(in crate::screens::record) superset: Option<SupersetGroupView>,
}
