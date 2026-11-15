//! 今回の入力と前回の記録の差の集計。

use warrior_core::exercise::RecordMode;
use warrior_core::previous::{
    exercise_difference, total_weight_difference, ExerciseDifference, PreviousTraining,
    TotalWeightDifference,
};

use super::super::sets::is_lateral;
use super::view::ExerciseView;

/// 今回の入力から作った種目の合計。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CurrentTotals {
    total_weight_kg: f64,
    total_duration_seconds: u32,
    longest_duration_seconds: u32,
}

fn current_totals(view: &ExerciseView) -> CurrentTotals {
    let mode = view.snapshot.record_mode;
    let lateral = is_lateral(view.snapshot.laterality, mode);
    let mut totals = CurrentTotals::default();
    for set in &view.sets {
        let Some(record) = set.to_record(&view.snapshot.exercise_id, mode, lateral) else {
            continue;
        };
        totals.total_weight_kg += record.total_weight_kg();
        totals.total_duration_seconds += record.total_duration_seconds();
        totals.longest_duration_seconds = totals
            .longest_duration_seconds
            .max(record.longest_duration_seconds().unwrap_or(0));
    }
    totals
}

/// 種目ごとの今回と前回の差。
pub(in crate::screens::record) fn exercise_diff(view: &ExerciseView) -> Option<ExerciseDifference> {
    let totals = current_totals(view);
    exercise_difference(
        view.snapshot.record_mode,
        totals.total_weight_kg,
        totals.total_duration_seconds,
        totals.longest_duration_seconds,
        view.previous.as_ref(),
    )
}

/// トレーニング全体の今回と前回の差。
pub(in crate::screens::record) fn training_difference(
    views: &[ExerciseView],
    previous: Option<&PreviousTraining>,
) -> Option<TotalWeightDifference> {
    let current = views
        .iter()
        .filter(|view| view.snapshot.record_mode != RecordMode::Time)
        .map(|view| current_totals(view).total_weight_kg)
        .sum::<f64>();
    total_weight_difference(current, previous.map(|training| training.total_weight_kg))
}
