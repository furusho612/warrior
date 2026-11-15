//! 推移のグラフの SVG の組み立て。
//!
//! 点の配置と目盛りの文言を、画面に依存しない純粋な関数として持つ。
//! 線と点の色はテーマのカスタムプロパティを使い、ライトとダークの両方で
//! 背景から判別できるようにする (REQ-ANL-002)。

use dioxus::prelude::*;
use warrior_core::history::{GraphMetric, GraphPoint};
use warrior_core::jst::JstDate;

use crate::calendar_view::format_seconds;
use crate::text::{text, Key};

/// グラフの描画領域の幅。
pub(crate) const WIDTH: f64 = 320.0;
/// グラフの描画領域の高さ。
pub(crate) const HEIGHT: f64 = 180.0;
const PADDING_LEFT: f64 = 44.0;
const PADDING_RIGHT: f64 = 10.0;
const PADDING_TOP: f64 = 10.0;
const PADDING_BOTTOM: f64 = 26.0;
const SECONDS_PER_DAY: i64 = 86_400;

/// グラフの 1 点の位置。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlotPoint {
    pub date: JstDate,
    pub value: f64,
    pub x: f64,
    pub y: f64,
}

/// 縦軸の目盛り。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AxisTick {
    pub y: f64,
    pub label: String,
}

/// 横軸の日付の目盛り。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DateLabel {
    pub x: f64,
    pub label: String,
}

/// グラフの配置。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GraphLayout {
    pub points: Vec<PlotPoint>,
    pub y_ticks: Vec<AxisTick>,
    pub x_labels: Vec<DateLabel>,
    pub line: String,
    pub empty: bool,
}

/// 点の列からグラフの配置を作る。
///
/// 縦軸は値の範囲に余白を足す。値が 1 つだけのときと、全ての値が同じときも
/// 範囲が 0 にならないようにする。横軸は最初と最後の日付の間の日数で等分する。
pub(crate) fn layout(points: &[GraphPoint], metric: GraphMetric) -> GraphLayout {
    if points.is_empty() {
        return GraphLayout {
            points: Vec::new(),
            y_ticks: Vec::new(),
            x_labels: Vec::new(),
            line: String::new(),
            empty: true,
        };
    }
    let (min, max) = value_range(points);
    let plot_width = WIDTH - PADDING_LEFT - PADDING_RIGHT;
    let plot_height = HEIGHT - PADDING_TOP - PADDING_BOTTOM;
    let first = points.first().map(|point| point.date);
    let last = points.last().map(|point| point.date);
    let first_seconds = first.map(|date| date.start_utc().seconds());
    let last_seconds = last.map(|date| date.start_utc().seconds());
    let plotted = points
        .iter()
        .map(|point| {
            let x = match (first_seconds, last_seconds) {
                (Some(first), Some(last)) if last > first => {
                    let days = (point.date.start_utc().seconds() - first) / SECONDS_PER_DAY;
                    let total = (last - first) / SECONDS_PER_DAY;
                    PADDING_LEFT + plot_width * days as f64 / total as f64
                }
                _ => PADDING_LEFT + plot_width / 2.0,
            };
            let y = PADDING_TOP + plot_height * (max - point.value) / (max - min);
            PlotPoint {
                date: point.date,
                value: point.value,
                x,
                y,
            }
        })
        .collect::<Vec<_>>();
    let line = plotted
        .iter()
        .map(|point| format!("{:.1},{:.1}", point.x, point.y))
        .collect::<Vec<_>>()
        .join(" ");
    GraphLayout {
        y_ticks: axis_ticks(min, max, metric),
        x_labels: date_labels(&plotted),
        points: plotted,
        line,
        empty: false,
    }
}

fn value_range(points: &[GraphPoint]) -> (f64, f64) {
    let min = points
        .iter()
        .map(|point| point.value)
        .fold(f64::INFINITY, f64::min);
    let max = points
        .iter()
        .map(|point| point.value)
        .fold(f64::NEG_INFINITY, f64::max);
    if max > min {
        return (min, max);
    }
    let margin = (max.abs() * 0.1).max(1.0);
    ((min - margin).max(0.0), max + margin)
}

fn axis_ticks(min: f64, max: f64, metric: GraphMetric) -> Vec<AxisTick> {
    let middle = (min + max) / 2.0;
    let plot_height = HEIGHT - PADDING_TOP - PADDING_BOTTOM;
    [max, middle, min]
        .into_iter()
        .enumerate()
        .map(|(index, value)| AxisTick {
            y: PADDING_TOP + plot_height * index as f64 / 2.0,
            label: tick_label(metric, value),
        })
        .collect()
}

fn tick_label(metric: GraphMetric, value: f64) -> String {
    match metric {
        GraphMetric::LongestDuration | GraphMetric::TotalDuration => {
            format_seconds(value.max(0.0).round() as u32)
        }
        GraphMetric::MaxWeight | GraphMetric::EstimatedOneRepMax | GraphMetric::TotalWeight => {
            format!("{value:.0} kg")
        }
    }
}

fn date_labels(points: &[PlotPoint]) -> Vec<DateLabel> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let last = points.last().unwrap_or(first);
    if first.date == last.date {
        return vec![DateLabel {
            x: first.x,
            label: date_text(first.date),
        }];
    }
    vec![
        DateLabel {
            x: first.x,
            label: date_text(first.date),
        },
        DateLabel {
            x: last.x,
            label: date_text(last.date),
        },
    ]
}

/// グラフの点の値を文言にする。
pub(crate) fn point_value_text(metric: GraphMetric, value: f64) -> String {
    match metric {
        GraphMetric::LongestDuration | GraphMetric::TotalDuration => {
            format_seconds(value.max(0.0).round() as u32)
        }
        GraphMetric::MaxWeight | GraphMetric::EstimatedOneRepMax | GraphMetric::TotalWeight => {
            format!("{value:.1} kg")
        }
    }
}

fn date_text(date: JstDate) -> String {
    format!("{}/{}", date.month(), date.day())
}

/// グラフの線と点の色。テーマの主色を使う。
pub(crate) const LINE_CLASS: &str = "stroke-primary";
/// グラフの軸と目盛りの色。テーマの線の色を使う。
pub(crate) const AXIS_CLASS: &str = "stroke-line";
/// グラフの目盛りの文言の色。テーマの補助の文字の色を使う。
pub(crate) const LABEL_CLASS: &str = "fill-text-sub";

/// 推移のグラフ。
#[component]
pub fn GraphChart(points: Vec<GraphPoint>, metric: GraphMetric) -> Element {
    let graph = layout(&points, metric);
    let aria = text(Key::ExerciseHistoryGraph);
    rsx! {
        svg {
            class: "h-auto w-full",
            view_box: "0 0 {WIDTH} {HEIGHT}",
            role: "img",
            "aria-label": "{aria}",
            for tick in graph.y_ticks.iter() {
                line {
                    class: AXIS_CLASS,
                    x1: "{PADDING_LEFT}",
                    x2: "{WIDTH - PADDING_RIGHT}",
                    y1: "{tick.y}",
                    y2: "{tick.y}",
                    stroke_width: "1",
                    "aria-hidden": "true"
                }
                text {
                    class: LABEL_CLASS,
                    x: "{PADDING_LEFT - 4.0}",
                    y: "{tick.y + 3.0}",
                    "text-anchor": "end",
                    font_size: "9",
                    "{tick.label}"
                }
            }
            for label in graph.x_labels.iter() {
                text {
                    class: LABEL_CLASS,
                    x: "{label.x}",
                    y: "{HEIGHT - 8.0}",
                    "text-anchor": "middle",
                    font_size: "9",
                    "{label.label}"
                }
            }
            if graph.empty {
                text {
                    class: LABEL_CLASS,
                    x: "{WIDTH / 2.0}",
                    y: "{HEIGHT / 2.0}",
                    "text-anchor": "middle",
                    font_size: "11",
                    "{text(Key::ExerciseHistoryGraphEmpty)}"
                }
            } else {
                polyline {
                    class: LINE_CLASS,
                    fill: "none",
                    stroke_width: "2",
                    points: graph.line.clone(),
                    "data-graph-line": "true"
                }
                for point in graph.points.iter() {
                    circle {
                        class: "fill-primary",
                        cx: "{point.x}",
                        cy: "{point.y}",
                        r: "3",
                        "data-graph-point": "true",
                        "data-graph-date": "{point.date}",
                        "data-graph-value": "{point.value}",
                        title { "{point.date}: {point_value_text(metric, point.value)}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! グラフの配置と、ライトとダークの色の判別の単体テスト。
    //!
    //! 対応する要求: `REQ-ANL-002`。
    //! 要求の受入基準は E2E が担い、ここは点の位置、目盛り、色の選択を確かめる。

    use warrior_core::history::{GraphMetric, GraphPoint};
    use warrior_core::jst::JstDate;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{
        layout, point_value_text, AXIS_CLASS, HEIGHT, LABEL_CLASS, LINE_CLASS, PADDING_LEFT,
        PADDING_RIGHT, PADDING_TOP, WIDTH,
    };
    use crate::generated::tokens;

    #[wasm_bindgen_test]
    fn an_empty_graph_has_no_points_and_no_line() {
        let graph = layout(&[], GraphMetric::MaxWeight);
        assert!(graph.empty);
        assert!(graph.points.is_empty());
        assert!(graph.line.is_empty());
        assert!(graph.y_ticks.is_empty());
        assert!(graph.x_labels.is_empty());
    }

    #[wasm_bindgen_test]
    fn a_single_point_is_centered_and_the_value_range_has_a_margin() {
        let point = Point::new(2026, 11, 12, 100.0).point();
        let graph = layout(&[point], GraphMetric::MaxWeight);
        assert!(!graph.empty);
        assert_eq!(graph.points.len(), 1);
        assert_eq!(
            graph.points[0].x,
            PADDING_LEFT + (WIDTH - PADDING_LEFT - PADDING_RIGHT) / 2.0
        );
        assert!(graph.points[0].y > PADDING_TOP);
        assert!(graph.points[0].y < HEIGHT);
        assert_eq!(graph.x_labels.len(), 1);
        assert_eq!(graph.x_labels[0].label, "11/12");
        assert_eq!(graph.y_ticks.len(), 3);
        assert_eq!(graph.y_ticks[0].label, "110 kg");
        assert_eq!(graph.y_ticks[2].label, "90 kg");
        assert_eq!(
            graph.line,
            format!("{:.1},{:.1}", graph.points[0].x, graph.points[0].y)
        );
    }

    #[wasm_bindgen_test]
    fn later_dates_are_further_right_and_larger_values_are_higher() {
        let points = vec![
            Point::new(2026, 11, 1, 80.0).point(),
            Point::new(2026, 11, 11, 100.0).point(),
            Point::new(2026, 11, 21, 90.0).point(),
        ];
        let graph = layout(&points, GraphMetric::MaxWeight);
        assert_eq!(graph.points.len(), 3);
        assert!(graph.points[0].x < graph.points[1].x);
        assert!(graph.points[1].x < graph.points[2].x);
        assert!(graph.points[1].y < graph.points[0].y);
        assert!(graph.points[1].y < graph.points[2].y);
        assert!(graph.points[2].y < graph.points[0].y);
        assert_eq!(graph.x_labels.len(), 2);
        assert_eq!(graph.x_labels[0].label, "11/1");
        assert_eq!(graph.x_labels[1].label, "11/21");
        assert!(graph.line.split(' ').count() == 3);
        assert!(!graph.line.contains("NaN"));
    }

    #[wasm_bindgen_test]
    fn equal_values_keep_a_non_zero_range() {
        let points = vec![
            Point::new(2026, 11, 1, 60.0).point(),
            Point::new(2026, 11, 2, 60.0).point(),
        ];
        let graph = layout(&points, GraphMetric::EstimatedOneRepMax);
        assert_eq!(graph.points.len(), 2);
        for point in &graph.points {
            assert!(point.y.is_finite());
            assert!(point.y > PADDING_TOP);
        }
        assert_eq!(graph.y_ticks.len(), 3);
    }

    #[wasm_bindgen_test]
    fn the_time_axis_formats_the_seconds() {
        let points = vec![
            Point::new(2026, 11, 1, 60.0).point(),
            Point::new(2026, 11, 2, 90.0).point(),
        ];
        let graph = layout(&points, GraphMetric::LongestDuration);
        assert_eq!(graph.y_ticks[2].label, "1分0秒");
        assert_eq!(graph.y_ticks[0].label, "1分30秒");
        assert_eq!(
            point_value_text(GraphMetric::TotalDuration, 150.0),
            "2分30秒"
        );
        assert_eq!(point_value_text(GraphMetric::MaxWeight, 100.0), "100.0 kg");
    }

    #[wasm_bindgen_test]
    fn the_line_and_the_axis_use_the_theme_colors() {
        assert_eq!(LINE_CLASS, "stroke-primary");
        assert_eq!(AXIS_CLASS, "stroke-line");
        assert_eq!(LABEL_CLASS, "fill-text-sub");
    }

    #[wasm_bindgen_test]
    fn the_line_is_distinguishable_from_the_background_in_both_themes() {
        for (primary, background) in [
            (tokens::color::dark::PRIMARY, tokens::color::dark::BG),
            (tokens::color::light::PRIMARY, tokens::color::light::BG),
        ] {
            assert!(
                contrast_ratio(primary, background) >= 4.5,
                "{primary} on {background} must be readable"
            );
        }
    }

    #[wasm_bindgen_test]
    fn the_date_labels_use_the_japan_date() {
        assert_eq!(super::date_text(JstDate::new(2026, 1, 5).unwrap()), "1/5");
    }

    struct Point {
        date: JstDate,
        value: f64,
    }

    impl Point {
        fn new(year: i64, month: u32, day: u32, value: f64) -> Self {
            Self {
                date: JstDate::new(year, month, day).expect("a valid date"),
                value,
            }
        }

        fn point(&self) -> GraphPoint {
            GraphPoint {
                date: self.date,
                value: self.value,
            }
        }
    }

    fn contrast_ratio(first: &str, second: &str) -> f64 {
        let first = luminance(first);
        let second = luminance(second);
        let (lighter, darker) = if first > second {
            (first, second)
        } else {
            (second, first)
        };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn luminance(color: &str) -> f64 {
        let color = color.trim_start_matches('#');
        let channels = (0..3)
            .map(|index| {
                let start = index * 2;
                u8::from_str_radix(&color[start..start + 2], 16).expect("a hex channel") as f64
                    / 255.0
            })
            .collect::<Vec<_>>();
        let linear = channels
            .iter()
            .map(|channel| {
                if *channel <= 0.039_28 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                }
            })
            .collect::<Vec<_>>();
        0.212_6 * linear[0] + 0.715_2 * linear[1] + 0.072_2 * linear[2]
    }
}
