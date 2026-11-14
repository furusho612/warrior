//! 記録の種目の一覧、追加、並べ替え、削除と、前回の記録の表示、コピー、差。
//!
//! 種目の行と入力は端末内の IndexedDB に保存し、送信待ちの列を通す (ADR-002)。
//! セットの入力は種目ごとの最小の欄であり、重量、回数、時間だけを持つ。
//! 前回の選択と差の計算は `warrior_core::previous` の純粋な関数を使う。

use dioxus::prelude::*;
use warrior_core::exercise::{ExerciseSnapshot, RecordMode};
use warrior_core::previous::{
    exercise_difference, total_weight_difference, DurationDifference, ExerciseDifference,
    PreviousExercise, PreviousSet, PreviousTraining, TotalWeightDifference,
};
use warrior_core::set_calc::{BestKey, SetKind, SetRecord, SetSegment};
use warrior_core::training_record::TrainingExerciseRecord;

use super::RecordSignals;
use crate::previous as previous_api;
use crate::routes::navigate;
use crate::storage;
use crate::sync;
use crate::text::{text, Key};
use crate::training_exercise;

/// 記録の種目の一覧と、選んだ種目の詳細。
#[component]
pub(super) fn ExerciseSection(training_id: String, signals: RecordSignals) -> Element {
    let views = use_signal(Vec::<ExerciseView>::new);
    let mut selected = use_signal(|| Option::<String>::None);
    let previous_training = use_signal(|| Option::<PreviousTraining>::None);
    let error = use_signal(|| Option::<String>::None);
    let loading = use_signal(|| true);
    let mut reload = use_signal(|| 0u32);
    let mut last_tree = use_signal(|| 0u64);
    let reload_effect = reload;
    let tree_revision = signals.tree_revision;
    let route_id = training_id.clone();
    use_effect(move || {
        let _ = (reload_effect)();
        let tree = (tree_revision)();
        let preserve_inputs = tree == *last_tree.peek();
        last_tree.set(tree);
        let id = route_id.clone();
        spawn(load(
            id,
            views,
            selected,
            previous_training,
            error,
            loading,
            preserve_inputs,
        ));
    });

    let on_input = EventHandler::new(
        move |(view_id, index, field, value): (String, usize, Field, String)| {
            update_input(views, &view_id, index, field, value);
        },
    );
    let on_copy = EventHandler::new(move |view_id: String| {
        copy_previous(views, &view_id);
    });
    let delete_training_id = training_id.clone();
    let on_delete = EventHandler::new(move |view_id: String| {
        let operation = match training_exercise::delete(&delete_training_id, &view_id) {
            Ok(operation) => operation,
            Err(_) => {
                set_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
        };
        let training_id = delete_training_id.clone();
        spawn(async move {
            let result = match storage::current().await {
                Ok(storage) => {
                    storage
                        .save_training_exercise_delete(&training_id, &view_id, &operation)
                        .await
                }
                Err(error) => Err(error),
            };
            if result.is_err() {
                set_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
            sync::flush_in_background();
            reload.set((reload)() + 1);
        });
    });
    let move_training_id = training_id.clone();
    let on_move = EventHandler::new(move |(view_id, delta): (String, i32)| {
        let mut list = (views)();
        let Some(index) = list.iter().position(|view| view.record.id == view_id) else {
            return;
        };
        let target = index as i32 + delta;
        if target < 0 || target as usize >= list.len() {
            return;
        }
        list.swap(index, target as usize);
        let ordered = list
            .iter()
            .map(|view| view.record.id.clone())
            .collect::<Vec<_>>();
        let operation = match training_exercise::reorder(&move_training_id, &ordered) {
            Ok(operation) => operation,
            Err(_) => {
                set_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
        };
        let training_id = move_training_id.clone();
        spawn(async move {
            let result = match storage::current().await {
                Ok(storage) => {
                    storage
                        .save_training_exercise_reorder(&training_id, &ordered, &operation)
                        .await
                }
                Err(error) => Err(error),
            };
            if result.is_err() {
                set_error(error, Key::RecordExerciseSaveFailed);
                return;
            }
            sync::flush_in_background();
            reload.set((reload)() + 1);
        });
    });
    let add_training_id = training_id.clone();
    let on_add = EventHandler::new(move |_: ()| {
        navigate(&format!("/record/{add_training_id}/exercises"));
    });

    let current_views = (views)();
    let selected_id = (selected)();
    let selected_view = selected_id
        .as_ref()
        .and_then(|id| current_views.iter().find(|view| &view.record.id == id))
        .cloned();
    let view_only = (signals.view_only)();
    let training_diff = training_difference(&current_views, (previous_training)().as_ref());
    let load_error = (error)();
    let is_loading = (loading)();

    rsx! {
        section {
            class: "flex flex-col gap-2",
            "data-exercises": "true",
            div { class: "flex items-center justify-between gap-2",
                h2 { class: "text-sm font-bold", "{text(Key::RecordRail)}" }
                button {
                    class: "min-h-11 border border-primary px-3 text-[13px] font-bold text-primary",
                    "data-add-exercise": "true",
                    disabled: view_only,
                    onclick: move |_| on_add.call(()),
                    "{text(Key::RecordExerciseAdd)}"
                }
            }
            if let Some(message) = load_error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| reload.set((reload)() + 1),
                    "{text(Key::RecordRetry)}"
                }
            }
            if current_views.is_empty() && !is_loading {
                p { class: "text-[13px] text-text-sub", "{text(Key::RecordExerciseEmpty)}" }
            }
            div { class: "flex flex-col gap-1", "data-exercise-list": "true",
                for (index, view) in current_views.iter().enumerate() {
                    ExerciseRow {
                        key: "{view.record.id}",
                        view: view.clone(),
                        selected: selected_id.as_deref() == Some(view.record.id.as_str()),
                        first: index == 0,
                        last: index + 1 == current_views.len(),
                        disabled: view_only,
                        on_select: {
                            let id = view.record.id.clone();
                            move |_| selected.set(Some(id.clone()))
                        },
                        on_up: {
                            let id = view.record.id.clone();
                            move |_| on_move.call((id.clone(), -1))
                        },
                        on_down: {
                            let id = view.record.id.clone();
                            move |_| on_move.call((id.clone(), 1))
                        },
                        on_delete: {
                            let id = view.record.id.clone();
                            move |_| on_delete.call(id.clone())
                        },
                    }
                }
            }
            if let Some(view) = selected_view {
                ExerciseDetail {
                    view,
                    disabled: view_only,
                    on_input,
                    on_copy,
                }
            } else {
                EmptySetTable {}
            }
            if let Some(diff) = training_diff {
                TrainingDifference { diff }
            }
        }
    }
}

/// 画面が読む 1 つの種目。
#[derive(Clone, PartialEq)]
struct ExerciseView {
    record: TrainingExerciseRecord,
    snapshot: ExerciseSnapshot,
    previous: Option<PreviousExercise>,
    previous_error: bool,
    inputs: Vec<SetInput>,
}

/// セットの入力欄の 1 行。
#[derive(Clone, PartialEq)]
struct SetInput {
    kind: SetKind,
    weight: String,
    reps: String,
    duration: String,
    grip: Option<String>,
}

impl Default for SetInput {
    fn default() -> Self {
        Self {
            kind: SetKind::Normal,
            weight: String::new(),
            reps: String::new(),
            duration: String::new(),
            grip: None,
        }
    }
}

impl SetInput {
    fn from_previous(set: &PreviousSet) -> Self {
        let segment = set.segments.first();
        Self {
            kind: set.kind,
            weight: segment.map_or_else(String::new, |segment| format_number(segment.weight_kg)),
            reps: segment.map_or_else(String::new, |segment| segment.reps.to_string()),
            duration: segment.map_or_else(String::new, |segment| {
                segment.longest_duration_seconds().to_string()
            }),
            grip: set.grip.clone(),
        }
    }

    fn to_record(&self, exercise_id: &str, mode: RecordMode) -> Option<SetRecord> {
        let key = BestKey {
            exercise_id: exercise_id.to_owned(),
            grip: None,
            attachment: None,
        };
        match mode {
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                let weight = self.weight.trim().parse::<f64>().ok()?;
                let reps = self.reps.trim().parse::<u32>().ok()?;
                if weight < 0.0 || !weight.is_finite() || reps == 0 {
                    return None;
                }
                SetRecord::new(
                    key,
                    self.kind,
                    vec![SetSegment {
                        weight_kg: weight,
                        reps,
                        momentum_reps: 0,
                        assisted_reps: 0,
                        left: None,
                        right: None,
                        duration_seconds: None,
                    }],
                    None,
                    None,
                    mode,
                )
                .ok()
            }
            RecordMode::Time => {
                let duration = self.duration.trim().parse::<u32>().ok()?;
                if duration == 0 {
                    return None;
                }
                SetRecord::new(key, self.kind, Vec::new(), None, Some(duration), mode).ok()
            }
        }
    }
}

/// 入力欄の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Weight,
    Reps,
    Duration,
}

/// 今回の入力から作った種目の合計。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CurrentTotals {
    total_weight_kg: f64,
    total_duration_seconds: u32,
    longest_duration_seconds: u32,
}

fn current_totals(view: &ExerciseView) -> CurrentTotals {
    let mut totals = CurrentTotals::default();
    for input in &view.inputs {
        let Some(record) = input.to_record(&view.snapshot.exercise_id, view.snapshot.record_mode)
        else {
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

fn exercise_diff(view: &ExerciseView) -> Option<ExerciseDifference> {
    let totals = current_totals(view);
    exercise_difference(
        view.snapshot.record_mode,
        totals.total_weight_kg,
        totals.total_duration_seconds,
        totals.longest_duration_seconds,
        view.previous.as_ref(),
    )
}

fn training_difference(
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

fn update_input(
    mut views: Signal<Vec<ExerciseView>>,
    view_id: &str,
    index: usize,
    field: Field,
    value: String,
) {
    let mut list = (views)();
    let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) else {
        return;
    };
    let Some(input) = view.inputs.get_mut(index) else {
        return;
    };
    match field {
        Field::Weight => input.weight = value,
        Field::Reps => input.reps = value,
        Field::Duration => input.duration = value,
    }
    views.set(list);
}

fn copy_previous(mut views: Signal<Vec<ExerciseView>>, view_id: &str) {
    let mut list = (views)();
    let Some(view) = list.iter_mut().find(|view| view.record.id == view_id) else {
        return;
    };
    let Some(previous) = &view.previous else {
        return;
    };
    view.inputs = previous.sets.iter().map(SetInput::from_previous).collect();
    views.set(list);
}

fn set_error(mut error: Signal<Option<String>>, key: Key) {
    error.set(Some(text(key).to_owned()));
}

async fn load(
    training_id: String,
    mut views: Signal<Vec<ExerciseView>>,
    mut selected: Signal<Option<String>>,
    mut previous_training: Signal<Option<PreviousTraining>>,
    mut error: Signal<Option<String>>,
    mut loading: Signal<bool>,
    preserve_inputs: bool,
) {
    loading.set(true);
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            set_error(error, Key::RecordExerciseLoadFailed);
            loading.set(false);
            return;
        }
    };
    let records = match storage.training_exercises(&training_id).await {
        Ok(records) => records,
        Err(_) => {
            set_error(error, Key::RecordExerciseLoadFailed);
            loading.set(false);
            return;
        }
    };
    let old = (views)();
    let mut next = Vec::with_capacity(records.len());
    let mut training_previous = None;
    for record in records {
        let Ok(snapshot) = serde_json::from_str::<ExerciseSnapshot>(&record.snapshot) else {
            continue;
        };
        let (previous, previous_error) =
            match previous_api::fetch(&training_id, &snapshot.exercise_id, None, None).await {
                Ok(response) => {
                    if training_previous.is_none() {
                        training_previous = response.training.clone();
                    }
                    (response.exercise, false)
                }
                Err(_) => (None, true),
            };
        let inputs = if preserve_inputs {
            old.iter()
                .find(|view| view.record.id == record.id)
                .map_or_else(|| vec![SetInput::default()], |view| view.inputs.clone())
        } else {
            vec![SetInput::default()]
        };
        next.push(ExerciseView {
            record,
            snapshot,
            previous,
            previous_error,
            inputs,
        });
    }
    if selected
        .peek()
        .as_ref()
        .is_none_or(|id| !next.iter().any(|view| &view.record.id == id))
    {
        selected.set(next.first().map(|view| view.record.id.clone()));
    }
    previous_training.set(training_previous);
    views.set(next);
    error.set(None);
    loading.set(false);
}

#[component]
fn ExerciseRow(
    view: ExerciseView,
    selected: bool,
    first: bool,
    last: bool,
    disabled: bool,
    on_select: EventHandler<()>,
    on_up: EventHandler<()>,
    on_down: EventHandler<()>,
    on_delete: EventHandler<()>,
) -> Element {
    let class = if selected {
        "flex min-h-11 items-center justify-between gap-2 border border-primary bg-surface px-2"
    } else {
        "flex min-h-11 items-center justify-between gap-2 border border-line bg-surface px-2"
    };
    rsx! {
        div {
            class,
            "data-exercise-row": "true",
            "data-training-exercise-id": "{view.record.id}",
            "data-exercise-id": "{view.snapshot.exercise_id}",
            "data-selected": if selected { "true" } else { "false" },
            button {
                class: "min-h-11 min-w-0 flex-1 truncate text-left text-[13px] font-bold",
                onclick: move |_| on_select.call(()),
                "{view.snapshot.name}"
            }
            div { class: "flex shrink-0 gap-1",
                button {
                    class: "min-h-11 min-w-11 border border-line px-1 text-[11px]",
                    "data-move-up": "true",
                    disabled: disabled || first,
                    onclick: move |_| on_up.call(()),
                    "{text(Key::RecordExerciseMoveUp)}"
                }
                button {
                    class: "min-h-11 min-w-11 border border-line px-1 text-[11px]",
                    "data-move-down": "true",
                    disabled: disabled || last,
                    onclick: move |_| on_down.call(()),
                    "{text(Key::RecordExerciseMoveDown)}"
                }
                button {
                    class: "min-h-11 min-w-11 border border-line px-1 text-[11px] text-primary",
                    "data-delete-exercise": "true",
                    disabled,
                    onclick: move |_| on_delete.call(()),
                    "{text(Key::RecordExerciseDelete)}"
                }
            }
        }
    }
}

#[component]
fn ExerciseDetail(
    view: ExerciseView,
    disabled: bool,
    on_input: EventHandler<(String, usize, Field, String)>,
    on_copy: EventHandler<String>,
) -> Element {
    let diff = exercise_diff(&view);
    let view_id = view.record.id.clone();
    let copy_id = view.record.id.clone();
    let previous = view.previous.clone();
    let previous_error = view.previous_error;
    let is_time = view.snapshot.record_mode == RecordMode::Time;
    let rows = view.inputs.clone();

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-2",
            "data-exercise-detail": "true",
            "data-exercise-id": "{view.snapshot.exercise_id}",
            div { class: "flex items-center justify-between gap-2",
                h3 { class: "text-[13px] font-bold", "{view.snapshot.name}" }
                if previous.is_some() {
                    button {
                        class: "min-h-11 border border-primary px-2 text-[11px] font-bold text-primary",
                        "data-copy-previous": "true",
                        disabled,
                        onclick: move |_| on_copy.call(copy_id.clone()),
                        "{text(Key::RecordExerciseCopy)}"
                    }
                }
            }
            if let Some(previous) = previous {
                PreviousRecord { previous }
            } else if previous_error {
                p { class: "text-[11px] text-text-sub", "{text(Key::RecordExercisePreviousFailed)}" }
            }
            if let Some(diff) = diff {
                ExerciseDifferenceView { diff }
            }
            SetTable {
                view_id,
                rows,
                is_time,
                disabled,
                on_input,
            }
        }
    }
}

#[component]
fn PreviousRecord(previous: PreviousExercise) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-line p-2",
            "data-previous": "true",
            "data-previous-total-weight-kg": "{previous.total_weight_kg}",
            "data-previous-total-duration-seconds": "{previous.total_duration_seconds}",
            "data-previous-max-duration-seconds": "{previous.max_duration_seconds}",
            h4 { class: "text-[11px] font-bold", "{text(Key::RecordExercisePrevious)}" }
            for (index, set) in previous.sets.iter().enumerate() {
                div {
                    class: "text-[11px] leading-5",
                    "data-previous-set": "true",
                    "data-set-index": "{index}",
                    "data-set-kind": "{set.kind.as_str()}",
                    "{previous_set_text(index, set)}"
                }
            }
        }
    }
}

#[component]
fn ExerciseDifferenceView(diff: ExerciseDifference) -> Element {
    match diff {
        ExerciseDifference::Weight(diff) => rsx! {
            div {
                class: "text-[13px] font-bold",
                "data-exercise-weight-diff": "{diff.difference_kg}",
                "{text(Key::RecordExerciseWeightDiff)}: {signed_weight(diff.difference_kg)}"
            }
        },
        ExerciseDifference::Time(diff) => rsx! {
            div {
                class: "flex flex-col gap-1 text-[13px] font-bold",
                "data-exercise-total-duration-diff": "{diff.total_difference_seconds}",
                "data-exercise-longest-duration-diff": "{diff.longest_difference_seconds}",
                div { "{text(Key::RecordExerciseTotalDurationDiff)}: {signed_duration(diff)}" }
                div { "{text(Key::RecordExerciseLongestDurationDiff)}: {signed_longest(diff)}" }
            }
        },
    }
}

#[component]
fn TrainingDifference(diff: TotalWeightDifference) -> Element {
    rsx! {
        div {
            class: "border border-line bg-surface p-2 text-[13px] font-bold",
            "data-training-total-diff": "{diff.difference_kg}",
            "{text(Key::RecordTrainingWeightDiff)}: {signed_weight(diff.difference_kg)}"
        }
    }
}

#[component]
fn SetTable(
    view_id: String,
    rows: Vec<SetInput>,
    is_time: bool,
    disabled: bool,
    on_input: EventHandler<(String, usize, Field, String)>,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-1",
            h4 { class: "text-[11px] font-bold", "{text(Key::RecordSets)}" }
            div {
                class: "h-[222px] overflow-y-auto border border-line bg-surface",
                "data-set-table": "true",
                "data-set-rows": "{rows.len()}",
                "data-set-row-height": "44",
                for (index, row) in rows.iter().enumerate() {
                    div {
                        class: "flex h-11 items-center gap-1 border-b border-line px-2",
                        "data-set-row": "true",
                        "data-set-row-index": "{index}",
                        span { class: "w-5 shrink-0 text-[11px] text-text-sub", "{index + 1}" }
                        if is_time {
                            input {
                                class: "min-h-9 w-full border border-line bg-bg px-2 text-[13px]",
                                r#type: "number",
                                inputmode: "numeric",
                                min: "1",
                                step: "1",
                                placeholder: "{text(Key::RecordExerciseDuration)}",
                                "data-set-duration": "true",
                                value: "{row.duration}",
                                disabled,
                                oninput: {
                                    let view_id = view_id.clone();
                                    move |event| on_input.call((view_id.clone(), index, Field::Duration, event.value()))
                                },
                            }
                        } else {
                            input {
                                class: "min-h-9 w-1/2 border border-line bg-bg px-2 text-[13px]",
                                r#type: "number",
                                inputmode: "decimal",
                                min: "0",
                                step: "0.5",
                                placeholder: "{text(Key::RecordExerciseWeight)}",
                                "data-set-weight": "true",
                                value: "{row.weight}",
                                disabled,
                                oninput: {
                                    let view_id = view_id.clone();
                                    move |event| on_input.call((view_id.clone(), index, Field::Weight, event.value()))
                                },
                            }
                            input {
                                class: "min-h-9 w-1/2 border border-line bg-bg px-2 text-[13px]",
                                r#type: "number",
                                inputmode: "numeric",
                                min: "1",
                                step: "1",
                                placeholder: "{text(Key::RecordExerciseReps)}",
                                "data-set-reps": "true",
                                value: "{row.reps}",
                                disabled,
                                oninput: {
                                    let view_id = view_id.clone();
                                    move |event| on_input.call((view_id.clone(), index, Field::Reps, event.value()))
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptySetTable() -> Element {
    rsx! {
        div { class: "flex flex-col gap-1",
            h2 { class: "text-sm font-bold", "{text(Key::RecordSets)}" }
            div {
                class: "h-[222px] overflow-y-auto border border-line bg-surface",
                "data-set-table": "true",
                "data-set-rows": "5",
                "data-set-row-height": "44",
                for index in 0..5 {
                    div {
                        class: "h-11 border-b border-line",
                        "data-set-row": "true",
                        "data-set-row-index": "{index}",
                    }
                }
            }
        }
    }
}

fn previous_set_text(index: usize, set: &PreviousSet) -> String {
    let segment = set.segments.first();
    let value = match segment {
        Some(segment) => {
            let duration = segment.longest_duration_seconds();
            if duration > 0 && segment.weight_kg == 0.0 && segment.reps == 0 {
                format!("{duration}{}", text(Key::CalendarSeconds))
            } else {
                format!("{} kg x {}", format_number(segment.weight_kg), segment.reps)
            }
        }
        None => String::new(),
    };
    format!("{} {} ({})", index + 1, value, set_kind_text(set.kind))
}

fn set_kind_text(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Normal => text(Key::CalendarSetKindNormal),
        SetKind::Warmup => text(Key::CalendarSetKindWarmup),
        SetKind::Drop => text(Key::CalendarSetKindDrop),
    }
}

fn signed_weight(value: f64) -> String {
    if value > 0.0 {
        format!("+{value:.1} kg")
    } else if value < 0.0 {
        format!("{value:.1} kg")
    } else {
        "0 kg".to_owned()
    }
}

fn signed_duration(diff: DurationDifference) -> String {
    signed_seconds(diff.total_difference_seconds)
}

fn signed_longest(diff: DurationDifference) -> String {
    signed_seconds(diff.longest_difference_seconds)
}

fn signed_seconds(value: i64) -> String {
    if value > 0 {
        format!("+{value} {}", text(Key::CalendarSeconds))
    } else if value < 0 {
        format!("{value} {}", text(Key::CalendarSeconds))
    } else {
        format!("0 {}", text(Key::CalendarSeconds))
    }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}
