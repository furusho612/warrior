//! 記録画面の種目レールが読む状態。
//!
//! 記録画面が種目の一覧と選択を公開し、共通の枠の 60px のレールが一覧を描く。
//! レールの選択は記録画面の詳細の表示に使う。画面を離れるときに記録画面が空にする。

/// レールが描く 1 つの種目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRailExercise {
    /// トレーニングの種目の行の ID。
    pub id: String,
    /// 種目マスターまたは独自種目の ID。
    pub exercise_id: String,
    /// 記録時点の種目名。
    pub name: String,
}

/// 種目レールの状態。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecordRailState {
    /// レールを出しているトレーニングの ID。別の画面では None。
    pub training_id: Option<String>,
    /// レールに並べる種目。
    pub exercises: Vec<RecordRailExercise>,
    /// 選択中の種目の行の ID。
    pub selected: Option<String>,
}
