//! 種目ごとの目標重量、現在の記録との差、途中の目標の検証 (PRD の「種目ごとの目標重量」)。
//!
//! 対応する要求: `REQ-GOAL-001`、`REQ-GOAL-002`、`REQ-GOAL-004`、`REQ-GOAL-005`。
//! 目標は利用者、種目、グリップ、アタッチメントの組ごとに持つ。現在の記録は、表示の日を
//! 含む直近 7 日間の終了済みトレーニングの最大重量の平均とし、記録が無ければ同じ組の
//! 自己ベストの最大重量を使う。D1 の読み書きは Worker の `goal` が担う。

use serde::{Deserialize, Serialize};

use crate::exercise::{GripMode, GripSelection};
use crate::jst::JstDate;
use crate::set_calc::SetRecord;

/// 目標重量の上限 (kg)。
pub const MAX_TARGET_WEIGHT_KG: f64 = 1000.0;

/// 現在の記録に含める日数。表示の日を含む。
pub const CURRENT_RECORD_DAYS: i64 = 7;

/// 種目とグリップとアタッチメントの組ごとの目標。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExerciseGoal {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。選ばない種目は None。
    #[serde(default)]
    pub grip: Option<String>,
    /// アタッチメント。選ばない種目は None。
    #[serde(default)]
    pub attachment: Option<String>,
    /// 目標重量 (kg)。
    pub target_weight_kg: f64,
    /// 達成したい時期。任意である。
    #[serde(default)]
    pub target_date: Option<JstDate>,
    /// 目標を判断する時点の現在の記録 (kg)。
    ///
    /// AI のコンテキストは、この値を途中の目標の検証に使う。記録が無ければ None。
    #[serde(default)]
    pub current_weight_kg: Option<f64>,
}

/// 目標の検証の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalError {
    /// 目標重量が 0 kg 以下、1000 kg を超える、または有限でない。
    InvalidTargetWeight,
    /// 達成したい時期が設定の日と同じ、またはそれより前である。
    TargetDateNotAfterToday,
}

impl ExerciseGoal {
    /// 目標を検証する。時期は設定の日より後でなければならない。
    pub fn validate(&self, today: JstDate) -> Result<(), GoalError> {
        if !self.target_weight_kg.is_finite()
            || self.target_weight_kg <= 0.0
            || self.target_weight_kg > MAX_TARGET_WEIGHT_KG
        {
            return Err(GoalError::InvalidTargetWeight);
        }
        if self.target_date.is_some_and(|date| date <= today) {
            return Err(GoalError::TargetDateNotAfterToday);
        }
        Ok(())
    }

    /// 比較の単位をキーにする。現在の記録はキーに含めない。
    pub fn key(&self) -> GoalKey {
        GoalKey {
            exercise_id: self.exercise_id.clone(),
            grip: self.grip.clone(),
            attachment: self.attachment.clone(),
        }
    }
}

/// 目標と現在の記録の比較の単位。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GoalKey {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。
    pub grip: Option<String>,
    /// アタッチメント。
    pub attachment: Option<String>,
}

/// 目標と、目標を判断する時点の現在の記録。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalProgress {
    /// 目標。
    pub goal: ExerciseGoal,
    /// 現在の記録 (kg)。記録が無ければ None。
    pub current_weight_kg: Option<f64>,
    /// 目標重量から現在の記録を引いた値。現在の記録が無ければ None。
    pub difference_kg: Option<f64>,
}

impl GoalProgress {
    /// 目標と現在の記録から差を求める。
    pub fn new(mut goal: ExerciseGoal, current_weight_kg: Option<f64>) -> Self {
        let current = current_weight_kg.filter(|value| value.is_finite() && *value > 0.0);
        let difference_kg = current.map(|current| goal.target_weight_kg - current);
        goal.current_weight_kg = current;
        Self {
            goal,
            current_weight_kg: current,
            difference_kg,
        }
    }
}

/// 現在の記録の計算に渡す、終了済みトレーニングの 1 セット。
#[derive(Debug, Clone, Copy)]
pub struct GoalSet<'a> {
    /// トレーニングの ID。同じトレーニングのセットは 1 つの最大重量にまとめる。
    pub training_id: &'a str,
    /// トレーニングの日付 (日本時間)。
    pub date: JstDate,
    /// 完了したセットの記録。
    pub record: &'a SetRecord,
}

/// 現在の記録を求める。
///
/// 表示の日を含む直近 7 日間の終了済みトレーニングについて、トレーニングごとの最大重量を
/// 平均する。ウォームアップと時間で記録する種目は最大重量を持たないため含めない。
/// 直近 7 日間に記録が無ければ、同じ組の自己ベストの最大重量を使う。どちらも無ければ None。
pub fn current_weight_kg(
    today: JstDate,
    sets: &[GoalSet<'_>],
    best_weight_kg: Option<f64>,
) -> Option<f64> {
    let Some(start) = today.add_days(-(CURRENT_RECORD_DAYS - 1)) else {
        return usable_best(best_weight_kg);
    };
    let mut trainings = Vec::<(&str, f64)>::new();
    for set in sets {
        if set.date < start || set.date > today {
            continue;
        }
        let Some(weight) = set.record.max_weight_kg().filter(|value| *value > 0.0) else {
            continue;
        };
        match trainings
            .iter_mut()
            .find(|(training_id, _)| *training_id == set.training_id)
        {
            Some((_, current)) => *current = current.max(weight),
            None => trainings.push((set.training_id, weight)),
        }
    }
    if !trainings.is_empty() {
        let total = trainings.iter().map(|(_, weight)| *weight).sum::<f64>();
        return Some(total / trainings.len() as f64);
    }
    usable_best(best_weight_kg)
}

fn usable_best(value: Option<f64>) -> Option<f64> {
    value.filter(|weight| weight.is_finite() && *weight > 0.0)
}

/// 途中の目標として使える値だけを残す。
///
/// 現在の記録より大きく、最終の目標より小さい有限の値だけを残す。現在の記録が無い場合は、
/// 0 kg より大きく最終の目標より小さい値を残す。順序は入力のまま保つ。
pub fn valid_intermediate_targets(
    current_weight_kg: Option<f64>,
    target_weight_kg: f64,
    values: &[f64],
) -> Vec<f64> {
    let lower = current_weight_kg
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
    values
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > lower && *value < target_weight_kg)
        .collect()
}

/// 種目のグリップの選択肢に、目標の組が含まれるか。
///
/// グリップを選ばない種目は grip と attachment の両方が None のときだけ受け付ける。
/// グリップまたはアタッチメントを選ぶ種目は、種目が持つ選択肢の比較のキーと一致する
/// 組だけを受け付ける。組を省略した目標は受け付けない。
pub fn matches_selection(
    mode: GripMode,
    options: &[GripSelection],
    grip: Option<&str>,
    attachment: Option<&str>,
) -> bool {
    match mode {
        GripMode::None => grip.is_none() && attachment.is_none(),
        GripMode::Grip | GripMode::Attachment => options.iter().any(|option| {
            let (option_grip, option_attachment) = option.comparison_key();
            option_grip.as_deref() == grip && option_attachment.as_deref() == attachment
        }),
    }
}

#[cfg(test)]
mod tests {
    //! 目標の検証、現在の記録、差、途中の目標の例示テスト。
    //!
    //! 対応する要求: `REQ-GOAL-001`、`REQ-GOAL-002`、`REQ-GOAL-004`、`REQ-GOAL-005`。
    //! 任意入力の性質は `backend/pbt/tests/goal.rs` が担う。ここは 7 日間の境界、
    //! 同じ日の複数のトレーニング、自己ベストへの切り替え、検証の境界を固定する。

    use super::{
        current_weight_kg, matches_selection, valid_intermediate_targets, ExerciseGoal, GoalError,
        GoalProgress, GoalSet, MAX_TARGET_WEIGHT_KG,
    };
    use crate::exercise::{GripMode, GripSelection, GripStyle, GripWidth};
    use crate::jst::JstDate;
    use crate::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};

    #[test]
    fn req_goal_001_accepts_a_weight_up_to_the_upper_bound_and_a_future_date() {
        let today = date(2026, 11, 12);
        let mut goal = goal(1000.0, Some(date(2026, 11, 13)));
        assert_eq!(goal.validate(today), Ok(()));
        goal.target_weight_kg = 0.5;
        goal.target_date = None;
        assert_eq!(goal.validate(today), Ok(()));
    }

    #[test]
    fn req_goal_001_rejects_a_weight_outside_the_range() {
        let today = date(2026, 11, 12);
        for weight in [0.0, -1.0, f64::NAN, f64::INFINITY, 1000.1] {
            let mut goal = goal(weight, None);
            assert_eq!(
                goal.validate(today),
                Err(GoalError::InvalidTargetWeight),
                "the weight {weight} must be rejected"
            );
            goal.target_weight_kg = MAX_TARGET_WEIGHT_KG;
            assert_eq!(goal.validate(today), Ok(()));
        }
    }

    #[test]
    fn req_goal_001_rejects_a_target_date_that_is_not_after_the_setting_day() {
        let today = date(2026, 11, 12);
        for target_date in [Some(today), Some(date(2026, 11, 11))] {
            assert_eq!(
                goal(100.0, target_date).validate(today),
                Err(GoalError::TargetDateNotAfterToday),
                "the date {target_date:?} must be rejected"
            );
        }
    }

    #[test]
    fn req_goal_002_averages_the_training_maxima_inside_the_seven_days() {
        let today = date(2026, 11, 12);
        let records = [
            ("first", date(2026, 11, 6), weight_record(100.0)),
            ("first", date(2026, 11, 6), weight_record(80.0)),
            ("second", date(2026, 11, 12), weight_record(90.0)),
            ("outside", date(2026, 11, 5), weight_record(200.0)),
        ];
        let sets = goal_sets(&records);
        assert_eq!(
            current_weight_kg(today, &sets, Some(500.0)),
            Some(95.0),
            "the average uses the maximum of each training on the first and the last day"
        );
    }

    #[test]
    fn req_goal_002_two_trainings_on_the_same_day_are_averaged_separately() {
        let today = date(2026, 11, 12);
        let records = [
            ("morning", today, weight_record(80.0)),
            ("evening", today, weight_record(100.0)),
        ];
        let sets = goal_sets(&records);
        assert_eq!(current_weight_kg(today, &sets, None), Some(90.0));
    }

    #[test]
    fn req_goal_002_drop_sets_use_the_first_segment_and_warmups_are_excluded() {
        let today = date(2026, 11, 12);
        let drop = SetRecord::new(
            key(),
            SetKind::Drop,
            vec![segment(100.0, 5), segment(120.0, 3)],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid drop set");
        let warmup = SetRecord::new(
            key(),
            SetKind::Warmup,
            vec![segment(200.0, 5)],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid warmup set");
        let sets = vec![
            GoalSet {
                training_id: "drop",
                date: today,
                record: &drop,
            },
            GoalSet {
                training_id: "warmup",
                date: today,
                record: &warmup,
            },
        ];
        assert_eq!(current_weight_kg(today, &sets, None), Some(100.0));
    }

    #[test]
    fn req_goal_002_falls_back_to_the_personal_best_without_records_in_the_window() {
        let today = date(2026, 11, 12);
        let records = [("old", date(2026, 10, 1), weight_record(80.0))];
        let sets = goal_sets(&records);
        assert_eq!(current_weight_kg(today, &sets, Some(120.0)), Some(120.0));
        assert_eq!(current_weight_kg(today, &sets, None), None);
        assert_eq!(
            current_weight_kg(today, &[], Some(0.0)),
            None,
            "a non-positive best is not a current record"
        );
    }

    #[test]
    fn req_goal_002_the_difference_is_the_target_minus_the_current_record() {
        let progress = GoalProgress::new(goal(100.0, None), Some(80.0));
        assert_eq!(progress.current_weight_kg, Some(80.0));
        assert_eq!(progress.difference_kg, Some(20.0));
        let ahead = GoalProgress::new(goal(100.0, None), Some(120.0));
        assert_eq!(ahead.difference_kg, Some(-20.0));
        let unknown = GoalProgress::new(goal(100.0, None), None);
        assert_eq!(unknown.current_weight_kg, None);
        assert_eq!(unknown.difference_kg, None);
    }

    #[test]
    fn req_goal_004_intermediate_targets_stay_between_the_current_and_the_final() {
        assert_eq!(
            valid_intermediate_targets(Some(20.0), 60.0, &[30.0, 20.0, 60.0, 70.0, 40.0]),
            vec![30.0, 40.0]
        );
        assert_eq!(
            valid_intermediate_targets(None, 60.0, &[10.0, 60.0]),
            vec![10.0]
        );
        assert!(valid_intermediate_targets(Some(20.0), 60.0, &[f64::NAN]).is_empty());
    }

    #[test]
    fn req_goal_001_a_gripless_exercise_accepts_only_the_empty_selection() {
        assert!(matches_selection(GripMode::None, &[], None, None));
        assert!(!matches_selection(
            GripMode::None,
            &[],
            Some("pronated_medium"),
            None
        ));
    }

    #[test]
    fn req_goal_001_a_grip_exercise_requires_one_of_its_options() {
        let options = [GripSelection::Grip {
            style: GripStyle::Pronated,
            width: GripWidth::Medium,
        }];
        assert!(matches_selection(
            GripMode::Grip,
            &options,
            Some("pronated_medium"),
            None
        ));
        assert!(!matches_selection(GripMode::Grip, &options, None, None));
        assert!(!matches_selection(
            GripMode::Grip,
            &options,
            Some("supinated_narrow"),
            None
        ));
    }

    #[test]
    fn req_goal_001_an_attachment_exercise_uses_the_whole_pair() {
        let options = [GripSelection::Attachment {
            attachment: crate::exercise::Attachment::WideBar,
            style: Some(GripStyle::Pronated),
        }];
        assert!(matches_selection(
            GripMode::Attachment,
            &options,
            Some("pronated"),
            Some("wide_bar")
        ));
        assert!(!matches_selection(
            GripMode::Attachment,
            &options,
            None,
            Some("wide_bar")
        ));
        assert!(!matches_selection(
            GripMode::Attachment,
            &options,
            Some("supinated"),
            Some("wide_bar")
        ));
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }

    fn goal(target_weight_kg: f64, target_date: Option<JstDate>) -> ExerciseGoal {
        ExerciseGoal {
            exercise_id: "bench".to_owned(),
            grip: None,
            attachment: None,
            target_weight_kg,
            target_date,
            current_weight_kg: None,
        }
    }

    fn key() -> BestKey {
        BestKey {
            exercise_id: "bench".to_owned(),
            grip: None,
            attachment: None,
        }
    }

    fn segment(weight_kg: f64, reps: u32) -> SetSegment {
        SetSegment {
            weight_kg,
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        }
    }

    fn weight_record(weight_kg: f64) -> SetRecord {
        SetRecord::new(
            key(),
            SetKind::Normal,
            vec![segment(weight_kg, 5)],
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid set")
    }

    fn goal_sets<'a>(records: &'a [(&'a str, JstDate, SetRecord)]) -> Vec<GoalSet<'a>> {
        records
            .iter()
            .map(|(training_id, date, record)| GoalSet {
                training_id,
                date: *date,
                record,
            })
            .collect()
    }
}
