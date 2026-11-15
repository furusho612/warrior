//! 種目別の履歴、自己ベスト、目標重量、推移のグラフの画面。
//!
//! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`、
//! `REQ-GOAL-001`、`REQ-GOAL-002`。
//! 期間と指標の選択肢は種目の記録方式から作り、集計はサーバーの応答を使う (ADR-016)。
//! 目標重量と現在の記録との差は履歴の応答から表示し、グラフには目標の線を重ねる。
//! セットの文言はカレンダーの日の詳細と同じ組み立てを共有する。

use dioxus::prelude::*;
use warrior_core::calendar::CalendarSet;
use warrior_core::exercise::GripMode;
use warrior_core::goal::GoalProgress;
use warrior_core::history::{
    ExerciseBest, ExerciseGraph, ExerciseHistory, GraphMetric, GraphPeriod, GripFilter,
    HistoryTraining,
};
use warrior_core::jst::JstDate;
use warrior_core::training_session::BestKind;

use crate::calendar_view::{day_title, format_seconds, set_notes, set_text};
use crate::goal as goal_api;
use crate::graph::GraphChart;
use crate::grip_text::{attachment_label, grip_label};
use crate::history as history_api;
use crate::text::{text, Key};

/// 種目別の履歴とグラフの画面。
#[component]
pub fn ExerciseHistoryScreen(id: String) -> Element {
    let state = use_history_state(id.clone());
    let history_now = (state.history)();
    let history_error = (state.history_error)();
    let graph_now = (state.graph)();
    let graph_error = (state.graph_error)();
    let choice = (state.filter)();
    let period = (state.period)();
    let record_mode = history_now.as_ref().map(|history| history.record_mode);
    let metric = (state.metric)();
    let filter_choices = history_now
        .as_ref()
        .map(history_api::filter_choices)
        .unwrap_or_default();
    let loading = history_now.is_none() && history_error.is_none();
    let graph_retry = EventHandler::new(move |_: ()| {
        let mut state = state;
        state.graph_reload += 1;
    });
    let filter_state = state;
    let period_state = state;
    let metric_state = state;
    let history_retry_state = state;
    let goal_changed_state = state;
    let on_goal_changed = EventHandler::new(move |_: ()| {
        let mut state = goal_changed_state;
        state.history_reload += 1;
        state.graph_reload += 1;
    });

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-3 p-5",
            "data-exercise-history": "{id}",
            h1 { class: "text-lg font-bold", "{text(Key::ExerciseHistoryTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub",
                "{text(Key::ExerciseHistoryDescription)}"
            }
            if let Some(message) = history_error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-history-retry": "true",
                    onclick: move |_| {
                        let mut state = history_retry_state;
                        state.history_reload += 1;
                    },
                    "{text(Key::ExerciseHistoryRetry)}"
                }
            } else if let Some(history) = history_now {
                h2 { class: "text-base font-bold", "{history.name}" }
                if history.grip_mode != warrior_core::exercise::GripMode::None {
                    label {
                        class: "flex flex-col gap-1 text-[11px] font-bold text-text-sub",
                        "{text(Key::ExerciseHistoryGripFilter)}"
                        select {
                            class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                            "data-history-grip-filter": "true",
                            value: choice.clone().unwrap_or_default(),
                            onchange: move |event| {
                                let mut state = filter_state;
                                let value = event.value();
                                state.filter.set((!value.is_empty()).then_some(value));
                                state.history.set(None);
                                state.history_error.set(None);
                                state.graph.set(None);
                                state.graph_error.set(None);
                            },
                            option {
                                value: "",
                                selected: choice.is_none(),
                                "{text(Key::ExerciseHistoryAllGrips)}"
                            }
                            for option in filter_choices {
                                option {
                                    value: option.value.clone(),
                                    selected: choice.as_deref() == Some(option.value.as_str()),
                                    "{option.label}"
                                }
                            }
                        }
                    }
                }
                HistoryBests { bests: history.bests.clone() }
                GoalSection {
                    exercise_id: id.clone(),
                    grip_mode: history.grip_mode,
                    choice: choice.clone(),
                    goals: history.goals.clone(),
                    on_changed: on_goal_changed,
                }
                if let Some(mode) = record_mode {
                    div { class: "flex flex-wrap gap-2",
                        label {
                            class: "flex flex-col gap-1 text-[11px] font-bold text-text-sub",
                            "{text(Key::ExerciseHistoryPeriodLabel)}"
                            select {
                                class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                                "data-history-period": "true",
                                value: "{period.as_str()}",
                                onchange: move |event| {
                                    let mut state = period_state;
                                    if let Some(period) = GraphPeriod::parse(&event.value()) {
                                        state.period.set(period);
                                        state.graph.set(None);
                                        state.graph_error.set(None);
                                    }
                                },
                                for (value, label) in history_api::period_options() {
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
                            "{text(Key::ExerciseHistoryMetricLabel)}"
                            select {
                                class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                                "data-history-metric": "true",
                                value: metric.map(GraphMetric::as_str).unwrap_or_default(),
                                onchange: move |event| {
                                    let mut state = metric_state;
                                    if let Some(metric) = GraphMetric::parse(&event.value()) {
                                        state.metric.set(Some(metric));
                                        state.graph.set(None);
                                        state.graph_error.set(None);
                                    }
                                },
                                for (value, label) in history_api::metric_options(mode) {
                                    option {
                                        value: "{value}",
                                        selected: metric.map(GraphMetric::as_str) == Some(value),
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
                GraphPanel {
                    graph: graph_now,
                    metric,
                    error: graph_error,
                    on_retry: graph_retry,
                }
                TrainingList { trainings: history.trainings.clone() }
            } else if loading {
                p { class: "text-[13px] text-text-sub", "{text(Key::ExerciseHistoryLoading)}" }
            }
        }
    }
}

/// 履歴とグラフの読み込み状態。
#[derive(Clone, Copy)]
struct HistorySignals {
    history: Signal<Option<ExerciseHistory>>,
    history_error: Signal<Option<String>>,
    filter: Signal<Option<String>>,
    period: Signal<GraphPeriod>,
    metric: Signal<Option<GraphMetric>>,
    graph: Signal<Option<ExerciseGraph>>,
    graph_error: Signal<Option<String>>,
    history_generation: Signal<u32>,
    graph_generation: Signal<u32>,
    history_reload: Signal<u32>,
    graph_reload: Signal<u32>,
}

/// 画面の状態を作り、履歴とグラフの読み込みを始める効果を登録する。
fn use_history_state(id: String) -> HistorySignals {
    let state = HistorySignals {
        history: use_signal(|| Option::<ExerciseHistory>::None),
        history_error: use_signal(|| Option::<String>::None),
        filter: use_signal(|| Option::<String>::None),
        period: use_signal(|| GraphPeriod::ThreeMonths),
        metric: use_signal(|| Option::<GraphMetric>::None),
        graph: use_signal(|| Option::<ExerciseGraph>::None),
        graph_error: use_signal(|| Option::<String>::None),
        history_generation: use_signal(|| 0u32),
        graph_generation: use_signal(|| 0u32),
        history_reload: use_signal(|| 0u32),
        graph_reload: use_signal(|| 0u32),
    };

    let history_id = id.clone();
    use_effect(move || {
        let mut state = state;
        let _ = (state.history_reload)();
        let choice = (state.filter)();
        let generation = state.history_generation.peek().wrapping_add(1);
        state.history_generation.set(generation);
        state.history_error.set(None);
        spawn(load_history(history_id.clone(), choice, generation, state));
    });

    use_effect(move || {
        let mut state = state;
        let _ = (state.graph_reload)();
        let choice = (state.filter)();
        let period = (state.period)();
        let Some(metric) = (state.metric)() else {
            return;
        };
        let generation = state.graph_generation.peek().wrapping_add(1);
        state.graph_generation.set(generation);
        state.graph_error.set(None);
        spawn(load_graph(
            id.clone(),
            choice,
            period,
            metric,
            generation,
            state,
        ));
    });

    state
}

async fn load_history(
    id: String,
    choice: Option<String>,
    generation: u32,
    mut state: HistorySignals,
) {
    let filter = history_api::filter_from_choice(choice.as_deref());
    let result = history_api::history(&id, &filter).await;
    if generation != *state.history_generation.peek() {
        return;
    }
    match result {
        Ok(history) => {
            if state
                .metric
                .peek()
                .is_none_or(|metric| !metric.is_allowed_for(history.record_mode))
            {
                state
                    .metric
                    .set(history_api::default_metric(history.record_mode));
            }
            state.history.set(Some(history));
            state.history_error.set(None);
        }
        Err(_) => {
            state.history.set(None);
            state
                .history_error
                .set(Some(text(Key::ExerciseHistoryLoadFailed).to_owned()));
        }
    }
}

async fn load_graph(
    id: String,
    choice: Option<String>,
    period: GraphPeriod,
    metric: GraphMetric,
    generation: u32,
    mut state: HistorySignals,
) {
    let filter = history_api::filter_from_choice(choice.as_deref());
    let result = history_api::graph(&id, &filter, period, metric).await;
    if generation != *state.graph_generation.peek()
        || (state.period)() != period
        || (state.metric)() != Some(metric)
        || (state.filter)() != choice
    {
        return;
    }
    match result {
        Ok(graph) => {
            state.graph.set(Some(graph));
            state.graph_error.set(None);
        }
        Err(_) => {
            state.graph.set(None);
            state
                .graph_error
                .set(Some(text(Key::ExerciseHistoryGraphLoadFailed).to_owned()));
        }
    }
}

/// グラフと、その読み込みの状態。
#[component]
fn GraphPanel(
    graph: Option<ExerciseGraph>,
    metric: Option<GraphMetric>,
    error: Option<String>,
    on_retry: EventHandler<()>,
) -> Element {
    let points = graph.as_ref().map(|graph| graph.points.len()).unwrap_or(0);
    let target = graph
        .as_ref()
        .and_then(|graph| match graph.goals.as_slice() {
            [goal] => Some(goal.goal.target_weight_kg),
            _ => None,
        });
    rsx! {
        section {
            class: "flex flex-col gap-2 border-t border-line pt-3",
            "data-history-graph": "true",
            "data-graph-points": "{points}",
            h2 { class: "text-sm font-bold", "{text(Key::ExerciseHistoryGraph)}" }
            if let Some(message) = error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-history-graph-retry": "true",
                    onclick: move |_| on_retry.call(()),
                    "{text(Key::ExerciseHistoryRetry)}"
                }
            } else if let (Some(graph), Some(metric)) = (graph, metric) {
                div {
                    "data-graph-period": graph.period.as_str(),
                    "data-graph-metric": graph.metric.as_str(),
                    GraphChart {
                        points: graph.points.clone(),
                        metric,
                        target,
                    }
                }
            } else {
                p { class: "text-[13px] text-text-sub", "{text(Key::ExerciseHistoryLoading)}" }
            }
        }
    }
}

/// 目標重量と現在の記録との差。
#[component]
fn GoalSection(
    exercise_id: String,
    grip_mode: GripMode,
    choice: Option<String>,
    goals: Vec<GoalProgress>,
    on_changed: EventHandler<()>,
) -> Element {
    let count = goals.len();
    let empty = goals.is_empty();
    let selected = selected_goal(&goals, choice.as_deref());
    let can_edit = grip_mode == GripMode::None || choice.is_some();
    let editor_key = selected.as_ref().map_or_else(
        || "new".to_owned(),
        |progress| {
            format!(
                "{:?}-{:?}-{}-{:?}",
                progress.goal.grip,
                progress.goal.attachment,
                progress.goal.target_weight_kg,
                progress.goal.target_date
            )
        },
    );
    let initial_weight = selected.as_ref().map_or_else(String::new, |progress| {
        progress.goal.target_weight_kg.to_string()
    });
    let initial_date = selected
        .as_ref()
        .and_then(|progress| progress.goal.target_date)
        .map_or_else(String::new, |date| date.to_string());
    let list_id = exercise_id.clone();
    let editor_id = exercise_id;
    let editor_choice = choice.clone();

    rsx! {
        section {
            class: "flex flex-col gap-1 border-t border-line pt-3",
            "data-history-goals": "true",
            "data-history-goals-count": "{count}",
            h2 { class: "text-sm font-bold", "{text(Key::ExerciseHistoryGoals)}" }
            if empty {
                p { class: "text-[13px] text-text-sub", "{text(Key::ExerciseHistoryGoalsEmpty)}" }
            }
            ul { class: "flex flex-col gap-1",
                for progress in goals {
                    GoalRow {
                        exercise_id: list_id.clone(),
                        progress,
                        on_changed,
                    }
                }
            }
            if can_edit {
                GoalEditor {
                    key: "{editor_key}",
                    exercise_id: editor_id,
                    choice: editor_choice,
                    initial_weight,
                    initial_date,
                    on_changed,
                }
            } else {
                p { class: "text-[12px] text-text-sub",
                    "{text(Key::ExerciseHistoryGoalSelectGrip)}"
                }
            }
        }
    }
}

fn selected_goal(goals: &[GoalProgress], choice: Option<&str>) -> Option<GoalProgress> {
    let filter = history_api::filter_from_choice(choice);
    goals
        .iter()
        .find(|progress| {
            progress.goal.grip == filter.grip && progress.goal.attachment == filter.attachment
        })
        .cloned()
}

/// 保存済みの 1 つの目標。
#[component]
fn GoalRow(exercise_id: String, progress: GoalProgress, on_changed: EventHandler<()>) -> Element {
    let target = progress.goal.target_weight_kg;
    let current = progress.current_weight_kg;
    let difference = progress.difference_kg;
    let target_date = progress.goal.target_date;
    let equipment = goal_equipment(&progress);
    let key = goal_key(&progress);
    let mut deleting = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let filter = GripFilter::new(progress.goal.grip.clone(), progress.goal.attachment.clone());

    rsx! {
        li {
            class: "flex min-h-11 flex-wrap items-baseline justify-between gap-2 border-b border-line",
            "data-history-goal": "{key}",
            "data-history-goal-target": "{target}",
            "data-history-goal-current": "{current.map_or_else(String::new, |value| value.to_string())}",
            "data-history-goal-difference": "{difference.map_or_else(String::new, |value| value.to_string())}",
            "data-history-goal-date": "{target_date.map_or_else(String::new, |date| date.to_string())}",
            div { class: "flex min-w-0 flex-1 flex-col gap-0.5",
                span { class: "text-[13px] font-semibold",
                    "{text(Key::ExerciseHistoryGoalTarget)} {goal_api::weight_label(target)}"
                    if let Some(date) = target_date {
                        span { class: "text-[11px] text-text-sub",
                            " / {text(Key::ExerciseHistoryGoalDate)} {date}"
                        }
                    }
                }
                if !equipment.is_empty() {
                    span { class: "text-[11px] text-text-sub", "{equipment}" }
                }
                if let Some(value) = current {
                    span { class: "text-[11px] text-text-sub",
                        "{text(Key::ExerciseHistoryGoalCurrent)} {goal_api::weight_label(value)}"
                    }
                } else {
                    span { class: "text-[11px] text-text-sub",
                        "{text(Key::ExerciseHistoryGoalCurrentMissing)}"
                    }
                }
                if let Some(value) = difference {
                    span { class: "text-[12px] font-semibold",
                        "{goal_api::difference_label(value)}"
                    }
                } else {
                    span { class: "text-[11px] text-text-sub",
                        "{text(Key::ExerciseHistoryGoalDifferenceMissing)}"
                    }
                }
                if let Some(message) = (error)() {
                    span { class: "text-[11px] text-primary", "{message}" }
                }
            }
            button {
                class: "min-h-11 border border-line px-3 text-[12px]",
                "data-goal-delete": "true",
                disabled: (deleting)(),
                onclick: move |_| {
                    deleting.set(true);
                    error.set(None);
                    let id = exercise_id.clone();
                    let filter = filter.clone();
                    spawn(async move {
                        match goal_api::delete(&id, &filter).await {
                            Ok(true) => on_changed.call(()),
                            Ok(false) => error.set(Some(text(Key::ExerciseHistoryGoalDeleteFailed).to_owned())),
                            Err(_) => error.set(Some(text(Key::ExerciseHistoryGoalDeleteFailed).to_owned())),
                        }
                        deleting.set(false);
                    });
                },
                "{text(Key::ExerciseHistoryGoalDelete)}"
            }
        }
    }
}

/// 目標の設定と変更の入力。
#[component]
fn GoalEditor(
    exercise_id: String,
    choice: Option<String>,
    initial_weight: String,
    initial_date: String,
    on_changed: EventHandler<()>,
) -> Element {
    let mut weight = use_signal(|| initial_weight);
    let mut date = use_signal(|| initial_date);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut notice = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "flex flex-col gap-2 border-t border-line pt-2",
            "data-goal-editor": "true",
            label { class: "flex flex-col gap-1 text-[11px] font-bold text-text-sub",
                "{text(Key::ExerciseHistoryGoalWeightLabel)}"
                input {
                    class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                    "data-goal-weight": "true",
                    r#type: "number",
                    step: "0.5",
                    min: "0",
                    max: "1000",
                    value: "{weight}",
                    oninput: move |event| weight.set(event.value()),
                }
            }
            label { class: "flex flex-col gap-1 text-[11px] font-bold text-text-sub",
                "{text(Key::ExerciseHistoryGoalDateLabel)}"
                input {
                    class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                    "data-goal-date": "true",
                    r#type: "date",
                    value: "{date}",
                    oninput: move |event| date.set(event.value()),
                }
            }
            if let Some(message) = (error)() {
                p { class: "text-[12px] text-primary", "{message}" }
            }
            if let Some(message) = (notice)() {
                p { class: "text-[12px] text-text-sub", "{message}" }
            }
            button {
                class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                "data-goal-save": "true",
                disabled: (saving)(),
                onclick: move |_| {
                    let Some(target) = parse_weight(&(weight)()) else {
                        error.set(Some(text(Key::ExerciseHistoryGoalWeightInvalid).to_owned()));
                        return;
                    };
                    let target_date = match (date)().trim() {
                        "" => None,
                        text_value => match JstDate::parse(text_value) {
                            Some(value) => Some(value),
                            None => {
                                error.set(Some(text(Key::ExerciseHistoryGoalDateInvalid).to_owned()));
                                return;
                            }
                        },
                    };
                    let filter = history_api::filter_from_choice(choice.as_deref());
                    let input = goal_api::GoalInput {
                        grip: filter.grip,
                        attachment: filter.attachment,
                        target_weight_kg: target,
                        target_date,
                    };
                    let id = exercise_id.clone();
                    saving.set(true);
                    error.set(None);
                    notice.set(None);
                    spawn(async move {
                        match goal_api::save(&id, &input).await {
                            Ok(_) => {
                                notice.set(Some(text(Key::ExerciseHistoryGoalSaved).to_owned()));
                                on_changed.call(());
                            }
                            Err(_) => error.set(Some(text(Key::ExerciseHistoryGoalSaveFailed).to_owned())),
                        }
                        saving.set(false);
                    });
                },
                if (saving)() {
                    "{text(Key::ExerciseHistoryGoalSaving)}"
                } else {
                    "{text(Key::ExerciseHistoryGoalSave)}"
                }
            }
        }
    }
}

fn parse_weight(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    (value.is_finite() && value > 0.0 && value <= 1000.0).then_some(value)
}

fn goal_key(progress: &GoalProgress) -> String {
    format!(
        "{}|{}",
        progress.goal.grip.as_deref().unwrap_or_default(),
        progress.goal.attachment.as_deref().unwrap_or_default()
    )
}

fn goal_equipment(progress: &GoalProgress) -> String {
    let mut parts = Vec::new();
    if let Some(grip) = &progress.goal.grip {
        parts.push(grip_label(grip));
    }
    if let Some(attachment) = &progress.goal.attachment {
        parts.push(attachment_label(attachment));
    }
    parts.join(" / ")
}

/// 種目別の自己ベスト。
#[component]
fn HistoryBests(bests: Vec<ExerciseBest>) -> Element {
    let count = bests.len();
    let empty = bests.is_empty();
    rsx! {
        section {
            class: "flex flex-col gap-1 border-t border-line pt-3",
            "data-history-bests": "true",
            "data-history-bests-count": "{count}",
            h2 { class: "text-sm font-bold", "{text(Key::ExerciseHistoryBests)}" }
            if empty {
                p { class: "text-[13px] text-text-sub", "{text(Key::ExerciseHistoryBestsEmpty)}" }
            }
            ul { class: "flex flex-col gap-1",
                for best in bests {
                    BestRow { best }
                }
            }
        }
    }
}

#[component]
fn BestRow(best: ExerciseBest) -> Element {
    let kind = history_api::best_label(best.kind);
    let value = best_value_text(best.kind, best.value);
    let equipment = best_equipment(&best);
    let date = best.achieved_on;
    let kind_key = best.kind.as_str();
    rsx! {
        li {
            class: "flex min-h-11 flex-wrap items-baseline justify-between gap-2 border-b border-line",
            "data-history-best": "{kind_key}",
            "data-history-best-value": "{best.value}",
            "data-history-best-date": "{date}",
            "data-history-best-equipment": equipment.clone(),
            span { class: "text-[13px]", "{kind}" }
            span { class: "font-condensed text-[15px] font-semibold", "{value}" }
            if !equipment.is_empty() {
                span { class: "text-[11px] text-text-sub", "{equipment}" }
            }
            span { class: "text-[11px] text-text-sub",
                "{text(Key::ExerciseHistoryAchievedOn)} {date}"
            }
        }
    }
}

fn best_equipment(best: &ExerciseBest) -> String {
    let mut parts = Vec::new();
    if let Some(grip) = &best.grip {
        parts.push(grip_label(grip));
    }
    if let Some(attachment) = &best.attachment {
        parts.push(attachment_label(attachment));
    }
    parts.join(" / ")
}

fn best_value_text(kind: BestKind, value: f64) -> String {
    match kind {
        BestKind::LongestDuration => format_seconds(value.max(0.0).round() as u32),
        BestKind::MaxWeight | BestKind::EstimatedOneRepMax | BestKind::TotalWeight => {
            format!("{value:.1} kg")
        }
    }
}

/// 過去の記録の一覧。
#[component]
fn TrainingList(trainings: Vec<HistoryTraining>) -> Element {
    let count = trainings.len();
    let empty = trainings.is_empty();
    rsx! {
        section {
            class: "flex flex-col gap-1 border-t border-line pt-3",
            "data-history-list": "true",
            "data-history-trainings": "{count}",
            h2 { class: "text-sm font-bold", "{text(Key::ExerciseHistoryList)}" }
            if empty {
                p { class: "text-[13px] text-text-sub", "{text(Key::ExerciseHistoryEmpty)}" }
            }
            ol { class: "flex flex-col gap-3",
                for training in trainings {
                    TrainingRow { training }
                }
            }
        }
    }
}

#[component]
fn TrainingRow(training: HistoryTraining) -> Element {
    let title = training_title(training.date);
    let sets = training.sets;
    rsx! {
        li {
            class: "flex flex-col gap-1",
            "data-history-training": "{training.training_id}",
            "data-history-date": "{training.date}",
            "data-history-finished": "{training.finished_at}",
            h3 { class: "text-[13px] font-bold", "{title}" }
            ol { class: "flex flex-col gap-1",
                for (index, set) in sets.into_iter().enumerate() {
                    SetRow { index: index + 1, set }
                }
            }
        }
    }
}

#[component]
fn SetRow(index: usize, set: CalendarSet) -> Element {
    let body = set_text(&set);
    let notes = set_notes(&set).join(" / ");
    let kind = set.kind.as_str();
    rsx! {
        li {
            class: "flex flex-col gap-0.5 pl-4",
            "data-history-set": "{index}",
            "data-history-set-kind": "{kind}",
            "data-history-set-text": body.clone(),
            "data-history-set-notes": notes.clone(),
            span { class: "font-condensed text-[13px] font-semibold", "{body}" }
            span { class: "text-[11px] text-text-sub", "{notes}" }
        }
    }
}

fn training_title(date: JstDate) -> String {
    format!("{}年{}", date.year(), day_title(date))
}

#[cfg(test)]
mod tests {
    //! 履歴の見出しと自己ベストの値の文言の単体テスト。
    //!
    //! 対応する要求: `REQ-ANL-001`、`REQ-ANL-004`。
    //! 要求の受入基準は E2E が担い、ここは日付の見出しと値の整形を確かめる。

    use warrior_core::history::ExerciseBest;
    use warrior_core::jst::JstDate;
    use warrior_core::training_session::BestKind;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{best_equipment, best_value_text, training_title};

    #[wasm_bindgen_test]
    fn the_training_title_has_the_year_and_the_weekday() {
        assert_eq!(
            training_title(JstDate::new(2026, 11, 12).unwrap()),
            "2026年11月12日(木)"
        );
    }

    #[wasm_bindgen_test]
    fn the_best_values_use_the_kind_units() {
        assert_eq!(best_value_text(BestKind::MaxWeight, 100.0), "100.0 kg");
        assert_eq!(
            best_value_text(BestKind::EstimatedOneRepMax, 112.5),
            "112.5 kg"
        );
        assert_eq!(best_value_text(BestKind::TotalWeight, 1_280.0), "1280.0 kg");
        assert_eq!(best_value_text(BestKind::LongestDuration, 90.0), "1分30秒");
    }

    #[wasm_bindgen_test]
    fn the_best_equipment_joins_the_grip_and_the_attachment() {
        let grip = ExerciseBest {
            kind: BestKind::MaxWeight,
            value: 100.0,
            achieved_on: JstDate::new(2026, 11, 12).unwrap(),
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
        };
        assert_eq!(best_equipment(&grip), "順手 ミディアム");
        let attachment = ExerciseBest {
            grip: Some("supinated".to_owned()),
            attachment: Some("wide_bar".to_owned()),
            ..grip
        };
        assert_eq!(best_equipment(&attachment), "逆手 / ワイドバー");
    }
}
