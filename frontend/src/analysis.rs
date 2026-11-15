//! 部位別の分析の API と、画面の選択肢と値の文言。
//!
//! 画面は要求の組み立てと応答の型だけを持ち、URL と選択肢の値はここに集める。
//! 期間と指標の意味は `warrior_core::body_part_analysis` が持つ。

use warrior_core::body_part_analysis::{AnalysisMetric, AnalysisPeriod, BodyPartAnalysis};

use crate::http::{self, HttpError};
use crate::text::{text, Key};

/// 部位別の分析を取る。
pub async fn body_parts(
    period: AnalysisPeriod,
    metric: AnalysisMetric,
) -> Result<BodyPartAnalysis, HttpError> {
    http::request_json("GET", &body_parts_url(period, metric)).await
}

/// 部位別の分析の URL。期間と指標を必ず付ける。
pub fn body_parts_url(period: AnalysisPeriod, metric: AnalysisMetric) -> String {
    format!(
        "/api/analysis/body-parts?period={}&metric={}",
        period.as_str(),
        metric.as_str()
    )
}

/// 期間の選択肢の値と文言。
pub fn period_options() -> Vec<(&'static str, &'static str)> {
    AnalysisPeriod::ALL
        .into_iter()
        .map(|period| (period.as_str(), period_label(period)))
        .collect()
}

/// 指標の選択肢の値と文言。
pub fn metric_options() -> Vec<(&'static str, &'static str)> {
    AnalysisMetric::ALL
        .into_iter()
        .map(|metric| (metric.as_str(), metric_label(metric)))
        .collect()
}

/// 期間の文言。
pub fn period_label(period: AnalysisPeriod) -> &'static str {
    match period {
        AnalysisPeriod::Week => text(Key::AnalysisPeriodWeek),
        AnalysisPeriod::Month => text(Key::AnalysisPeriodMonth),
    }
}

/// 指標の文言。
pub fn metric_label(metric: AnalysisMetric) -> &'static str {
    match metric {
        AnalysisMetric::Sets => text(Key::AnalysisMetricSets),
        AnalysisMetric::Weight => text(Key::AnalysisMetricWeight),
    }
}

/// 増減の見出しの文言。
pub fn change_label(period: AnalysisPeriod) -> &'static str {
    match period {
        AnalysisPeriod::Week => text(Key::AnalysisChangeWeek),
        AnalysisPeriod::Month => text(Key::AnalysisChangeMonth),
    }
}

/// セット数の文言。
pub fn sets_text(sets: u32) -> String {
    format!("{sets} {}", text(Key::AnalysisSetsUnit))
}

/// セット数の増減の文言。増えたときは符号を付ける。
pub fn sets_change_text(change: i64) -> String {
    format!("{} {}", signed(change), text(Key::AnalysisSetsUnit))
}

/// 総重量の文言。
pub fn weight_text(weight_kg: f64) -> String {
    format!("{weight_kg:.1} kg")
}

/// 総重量の増減の文言。増えたときは符号を付ける。
pub fn weight_change_text(change_kg: f64) -> String {
    format!("{} kg", signed_weight(change_kg))
}

fn signed(change: i64) -> String {
    if change > 0 {
        format!("+{change}")
    } else {
        change.to_string()
    }
}

fn signed_weight(change_kg: f64) -> String {
    if change_kg > 0.0 {
        format!("+{change_kg:.1}")
    } else {
        format!("{change_kg:.1}")
    }
}

#[cfg(test)]
mod tests {
    //! 部位別の分析の URL と選択肢と増減の文言の単体テスト。
    //!
    //! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
    //! 要求の受入基準は E2E が担い、ここは API のクエリの組み立てと、画面の文言を確かめる。

    use warrior_core::body_part_analysis::{AnalysisMetric, AnalysisPeriod};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{
        body_parts_url, change_label, metric_options, period_options, sets_change_text, sets_text,
        weight_change_text, weight_text,
    };

    #[wasm_bindgen_test]
    fn the_url_carries_the_period_and_the_metric() {
        assert_eq!(
            body_parts_url(AnalysisPeriod::Week, AnalysisMetric::Sets),
            "/api/analysis/body-parts?period=week&metric=sets"
        );
        assert_eq!(
            body_parts_url(AnalysisPeriod::Month, AnalysisMetric::Weight),
            "/api/analysis/body-parts?period=month&metric=weight"
        );
    }

    #[wasm_bindgen_test]
    fn the_period_options_are_the_two_units() {
        assert_eq!(
            period_options(),
            vec![("week", "週単位"), ("month", "月単位")]
        );
        assert_eq!(change_label(AnalysisPeriod::Week), "前の週との差");
        assert_eq!(change_label(AnalysisPeriod::Month), "前の月との差");
    }

    #[wasm_bindgen_test]
    fn the_metric_options_are_the_sets_and_the_weight() {
        assert_eq!(
            metric_options(),
            vec![("sets", "セット数"), ("weight", "総重量")]
        );
    }

    #[wasm_bindgen_test]
    fn the_values_and_the_changes_use_their_units_and_signs() {
        assert_eq!(sets_text(12), "12 セット");
        assert_eq!(sets_change_text(2), "+2 セット");
        assert_eq!(sets_change_text(-1), "-1 セット");
        assert_eq!(sets_change_text(0), "0 セット");
        assert_eq!(weight_text(1_740.0), "1740.0 kg");
        assert_eq!(weight_change_text(240.5), "+240.5 kg");
        assert_eq!(weight_change_text(-80.0), "-80.0 kg");
        assert_eq!(weight_change_text(0.0), "0.0 kg");
    }
}
