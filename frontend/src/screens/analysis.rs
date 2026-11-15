//! 部位別のセット数と総重量の分析と、部位区分ごとの回復状況の画面。
//!
//! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`、`REQ-ANL-008`、
//! `REQ-ANL-009`、`REQ-ANL-010`。
//! 期間と指標の選択肢は `warrior_core::body_part_analysis` から作り、集計はサーバーの
//! 応答を使う (ADR-016)。前の週と前の月との増減は応答の値をそのまま表示する。回復状況も
//! サーバーの応答を表示し、目安であることの注記を添える。

use dioxus::prelude::*;
use warrior_core::body_part_analysis::{AnalysisMetric, AnalysisPeriod, BodyPartAnalysis};
use warrior_core::recovery::RecoveryStatus;

use crate::analysis as analysis_api;
use crate::calendar_view::part_label;
use crate::graph::BodyPartBarChart;
use crate::text::{text, Key};

/// 部位別の分析の画面。
#[component]
pub fn AnalysisScreen() -> Element {
    let state = use_analysis_state();
    let analysis_now = (state.analysis)();
    let error = (state.error)();
    let recovery_now = (state.recovery)();
    let recovery_error = (state.recovery_error)();
    let period = (state.period)();
    let metric = (state.metric)();
    let loading = analysis_now.is_none() && error.is_none();
    let period_state = state;
    let metric_state = state;
    let retry_state = state;
    let recovery_retry_state = state;

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-analysis": "true",
            "data-analysis-period": "{period.as_str()}",
            "data-analysis-metric": "{metric.as_str()}",
            h1 { class: "text-lg font-bold", "{text(Key::AnalysisTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub",
                "{text(Key::AnalysisDescription)}"
            }
            div { class: "flex flex-wrap gap-2",
                label {
                    class: "flex flex-col gap-1 text-[11px] font-bold text-text-sub",
                    "{text(Key::AnalysisPeriodLabel)}"
                    select {
                        class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                        "data-analysis-period-select": "true",
                        value: "{period.as_str()}",
                        onchange: move |event| {
                            let mut state = period_state;
                            if let Some(period) = AnalysisPeriod::parse(&event.value()) {
                                state.period.set(period);
                                state.analysis.set(None);
                                state.error.set(None);
                            }
                        },
                        for (value, label) in analysis_api::period_options() {
                            option {
                                value: "{value}",
                                selected: period.as_str() == value,
                                "{label}"
                            }
                        }
                    }
                }
                label {
                    class: "flex flex-col gap-1 text-[11px] font-bold text-text-sub",
                    "{text(Key::AnalysisMetricLabel)}"
                    select {
                        class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                        "data-analysis-metric-select": "true",
                        value: "{metric.as_str()}",
                        onchange: move |event| {
                            let mut state = metric_state;
                            if let Some(metric) = AnalysisMetric::parse(&event.value()) {
                                state.metric.set(metric);
                                state.analysis.set(None);
                                state.error.set(None);
                            }
                        },
                        for (value, label) in analysis_api::metric_options() {
                            option {
                                value: "{value}",
                                selected: metric.as_str() == value,
                                "{label}"
                            }
                        }
                    }
                }
            }
            if let Some(message) = error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-analysis-retry": "true",
                    onclick: move |_| {
                        let mut state = retry_state;
                        state.analysis.set(None);
                        state.reload += 1;
                    },
                    "{text(Key::AnalysisRetry)}"
                }
            } else if let Some(analysis) = analysis_now {
                AnalysisParts { analysis: analysis.clone() }
                section {
                    class: "flex flex-col gap-2 border-t border-line pt-3",
                    h2 { class: "text-sm font-bold", "{text(Key::AnalysisChart)}" }
                    BodyPartBarChart {
                        entries: analysis.parts.clone(),
                        period: analysis.period,
                        metric: analysis.metric,
                    }
                }
            } else if loading {
                p { class: "text-[13px] text-text-sub", "{text(Key::AnalysisLoading)}" }
            }
            if let Some(recovery) = recovery_now {
                RecoveryParts { status: recovery.clone() }
            } else if let Some(message) = recovery_error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-recovery-retry": "true",
                    onclick: move |_| {
                        let mut state = recovery_retry_state;
                        state.recovery.set(None);
                        state.recovery_reload += 1;
                    },
                    "{text(Key::AnalysisRecoveryRetry)}"
                }
            }
        }
    }
}

/// 部位別の分析と回復状況の読み込み状態。
#[derive(Clone, Copy)]
struct AnalysisSignals {
    period: Signal<AnalysisPeriod>,
    metric: Signal<AnalysisMetric>,
    analysis: Signal<Option<BodyPartAnalysis>>,
    error: Signal<Option<String>>,
    generation: Signal<u32>,
    reload: Signal<u32>,
    recovery: Signal<Option<RecoveryStatus>>,
    recovery_error: Signal<Option<String>>,
    recovery_generation: Signal<u32>,
    recovery_reload: Signal<u32>,
}

/// 画面の状態を作り、分析と回復状況の読み込みを始める効果を登録する。
fn use_analysis_state() -> AnalysisSignals {
    let state = AnalysisSignals {
        period: use_signal(|| AnalysisPeriod::Week),
        metric: use_signal(|| AnalysisMetric::Sets),
        analysis: use_signal(|| Option::<BodyPartAnalysis>::None),
        error: use_signal(|| Option::<String>::None),
        generation: use_signal(|| 0u32),
        reload: use_signal(|| 0u32),
        recovery: use_signal(|| Option::<RecoveryStatus>::None),
        recovery_error: use_signal(|| Option::<String>::None),
        recovery_generation: use_signal(|| 0u32),
        recovery_reload: use_signal(|| 0u32),
    };

    use_effect(move || {
        let mut state = state;
        let _ = (state.reload)();
        let period = (state.period)();
        let metric = (state.metric)();
        let generation = state.generation.peek().wrapping_add(1);
        state.generation.set(generation);
        state.error.set(None);
        spawn(load_analysis(period, metric, generation, state));
    });

    use_effect(move || {
        let mut state = state;
        let _ = (state.recovery_reload)();
        let generation = state.recovery_generation.peek().wrapping_add(1);
        state.recovery_generation.set(generation);
        state.recovery_error.set(None);
        spawn(load_recovery(generation, state));
    });

    state
}

async fn load_analysis(
    period: AnalysisPeriod,
    metric: AnalysisMetric,
    generation: u32,
    mut state: AnalysisSignals,
) {
    let result = analysis_api::body_parts(period, metric).await;
    if generation != *state.generation.peek()
        || (state.period)() != period
        || (state.metric)() != metric
    {
        return;
    }
    match result {
        Ok(analysis) => {
            state.analysis.set(Some(analysis));
            state.error.set(None);
        }
        Err(_) => {
            state.analysis.set(None);
            state
                .error
                .set(Some(text(Key::AnalysisLoadFailed).to_owned()));
        }
    }
}

async fn load_recovery(generation: u32, mut state: AnalysisSignals) {
    let result = analysis_api::recovery().await;
    if generation != *state.recovery_generation.peek() {
        return;
    }
    match result {
        Ok(recovery) => {
            state.recovery.set(Some(recovery));
            state.recovery_error.set(None);
        }
        Err(_) => {
            state.recovery.set(None);
            state
                .recovery_error
                .set(Some(text(Key::AnalysisRecoveryLoadFailed).to_owned()));
        }
    }
}

/// 部位区分ごとのセット数と総重量、前の期間との増減。
#[component]
fn AnalysisParts(analysis: BodyPartAnalysis) -> Element {
    let change_label = analysis_api::change_label(analysis.period);
    let count = analysis.parts.len();
    rsx! {
        section {
            class: "flex flex-col gap-1 border-t border-line pt-3",
            "data-analysis-parts": "true",
            "data-analysis-part-count": "{count}",
            h2 { class: "text-sm font-bold", "{text(Key::AnalysisParts)}" }
            ul { class: "flex flex-col gap-1",
                for part in analysis.parts {
                    li {
                        class: "flex min-h-11 flex-wrap items-baseline justify-between gap-2 border-b border-line",
                        "data-body-part": "{part.body_part.as_str()}",
                        "data-body-part-sets": "{part.sets}",
                        "data-body-part-weight": "{part.weight_kg}",
                        "data-body-part-previous-sets": "{part.previous_sets}",
                        "data-body-part-previous-weight": "{part.previous_weight_kg}",
                        "data-body-part-sets-change": "{part.sets_change}",
                        "data-body-part-weight-change": "{part.weight_change_kg}",
                        span { class: "text-[13px] font-bold", "{part_label(part.body_part)}" }
                        span { class: "flex flex-col items-end",
                            span { class: "font-condensed text-[15px] font-semibold",
                                "{analysis_api::sets_text(part.sets)}"
                            }
                            span { class: "text-[11px] text-text-sub",
                                "{change_label} {analysis_api::sets_change_text(part.sets_change)}"
                            }
                        }
                        span { class: "flex flex-col items-end",
                            span { class: "font-condensed text-[15px] font-semibold",
                                "{analysis_api::weight_text(part.weight_kg)}"
                            }
                            span { class: "text-[11px] text-text-sub",
                                "{change_label} {analysis_api::weight_change_text(part.weight_change_kg)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 部位区分ごとの回復状況と、目安であることの注記。
#[component]
fn RecoveryParts(status: RecoveryStatus) -> Element {
    let count = status.parts.len();
    rsx! {
        section {
            class: "flex flex-col gap-1 border-t border-line pt-3",
            "data-recovery": "true",
            "data-recovery-part-count": "{count}",
            h2 { class: "text-sm font-bold", "{text(Key::AnalysisRecoveryTitle)}" }
            ul { class: "flex flex-col gap-1",
                for part in status.parts {
                    li {
                        class: "flex min-h-11 items-center justify-between gap-2 border-b border-line",
                        "data-recovery-part": "{part.body_part.as_str()}",
                        "data-recovery-state": "{analysis_api::recovery_state_value(part.state)}",
                        span { class: "text-[13px] font-bold", "{part_label(part.body_part)}" }
                        span { class: "text-[13px]",
                            {match part.state {
                                Some(state) => analysis_api::recovery_label(state),
                                None => text(Key::AnalysisRecoveryNoRecord),
                            }}
                        }
                    }
                }
            }
            p {
                class: "text-[11px] leading-5 text-text-sub",
                "data-recovery-note": "true",
                "{text(Key::AnalysisRecoveryNote)}"
            }
        }
    }
}
