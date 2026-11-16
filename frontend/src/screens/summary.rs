//! 終了したトレーニングのサマリーの表示。

use dioxus::prelude::*;
use dioxus_router::Link;
use warrior_core::training_session::{BestKind, PersonalBestItem, TrainingSessionSummary};

use crate::ai_notice::AiPrivacyNotice;
use crate::grip_text::{attachment_label, grip_label};
use crate::routes::Route;
use crate::screens::evaluations::DailyEvaluationPanel;
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
            DailyEvaluationPanel { date: summary.date.to_string() }
            Link {
                to: Route::Evaluations {},
                class: "flex min-h-11 items-center justify-center border border-line px-3 text-[13px] text-primary",
                "data-evaluations-link": "true",
                "{text(Key::EvaluationsLink)}"
            }
            AiPrivacyNotice {}
            SummaryRow {
                label: text(Key::SummaryDuration).to_owned(),
                value: format_duration(summary.duration_seconds),
            }
            SummaryRow {
                label: text(Key::SummaryTotalWeight).to_owned(),
                value: format!("{:.1} kg", summary.total_weight_kg),
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
    let label = best_label(item.kind);
    let value = best_value(&item);
    let name = item.exercise_name.clone();
    let equipment = best_equipment(&item);
    rsx! {
        li {
            class: "flex min-h-11 items-center gap-2 border-b border-line",
            "data-personal-best": "{item.kind.as_str()}",
            "data-personal-best-value": "{item.value}",
            if let Some(name) = name {
                span {
                    class: "min-w-0 flex-1 truncate text-[13px]",
                    "data-personal-best-name": "true",
                    "{name}"
                }
            } else {
                span { class: "min-w-0 flex-1" }
            }
            if let Some(equipment) = equipment {
                span {
                    class: "shrink-0 text-[10px] text-text-sub",
                    "data-personal-best-equipment": "true",
                    "{equipment}"
                }
            }
            span {
                class: "shrink-0 text-[11px] text-text-sub",
                "data-personal-best-kind": "true",
                "{label}"
            }
            span {
                class: "shrink-0 text-right text-[15px] font-bold",
                "data-personal-best-amount": "true",
                "{value}"
            }
        }
    }
}

pub(crate) fn best_label(kind: BestKind) -> &'static str {
    match kind {
        BestKind::MaxWeight => text(Key::SummaryBestMaxWeight),
        BestKind::EstimatedOneRepMax => text(Key::SummaryBestEstimatedOneRepMax),
        BestKind::TotalWeight => text(Key::SummaryBestTotalWeight),
        BestKind::LongestDuration => text(Key::SummaryBestLongestDuration),
    }
}

pub(crate) fn best_value(item: &PersonalBestItem) -> String {
    match item.kind {
        BestKind::LongestDuration => format_duration(item.value as i64),
        BestKind::EstimatedOneRepMax | BestKind::TotalWeight => {
            format!("{:.1} kg", item.value)
        }
        BestKind::MaxWeight => format!("{} kg", item.value),
    }
}

/// 自己ベストのグリップとアタッチメントの表示。
pub(crate) fn best_equipment(item: &PersonalBestItem) -> Option<String> {
    let grip = item.grip.as_deref().map(grip_label);
    let attachment = item.attachment.as_deref().map(attachment_label);
    match (grip, attachment) {
        (Some(grip), Some(attachment)) => Some(format!("{grip} / {attachment}")),
        (Some(grip), None) => Some(grip),
        (None, Some(attachment)) => Some(attachment),
        (None, None) => None,
    }
}

pub(crate) fn format_duration(seconds: i64) -> String {
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
    use warrior_core::exercise::Laterality;
    use warrior_core::set_calc::RecordMode;
    use warrior_core::training_session::{BestKind, PersonalBestItem};
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{best_equipment, best_label, best_value, format_duration};

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
        assert_eq!(best_label(BestKind::MaxWeight), "最大重量");
        assert_eq!(best_label(BestKind::EstimatedOneRepMax), "推定 1RM");
        assert_eq!(best_label(BestKind::TotalWeight), "種目別の総重量");
        assert_eq!(best_label(BestKind::LongestDuration), "最長時間");
    }

    #[wasm_bindgen_test]
    fn estimated_max_and_total_weight_are_rounded_to_one_decimal() {
        assert_eq!(best_value(&item(BestKind::MaxWeight, 100.0)), "100 kg");
        assert_eq!(
            best_value(&item(BestKind::EstimatedOneRepMax, 116.58)),
            "116.6 kg"
        );
        assert_eq!(
            best_value(&item(BestKind::TotalWeight, 1_640.0)),
            "1640.0 kg"
        );
        assert_eq!(
            best_value(&item(BestKind::LongestDuration, 90.0)),
            "1分30秒"
        );
    }

    #[wasm_bindgen_test]
    fn personal_bests_show_the_grip_and_the_attachment() {
        let mut grip = item(BestKind::MaxWeight, 100.0);
        grip.grip = Some("pronated_medium".to_owned());
        assert_eq!(best_equipment(&grip).as_deref(), Some("順手 ミディアム"));

        let mut attachment = item(BestKind::MaxWeight, 80.0);
        attachment.grip = Some("pronated".to_owned());
        attachment.attachment = Some("wide_bar".to_owned());
        assert_eq!(
            best_equipment(&attachment).as_deref(),
            Some("順手 / ワイドバー")
        );

        assert_eq!(best_equipment(&item(BestKind::TotalWeight, 500.0)), None);
    }

    fn item(kind: BestKind, value: f64) -> PersonalBestItem {
        PersonalBestItem {
            exercise_id: "bench-press".to_owned(),
            exercise_name: Some("ベンチプレス".to_owned()),
            record_mode: RecordMode::WeightAndReps,
            laterality: Laterality::Bilateral,
            grip: None,
            attachment: None,
            kind,
            value,
        }
    }
}
