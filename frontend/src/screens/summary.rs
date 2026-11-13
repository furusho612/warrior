//! 終了したトレーニングのサマリーの表示。

use dioxus::prelude::*;
use warrior_core::training_session::{PersonalBestItem, TrainingSessionSummary};

use crate::text::{text, Key};
use crate::training_session;

#[component]
pub fn SummaryScreen(training_id: String) -> Element {
    let summary = use_signal(|| Option::<TrainingSessionSummary>::None);
    let error = use_signal(|| Option::<String>::None);
    let loading = use_signal(|| true);
    let route_id = training_id.clone();

    use_effect(move || {
        let id = route_id.clone();
        spawn(async move {
            load(id, summary, error, loading).await;
        });
    });

    let retry_id = training_id.clone();
    let summary_now = summary();
    let error_now = error();
    let loading_now = loading();

    rsx! {
        section { class: "flex min-h-full flex-col gap-3 p-5",
            h1 { class: "text-lg font-bold", "{text(Key::SummaryTitle)}" }
            if let Some(summary) = summary_now {
                SummaryBody { summary }
            } else if let Some(message) = error_now {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| {
                        let id = retry_id.clone();
                        spawn(async move {
                            load(id, summary, error, loading).await;
                        });
                    },
                    "{text(Key::RecordRetry)}"
                }
            } else if loading_now {
                p { class: "text-[13px] text-text-sub", "{text(Key::SummaryLoading)}" }
            }
        }
    }
}

async fn load(
    training_id: String,
    mut summary: Signal<Option<TrainingSessionSummary>>,
    mut error: Signal<Option<String>>,
    mut loading: Signal<bool>,
) {
    loading.set(true);
    error.set(None);
    let result = training_session::summary(&training_id).await;
    loading.set(false);
    match result {
        Ok(value) => {
            summary.set(Some(value));
            error.set(None);
        }
        Err(_) => error.set(Some(text(Key::SummaryLoadFailed).to_owned())),
    }
}

#[component]
fn SummaryBody(summary: TrainingSessionSummary) -> Element {
    let personal_bests = summary.personal_bests.clone();
    rsx! {
        div {
            class: "flex flex-col gap-3",
            "data-summary": "true",
            "data-duration-seconds": "{summary.duration_seconds}",
            "data-total-weight-kg": "{summary.total_weight_kg}",
            "data-exercise-count": "{summary.exercise_count}",
            "data-set-count": "{summary.set_count}",
            SummaryRow {
                label: text(Key::SummaryDuration).to_owned(),
                value: format_duration(summary.duration_seconds),
            }
            SummaryRow {
                label: text(Key::SummaryTotalWeight).to_owned(),
                value: format!("{} kg", summary.total_weight_kg),
            }
            SummaryRow {
                label: text(Key::SummaryExerciseCount).to_owned(),
                value: format!("{}", summary.exercise_count),
            }
            SummaryRow {
                label: text(Key::SummarySetCount).to_owned(),
                value: format!("{}", summary.set_count),
            }
            h2 { class: "text-sm font-bold", "{text(Key::SummaryPersonalBests)}" }
            if personal_bests.is_empty() {
                p { class: "text-[13px] text-text-sub", "{text(Key::SummaryNoPersonalBests)}" }
            }
            ul { class: "flex flex-col gap-1",
                for item in personal_bests {
                    PersonalBestRow { item }
                }
            }
        }
    }
}

#[component]
fn SummaryRow(label: String, value: String) -> Element {
    rsx! {
        div { class: "flex min-h-11 items-center justify-between border-b border-line",
            span { class: "text-[13px] text-text-sub", "{label}" }
            span { class: "text-[15px] font-bold", "{value}" }
        }
    }
}

#[component]
fn PersonalBestRow(item: PersonalBestItem) -> Element {
    rsx! {
        li {
            class: "flex min-h-11 items-center justify-between border-b border-line",
            "data-personal-best": "{item.kind}",
            "data-personal-best-value": "{item.value}",
            span { class: "text-[13px]", "{best_label(&item.kind)}" }
            span { class: "text-[15px] font-bold", "{best_value(&item)}" }
        }
    }
}

fn best_label(kind: &str) -> String {
    let label = match kind {
        "max_weight" => text(Key::SummaryBestMaxWeight),
        "estimated_one_rep_max" => text(Key::SummaryBestEstimatedOneRepMax),
        "total_weight" => text(Key::SummaryBestTotalWeight),
        "longest_duration" => text(Key::SummaryBestLongestDuration),
        other => other,
    };
    label.to_owned()
}

fn best_value(item: &PersonalBestItem) -> String {
    if item.kind == "longest_duration" {
        format_duration(item.value as i64)
    } else {
        format!("{} kg", item.value)
    }
}

fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let remaining = seconds % 60;
    if hours > 0 {
        format!("{hours}時間{minutes}分{remaining}秒")
    } else if minutes > 0 {
        format!("{minutes}分{remaining}秒")
    } else {
        format!("{remaining}秒")
    }
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{best_label, format_duration};

    #[wasm_bindgen_test]
    fn format_duration_uses_hours_minutes_and_seconds() {
        assert_eq!(format_duration(0), "0秒");
        assert_eq!(format_duration(59), "59秒");
        assert_eq!(format_duration(60), "1分0秒");
        assert_eq!(format_duration(3_600), "1時間0分0秒");
        assert_eq!(format_duration(3_661), "1時間1分1秒");
        assert_eq!(format_duration(-1), "0秒");
    }

    #[wasm_bindgen_test]
    fn every_personal_best_kind_has_a_label() {
        for kind in [
            "max_weight",
            "estimated_one_rep_max",
            "total_weight",
            "longest_duration",
        ] {
            assert_ne!(best_label(kind), kind);
        }
        assert_eq!(best_label("unknown_kind"), "unknown_kind");
    }
}
