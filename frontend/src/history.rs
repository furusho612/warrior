//! 種目別の履歴とグラフの API と、画面の選択肢の組み立て。
//!
//! 画面は要求の組み立てと応答の型だけを持ち、URL と選択肢の値はここに集める。
//! 期間と指標の意味は `warrior_core::history` が持つ。

use warrior_core::exercise::{GripMode, GripSelection};
use warrior_core::history::{ExerciseGraph, ExerciseHistory, GraphMetric, GraphPeriod, GripFilter};
use warrior_core::set_calc::RecordMode;
use warrior_core::training_session::BestKind;

use crate::grip_text::{attachment_label, grip_label};
use crate::http::{self, HttpError};
use crate::text::{text, Key};

/// 種目別の履歴を取る。
pub async fn history(exercise_id: &str, filter: &GripFilter) -> Result<ExerciseHistory, HttpError> {
    http::request_json("GET", &history_url(exercise_id, filter)).await
}

/// 種目別のグラフを取る。
pub async fn graph(
    exercise_id: &str,
    filter: &GripFilter,
    period: GraphPeriod,
    metric: GraphMetric,
) -> Result<ExerciseGraph, HttpError> {
    http::request_json("GET", &graph_url(exercise_id, filter, period, metric)).await
}

/// 履歴の URL。絞り込みの指定が無ければクエリを付けない。
pub fn history_url(exercise_id: &str, filter: &GripFilter) -> String {
    format!(
        "/api/exercises/{exercise_id}/history{}",
        filter_query(filter)
    )
}

/// グラフの URL。期間と指標を必ず付ける。
pub fn graph_url(
    exercise_id: &str,
    filter: &GripFilter,
    period: GraphPeriod,
    metric: GraphMetric,
) -> String {
    format!(
        "/api/exercises/{exercise_id}/graph?period={}&metric={}{}",
        period.as_str(),
        metric.as_str(),
        filter_query(filter).replacen('?', "&", 1)
    )
}

fn filter_query(filter: &GripFilter) -> String {
    let mut parts = Vec::new();
    if let Some(grip) = &filter.grip {
        parts.push(format!("grip={grip}"));
    }
    if let Some(attachment) = &filter.attachment {
        parts.push(format!("attachment={attachment}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// 画面の絞り込みの選択肢。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterChoice {
    /// 選択肢の値。選択していないときは None。
    pub value: String,
    /// 画面に表示する文言。
    pub label: String,
}

/// 種目が持つグリップの選択肢から、画面の絞り込みの選択肢を作る。
///
/// グリップを選ばない種目は空である。
pub fn filter_choices(history: &ExerciseHistory) -> Vec<FilterChoice> {
    history
        .grip_options
        .iter()
        .copied()
        .map(|selection| FilterChoice {
            value: choice_value(selection),
            label: choice_label(selection),
        })
        .collect()
}

fn choice_value(selection: GripSelection) -> String {
    let (grip, attachment) = selection.comparison_key();
    match selection {
        GripSelection::None => String::new(),
        GripSelection::Grip { .. } => grip.unwrap_or_default(),
        GripSelection::Attachment { .. } => {
            format!(
                "{}|{}",
                grip.unwrap_or_default(),
                attachment.unwrap_or_default()
            )
        }
    }
}

fn choice_label(selection: GripSelection) -> String {
    match selection {
        GripSelection::None => String::new(),
        GripSelection::Grip { .. } => selection
            .comparison_key()
            .0
            .map_or_else(String::new, |key| grip_label(&key)),
        GripSelection::Attachment { .. } => {
            let (style, attachment) = selection.comparison_key();
            let attachment = attachment.map_or_else(String::new, |key| attachment_label(&key));
            match style {
                Some(style) => format!("{} / {}", grip_label(&style), attachment),
                None => attachment,
            }
        }
    }
}

/// 選択肢の値から、API の絞り込みを作る。
pub fn filter_from_choice(value: Option<&str>) -> GripFilter {
    let Some(value) = value.filter(|text| !text.is_empty()) else {
        return GripFilter::default();
    };
    match value.split_once('|') {
        Some((grip, attachment)) => {
            GripFilter::new(Some(grip.to_owned()), Some(attachment.to_owned()))
        }
        None => GripFilter::new(Some(value.to_owned()), None),
    }
}

/// 期間の選択肢の値と文言。
pub fn period_options() -> Vec<(&'static str, &'static str)> {
    GraphPeriod::ALL
        .into_iter()
        .map(|period| (period.as_str(), period_label(period)))
        .collect()
}

fn period_label(period: GraphPeriod) -> &'static str {
    match period {
        GraphPeriod::OneMonth => text(Key::ExerciseHistoryPeriodOneMonth),
        GraphPeriod::ThreeMonths => text(Key::ExerciseHistoryPeriodThreeMonths),
        GraphPeriod::SixMonths => text(Key::ExerciseHistoryPeriodSixMonths),
        GraphPeriod::OneYear => text(Key::ExerciseHistoryPeriodOneYear),
        GraphPeriod::All => text(Key::ExerciseHistoryPeriodAll),
    }
}

/// 記録方式ごとの指標の選択肢の値と文言。
pub fn metric_options(mode: RecordMode) -> Vec<(&'static str, &'static str)> {
    GraphMetric::allowed_for(mode)
        .iter()
        .map(|metric| (metric.as_str(), metric_label(*metric)))
        .collect()
}

/// 画面が最初に選ぶ指標。記録方式ごとの先頭の指標を使う。
pub fn default_metric(mode: RecordMode) -> Option<GraphMetric> {
    GraphMetric::allowed_for(mode).first().copied()
}

fn metric_label(metric: GraphMetric) -> &'static str {
    match metric {
        GraphMetric::MaxWeight => text(Key::SummaryBestMaxWeight),
        GraphMetric::EstimatedOneRepMax => text(Key::SummaryBestEstimatedOneRepMax),
        GraphMetric::TotalWeight => text(Key::SummaryBestTotalWeight),
        GraphMetric::LongestDuration => text(Key::SummaryBestLongestDuration),
        GraphMetric::TotalDuration => text(Key::ExerciseHistoryMetricTotalDuration),
    }
}

/// 自己ベストの種類の文言。
pub fn best_label(kind: BestKind) -> &'static str {
    match kind {
        BestKind::MaxWeight => text(Key::SummaryBestMaxWeight),
        BestKind::EstimatedOneRepMax => text(Key::SummaryBestEstimatedOneRepMax),
        BestKind::TotalWeight => text(Key::SummaryBestTotalWeight),
        BestKind::LongestDuration => text(Key::SummaryBestLongestDuration),
    }
}

/// グリップを選ぶ種目か。
pub fn has_grip_filter(mode: GripMode) -> bool {
    mode != GripMode::None
}

#[cfg(test)]
mod tests {
    //! 履歴とグラフの URL と選択肢の単体テスト。
    //!
    //! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-SET-011`。
    //! 要求の受入基準は E2E が担い、ここは API のクエリの組み立てと、画面の選択肢の値が
    //! `GripSelection::comparison_key` に対応することを確かめる。

    use warrior_core::exercise::{Attachment, GripMode, GripSelection, GripStyle, GripWidth};
    use warrior_core::history::{ExerciseHistory, GraphMetric, GraphPeriod, GripFilter};
    use warrior_core::set_calc::RecordMode;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{
        default_metric, filter_choices, filter_from_choice, graph_url, has_grip_filter,
        history_url, metric_options, period_options,
    };

    #[wasm_bindgen_test]
    fn the_history_url_carries_the_grip_and_the_attachment() {
        assert_eq!(
            history_url("exercise", &GripFilter::default()),
            "/api/exercises/exercise/history"
        );
        assert_eq!(
            history_url(
                "exercise",
                &GripFilter::new(Some("pronated_medium".to_owned()), None),
            ),
            "/api/exercises/exercise/history?grip=pronated_medium"
        );
        assert_eq!(
            history_url(
                "exercise",
                &GripFilter::new(Some("supinated".to_owned()), Some("wide_bar".to_owned())),
            ),
            "/api/exercises/exercise/history?grip=supinated&attachment=wide_bar"
        );
    }

    #[wasm_bindgen_test]
    fn the_graph_url_carries_the_period_the_metric_and_the_filter() {
        assert_eq!(
            graph_url(
                "exercise",
                &GripFilter::default(),
                GraphPeriod::OneMonth,
                GraphMetric::MaxWeight,
            ),
            "/api/exercises/exercise/graph?period=1m&metric=max_weight"
        );
        assert_eq!(
            graph_url(
                "exercise",
                &GripFilter::new(None, Some("straight_bar".to_owned())),
                GraphPeriod::All,
                GraphMetric::TotalDuration,
            ),
            "/api/exercises/exercise/graph?period=all&metric=total_duration&attachment=straight_bar"
        );
    }

    #[wasm_bindgen_test]
    fn the_filter_choices_use_the_comparison_keys() {
        let history = history_with_options(vec![
            GripSelection::Grip {
                style: GripStyle::Pronated,
                width: GripWidth::Medium,
            },
            GripSelection::Grip {
                style: GripStyle::Supinated,
                width: GripWidth::Narrow,
            },
        ]);
        let choices = filter_choices(&history);
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].value, "pronated_medium");
        assert_eq!(choices[0].label, "順手 ミディアム");
        assert_eq!(choices[1].value, "supinated_narrow");
        assert_eq!(choices[1].label, "逆手 ナロー");

        let attachments = history_with_options(vec![
            GripSelection::Attachment {
                attachment: Attachment::WideBar,
                style: Some(GripStyle::Pronated),
            },
            GripSelection::Attachment {
                attachment: Attachment::ParallelGrip,
                style: None,
            },
        ]);
        let choices = filter_choices(&attachments);
        assert_eq!(choices[0].value, "pronated|wide_bar");
        assert_eq!(choices[0].label, "順手 / ワイドバー");
        assert_eq!(choices[1].value, "|parallel_grip");
        assert_eq!(choices[1].label, "パラレルグリップ");
    }

    #[wasm_bindgen_test]
    fn the_filter_choice_round_trips_to_the_api_filter() {
        let filter = filter_from_choice(Some("pronated_medium"));
        assert_eq!(
            filter,
            GripFilter::new(Some("pronated_medium".to_owned()), None)
        );
        let filter = filter_from_choice(Some("supinated|wide_bar"));
        assert_eq!(
            filter,
            GripFilter::new(Some("supinated".to_owned()), Some("wide_bar".to_owned()))
        );
        let filter = filter_from_choice(Some("|parallel_grip"));
        assert_eq!(
            filter,
            GripFilter::new(None, Some("parallel_grip".to_owned()))
        );
        assert_eq!(filter_from_choice(None), GripFilter::default());
        assert_eq!(filter_from_choice(Some("")), GripFilter::default());
    }

    #[wasm_bindgen_test]
    fn the_period_options_are_the_five_periods() {
        let options = period_options();
        assert_eq!(options.len(), 5);
        assert_eq!(options[0], ("1m", "1 か月"));
        assert_eq!(options[4], ("all", "全期間"));
    }

    #[wasm_bindgen_test]
    fn the_metric_options_follow_the_record_mode() {
        let weights = metric_options(RecordMode::WeightAndReps);
        assert_eq!(weights.len(), 3);
        assert_eq!(weights[0], ("max_weight", "最大重量"));
        assert_eq!(weights[2], ("total_weight", "種目別の総重量"));
        let times = metric_options(RecordMode::Time);
        assert_eq!(times.len(), 2);
        assert_eq!(times[0], ("longest_duration", "最長時間"));
        assert_eq!(times[1], ("total_duration", "合計時間"));
        assert_eq!(
            default_metric(RecordMode::WeightAndReps),
            Some(GraphMetric::MaxWeight)
        );
        assert_eq!(
            default_metric(RecordMode::Time),
            Some(GraphMetric::LongestDuration)
        );
    }

    #[wasm_bindgen_test]
    fn only_a_grip_exercise_has_the_filter() {
        assert!(!has_grip_filter(GripMode::None));
        assert!(has_grip_filter(GripMode::Grip));
        assert!(has_grip_filter(GripMode::Attachment));
    }

    fn history_with_options(options: Vec<GripSelection>) -> ExerciseHistory {
        ExerciseHistory {
            exercise_id: "exercise".to_owned(),
            name: "種目".to_owned(),
            record_mode: RecordMode::WeightAndReps,
            laterality: warrior_core::exercise::Laterality::Bilateral,
            grip_mode: GripMode::Grip,
            grip_options: options,
            bests: Vec::new(),
            goals: Vec::new(),
            trainings: Vec::new(),
        }
    }
}
