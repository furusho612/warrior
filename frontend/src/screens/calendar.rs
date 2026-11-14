//! カレンダーの月表示、部位の色分け、日数の集計、日の詳細、絞り込み、凡例。

use dioxus::prelude::*;
use warrior_core::calendar::{
    next_month, previous_month, CalendarDayDetail, CalendarExercise, CalendarMonth, CalendarSet,
};
use warrior_core::exercise::BodyPart;
use warrior_core::jst::{JstDate, JstMonth, UtcInstant};

use crate::calendar as calendar_api;
use crate::calendar_view::{
    build_cells, day_aria_label, day_cell_class, day_number_class, day_parts, day_title,
    format_weight, part_button_class, part_color_class, part_counts, part_label, set_notes,
    set_text, weekdays, CalendarCell,
};
use crate::text::{text, Key};

#[component]
pub fn CalendarScreen() -> Element {
    let today = current_date();
    let mut month = use_signal(|| today.map(|date| date.month_range()));
    let mut part = use_signal(|| Option::<BodyPart>::None);
    let mut selected = use_signal(|| Option::<JstDate>::None);
    let mut month_data = use_signal(|| Option::<CalendarMonth>::None);
    let mut day_data = use_signal(|| Option::<CalendarDayDetail>::None);
    let month_error = use_signal(|| Option::<String>::None);
    let mut day_error = use_signal(|| Option::<String>::None);
    let mut month_reload = use_signal(|| 0u32);
    let mut day_reload = use_signal(|| 0u32);

    use_effect(move || {
        let _ = month_reload();
        let Some(current) = month() else {
            return;
        };
        let part = part();
        spawn(async move {
            load_month(current, part, month_data, month_error).await;
        });
    });

    use_effect(move || {
        let _ = day_reload();
        let Some(date) = selected() else {
            return;
        };
        spawn(async move {
            load_day(date, day_data, day_error).await;
        });
    });

    let on_select = EventHandler::new(move |date: JstDate| {
        selected.set(Some(date));
        day_data.set(None);
        day_error.set(None);
    });
    let on_part = EventHandler::new(move |value: Option<BodyPart>| {
        part.set(value);
    });
    let on_previous = EventHandler::new(move |_: ()| {
        let Some(current) = month() else {
            return;
        };
        let Some(previous) = previous_month(current) else {
            return;
        };
        month.set(Some(previous));
        month_data.set(None);
        selected.set(None);
        day_data.set(None);
    });
    let on_next = EventHandler::new(move |_: ()| {
        let Some(current) = month() else {
            return;
        };
        let Some(next) = next_month(current) else {
            return;
        };
        month.set(Some(next));
        month_data.set(None);
        selected.set(None);
        day_data.set(None);
    });
    let on_month_retry = EventHandler::new(move |_: ()| {
        month_reload += 1;
    });
    let on_day_retry = EventHandler::new(move |_: ()| {
        day_reload += 1;
    });

    let month_now = month();
    let part_now = part();
    let view = month_data();
    let detail = day_data();
    let loading = month_now.is_some() && view.is_none() && month_error().is_none();
    let (year, month_number) = month_now
        .map(|current| (current.start().year(), current.start().month()))
        .unwrap_or_default();
    let cells = build_cells(month_now, view.as_ref(), selected(), today);
    let counts = part_counts(view.as_ref());
    let training_days = view.as_ref().map(|view| view.training_days).unwrap_or(0);
    let detail_loading = selected().is_some() && detail.is_none() && day_error().is_none();
    let month_attr = month_now.map(month_text).unwrap_or_default();
    let part_attr = part_now.map(BodyPart::as_str).unwrap_or_default();

    rsx! {
        div {
            class: "flex min-h-full",
            "data-calendar": "true",
            "data-calendar-month": month_attr,
            "data-calendar-selected-part": part_attr,
            PartRail {
                counts: counts,
                selected: part_now,
                training_days: training_days,
                on_select: on_part,
            }
            section { class: "min-w-0 flex-1 px-3.5",
                div { class: "flex h-12 items-center justify-between",
                    h1 { class: "text-[17px] font-black", "{text(Key::CalendarTitle)}" }
                }
                div { class: "flex h-11 items-center justify-between",
                    button {
                        class: "flex h-11 w-11 items-center justify-center border border-line",
                        "data-calendar-previous": "true",
                        "aria-label": text(Key::CalendarPreviousMonth),
                        onclick: move |_| on_previous.call(()),
                        svg {
                            class: "h-5 w-5",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M15 5l-7 7 7 7" }
                        }
                    }
                    div { class: "flex items-baseline gap-3",
                        span { class: "text-sm font-bold",
                            span { class: "font-condensed text-[26px] font-semibold", "{year}" }
                            " 年 "
                            span { class: "font-condensed text-[26px] font-semibold", "{month_number}" }
                            " 月"
                        }
                        span { class: "text-xs text-text-sub",
                            "{text(Key::CalendarTrainingDays)} "
                            strong {
                                class: "font-condensed text-xl font-semibold text-text",
                                "data-calendar-training-days": "{training_days}",
                                "{training_days}"
                            }
                            " {text(Key::CalendarDaysUnit)}"
                        }
                    }
                    button {
                        class: "flex h-11 w-11 items-center justify-center border border-line",
                        "data-calendar-next": "true",
                        "aria-label": text(Key::CalendarNextMonth),
                        onclick: move |_| on_next.call(()),
                        svg {
                            class: "h-5 w-5",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M9 5l7 7-7 7" }
                        }
                    }
                }
                div {
                    class: "mt-3 grid grid-cols-7 text-center text-[11px] leading-5 text-text-sub",
                    "aria-hidden": "true",
                    for weekday in weekdays() {
                        span { "{weekday}" }
                    }
                }
                div { class: "grid grid-cols-7",
                    for cell in cells {
                        if let Some(cell) = cell {
                            CalendarDayCell { cell: cell, on_select: on_select }
                        } else {
                            div { class: "h-[50px] border-t border-line", "aria-hidden": "true" }
                        }
                    }
                }
                DayDetail {
                    detail: detail,
                    selected: selected(),
                    error: day_error(),
                    loading: detail_loading,
                    on_retry: on_day_retry,
                }
                if loading {
                    p { class: "mt-2 text-[13px] text-text-sub", "{text(Key::CalendarLoading)}" }
                }
                if let Some(message) = month_error() {
                    p { class: "mt-2 text-[13px] text-primary", "{message}" }
                    button {
                        class: "mt-1 min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| on_month_retry.call(()),
                        "{text(Key::CalendarRetry)}"
                    }
                }
            }
        }
    }
}

#[component]
fn PartRail(
    counts: Vec<(BodyPart, u32)>,
    selected: Option<BodyPart>,
    training_days: u32,
    on_select: EventHandler<Option<BodyPart>>,
) -> Element {
    rsx! {
        aside {
            class: "flex w-[60px] shrink-0 flex-col border-r border-line bg-surface",
            "data-calendar-legend": "true",
            "aria-label": text(Key::CalendarParts),
            span { class: "flex h-12 items-center justify-center text-[11px] text-text-sub",
                "{text(Key::CalendarParts)}"
            }
            button {
                class: part_button_class(selected.is_none()),
                "data-calendar-part": "all",
                "data-calendar-all-days": "{training_days}",
                "aria-pressed": if selected.is_none() { "true" } else { "false" },
                onclick: move |_| on_select.call(None),
                span { class: "text-xs font-bold", "{text(Key::CalendarAllParts)}" }
                span { class: "font-condensed text-[15px] font-semibold",
                    "{training_days} {text(Key::CalendarDaysUnit)}"
                }
            }
            for (part, days) in counts {
                {
                    let active = selected == Some(part);
                    let class = part_button_class(active);
                    rsx! {
                        button {
                            class: class,
                            "data-calendar-part": part.as_str(),
                            "data-calendar-part-days": "{days}",
                            "aria-pressed": if active { "true" } else { "false" },
                            onclick: move |_| on_select.call(Some(part)),
                            span {
                                class: format!("h-1.5 w-[22px] {}", part_color_class(part)),
                                "data-calendar-swatch": part.as_str(),
                                "aria-hidden": "true"
                            }
                            span { class: "text-xs", "{part_label(part)}" }
                            span { class: "font-condensed text-sm font-semibold text-text-sub",
                                "{days} {text(Key::CalendarDaysUnit)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CalendarDayCell(cell: CalendarCell, on_select: EventHandler<JstDate>) -> Element {
    let class = day_cell_class(&cell);
    let number_class = day_number_class(cell.today);
    let date = cell.date;
    let label = day_aria_label(&cell);
    let parts = cell
        .body_parts
        .iter()
        .map(|part| part.as_str())
        .collect::<Vec<_>>()
        .join(",");
    rsx! {
        button {
            class: class,
            "data-calendar-day": "{date}",
            "data-calendar-parts": parts,
            "data-calendar-highlighted": if cell.highlighted { "true" } else { "false" },
            "data-calendar-selected": if cell.selected { "true" } else { "false" },
            "aria-pressed": if cell.selected { "true" } else { "false" },
            "aria-label": label,
            onclick: move |_| on_select.call(date),
            span { class: number_class, "{date.day()}" }
            if !cell.body_parts.is_empty() {
                span { class: "flex h-1.5 w-[26px] gap-0.5", "aria-hidden": "true",
                    for part in cell.body_parts.iter() {
                        span {
                            class: format!("flex-1 {}", part_color_class(*part)),
                            "data-calendar-marker": part.as_str()
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DayDetail(
    detail: Option<CalendarDayDetail>,
    selected: Option<JstDate>,
    error: Option<String>,
    loading: bool,
    on_retry: EventHandler<()>,
) -> Element {
    let date_attr = selected.map(|date| date.to_string()).unwrap_or_default();
    rsx! {
        section {
            class: "mt-1 border-t border-text pt-2.5",
            "data-calendar-detail": date_attr,
            if let Some(message) = error {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    class: "mt-1 min-h-11 border border-line px-3 text-[13px]",
                    onclick: move |_| on_retry.call(()),
                    "{text(Key::CalendarRetry)}"
                }
            } else if let Some(detail) = detail {
                DayDetailBody { detail: detail }
            } else if loading {
                p { class: "text-[13px] text-text-sub", "{text(Key::CalendarLoading)}" }
            } else {
                p { class: "text-[13px] text-text-sub", "{text(Key::CalendarSelectDay)}" }
            }
        }
    }
}

#[component]
fn DayDetailBody(detail: CalendarDayDetail) -> Element {
    let parts = day_parts(&detail.trainings);
    let total = detail.total_weight_kg;
    rsx! {
        div { class: "flex items-baseline justify-between gap-2",
            h2 { class: "flex items-center gap-2 text-sm font-bold",
                "{day_title(detail.date)}"
                for part in parts {
                    span {
                        class: format!("h-2.5 w-2.5 {}", part_color_class(part)),
                        "data-calendar-day-part": part.as_str(),
                        "aria-hidden": "true"
                    }
                }
            }
            span { class: "text-[11px] text-text-sub",
                "{text(Key::CalendarTotalWeight)} "
                strong {
                    class: "font-condensed text-[18px] font-semibold text-text",
                    "data-calendar-day-total": "{total}",
                    "{format_weight(total)}"
                }
            }
        }
        if detail.trainings.is_empty() {
            p { class: "mt-2 text-[13px] text-text-sub", "{text(Key::CalendarNoTraining)}" }
        }
        for training in detail.trainings {
            div { "data-calendar-training": training.training_id.clone(),
                for exercise in training.exercises {
                    ExerciseDetail { exercise: exercise }
                }
            }
        }
    }
}

#[component]
fn ExerciseDetail(exercise: CalendarExercise) -> Element {
    let part = exercise.body_part;
    let total = exercise.total_weight_kg;
    let name = exercise.name.clone();
    let exercise_id = exercise.exercise_id.clone();
    rsx! {
        div {
            class: "flex flex-col border-b border-line py-1",
            "data-calendar-exercise": exercise_id,
            "data-calendar-exercise-name": name.clone(),
            "data-calendar-exercise-total": "{total}",
            div { class: "flex min-h-6 items-center justify-between gap-2",
                div { class: "flex min-w-0 items-center gap-2",
                    span {
                        class: format!("h-2.5 w-2.5 shrink-0 {}", part_color_class(part)),
                        "data-calendar-exercise-part": part.as_str(),
                        "aria-hidden": "true"
                    }
                    span { class: "truncate text-[13px]", "{name}" }
                }
                span { class: "shrink-0 font-condensed text-[15px] font-semibold",
                    "{format_weight(total)}"
                }
            }
            ol { class: "mt-1 flex flex-col gap-1",
                for (index, set) in exercise.sets.iter().enumerate() {
                    SetDetail { index: index + 1, set: set.clone() }
                }
            }
        }
    }
}

#[component]
fn SetDetail(index: usize, set: CalendarSet) -> Element {
    let body = set_text(&set);
    let notes = set_notes(&set);
    let kind = set.kind.as_str();
    let total = set.total_weight_kg;
    let notes_text = notes.join(" / ");
    rsx! {
        li {
            class: "flex flex-col gap-0.5 pl-4",
            "data-calendar-set": "{index}",
            "data-calendar-set-kind": kind,
            "data-calendar-set-total": "{total}",
            "data-calendar-set-text": body.clone(),
            "data-calendar-set-notes": notes_text.clone(),
            span { class: "font-condensed text-[13px] font-semibold", "{body}" }
            span { class: "text-[11px] text-text-sub", "{notes_text}" }
        }
    }
}

async fn load_month(
    month: JstMonth,
    part: Option<BodyPart>,
    mut data: Signal<Option<CalendarMonth>>,
    mut error: Signal<Option<String>>,
) {
    error.set(None);
    match calendar_api::month(month.start(), part).await {
        Ok(view) => {
            data.set(Some(view));
            error.set(None);
        }
        Err(_) => {
            data.set(None);
            error.set(Some(text(Key::CalendarLoadFailed).to_owned()));
        }
    }
}

async fn load_day(
    date: JstDate,
    mut data: Signal<Option<CalendarDayDetail>>,
    mut error: Signal<Option<String>>,
) {
    error.set(None);
    match calendar_api::day(date).await {
        Ok(detail) => {
            data.set(Some(detail));
            error.set(None);
        }
        Err(_) => {
            data.set(None);
            error.set(Some(text(Key::CalendarDayLoadFailed).to_owned()));
        }
    }
}

fn current_date() -> Option<JstDate> {
    let seconds = (js_sys::Date::now() / 1000.0) as i64;
    UtcInstant::from_epoch_seconds(seconds).jst_date()
}

fn month_text(month: JstMonth) -> String {
    format!("{:04}-{:02}", month.start().year(), month.start().month())
}
