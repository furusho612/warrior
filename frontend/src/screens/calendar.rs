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
    let state = use_calendar_state(today);
    let handlers = calendar_handlers(state);

    let month_now = (state.month)();
    let part_now = (state.part)();
    let selected = (state.selected)();
    let view = (state.month_data)();
    let month_error = (state.month_error)();
    let detail = (state.day_data)();
    let day_error = (state.day_error)();
    let training_days = view.as_ref().map(|view| view.training_days).unwrap_or(0);
    let loading = month_now.is_some() && view.is_none() && month_error.is_none();
    let detail_loading = selected.is_some() && detail.is_none() && day_error.is_none();

    rsx! {
        div {
            class: "flex min-h-full",
            "data-calendar": "true",
            "data-calendar-month": month_now.map(month_text).unwrap_or_default(),
            "data-calendar-selected-part": part_now.map(BodyPart::as_str).unwrap_or_default(),
            PartRail {
                counts: part_counts(view.as_ref()),
                selected: part_now,
                training_days: training_days,
                on_select: handlers.part,
            }
            section { class: "min-w-0 flex-1 px-3.5",
                div { class: "flex h-12 items-center justify-between",
                    h1 { class: "text-[17px] font-black", "{text(Key::CalendarTitle)}" }
                }
                MonthHeading {
                    month: month_now,
                    training_days: training_days,
                    on_previous: handlers.previous,
                    on_next: handlers.next,
                }
                MonthGrid {
                    month: month_now,
                    view: view,
                    selected: selected,
                    today: today,
                    on_select: handlers.select,
                }
                DayDetail {
                    detail: detail,
                    selected: selected,
                    error: day_error,
                    loading: detail_loading,
                    on_retry: handlers.day_retry,
                }
                MonthStatus {
                    loading: loading,
                    error: month_error,
                    on_retry: handlers.month_retry,
                }
            }
        }
    }
}

/// カレンダー画面の状態。月と日の読み込みも、この状態へ書き込む。
#[derive(Clone, Copy)]
struct CalendarSignals {
    month: Signal<Option<JstMonth>>,
    part: Signal<Option<BodyPart>>,
    selected: Signal<Option<JstDate>>,
    month_data: Signal<Option<CalendarMonth>>,
    month_error: Signal<Option<String>>,
    day_data: Signal<Option<CalendarDayDetail>>,
    day_error: Signal<Option<String>>,
    month_reload: Signal<u32>,
    day_reload: Signal<u32>,
    month_generation: Signal<u32>,
    day_generation: Signal<u32>,
}

/// カレンダー画面の状態を作り、月と日の読み込みを始める効果を登録する。
fn use_calendar_state(today: Option<JstDate>) -> CalendarSignals {
    let state = CalendarSignals {
        month: use_signal(|| today.map(|date| date.month_range())),
        part: use_signal(|| Option::<BodyPart>::None),
        selected: use_signal(|| Option::<JstDate>::None),
        month_data: use_signal(|| Option::<CalendarMonth>::None),
        month_error: use_signal(|| Option::<String>::None),
        day_data: use_signal(|| Option::<CalendarDayDetail>::None),
        day_error: use_signal(|| Option::<String>::None),
        month_reload: use_signal(|| 0u32),
        day_reload: use_signal(|| 0u32),
        month_generation: use_signal(|| 0u32),
        day_generation: use_signal(|| 0u32),
    };

    use_effect(move || {
        let mut state = state;
        let _ = (state.month_reload)();
        let Some(current) = (state.month)() else {
            return;
        };
        let generation = state.month_generation.peek().wrapping_add(1);
        state.month_generation.set(generation);
        state.month_error.set(None);
        let request = MonthLoad {
            generation,
            month: current,
            part: (state.part)(),
        };
        spawn(async move {
            load_month(request, state).await;
        });
    });

    use_effect(move || {
        let mut state = state;
        let _ = (state.day_reload)();
        let Some(date) = (state.selected)() else {
            return;
        };
        let generation = state.day_generation.peek().wrapping_add(1);
        state.day_generation.set(generation);
        state.day_error.set(None);
        let request = DayLoad { generation, date };
        spawn(async move {
            load_day(request, state).await;
        });
    });

    state
}

/// カレンダー画面の操作。
#[derive(Clone, Copy)]
struct CalendarHandlers {
    select: EventHandler<JstDate>,
    part: EventHandler<Option<BodyPart>>,
    previous: EventHandler<()>,
    next: EventHandler<()>,
    month_retry: EventHandler<()>,
    day_retry: EventHandler<()>,
}

fn calendar_handlers(state: CalendarSignals) -> CalendarHandlers {
    CalendarHandlers {
        select: EventHandler::new(move |date: JstDate| {
            let mut state = state;
            state.selected.set(Some(date));
            state.day_data.set(None);
            state.day_error.set(None);
        }),
        part: EventHandler::new(move |value: Option<BodyPart>| {
            let mut state = state;
            state.part.set(value);
        }),
        previous: EventHandler::new(move |_: ()| move_to_month(state, previous_month)),
        next: EventHandler::new(move |_: ()| move_to_month(state, next_month)),
        month_retry: EventHandler::new(move |_: ()| {
            let mut state = state;
            state.month_reload += 1;
        }),
        day_retry: EventHandler::new(move |_: ()| {
            let mut state = state;
            state.day_reload += 1;
        }),
    }
}

/// 月を移し、前の月の読み込み結果と選択を捨てる。
fn move_to_month(state: CalendarSignals, target: impl FnOnce(JstMonth) -> Option<JstMonth>) {
    let mut state = state;
    let Some(month) = (state.month)() else {
        return;
    };
    let Some(month) = target(month) else {
        return;
    };
    state.month.set(Some(month));
    state.month_data.set(None);
    state.selected.set(None);
    state.day_data.set(None);
    state.day_error.set(None);
}

/// 月の見出しと、前後の月への移動。
#[component]
fn MonthHeading(
    month: Option<JstMonth>,
    training_days: u32,
    on_previous: EventHandler<()>,
    on_next: EventHandler<()>,
) -> Element {
    let (year, month_number) = month
        .map(|current| (current.start().year(), current.start().month()))
        .unwrap_or_default();
    rsx! {
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
    }
}

/// 曜日の見出しと、月の日のセルのグリッド。
#[component]
fn MonthGrid(
    month: Option<JstMonth>,
    view: Option<CalendarMonth>,
    selected: Option<JstDate>,
    today: Option<JstDate>,
    on_select: EventHandler<JstDate>,
) -> Element {
    let cells = build_cells(month, view.as_ref(), selected, today);
    rsx! {
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
    }
}

/// 月の読み込み中と、読み込みに失敗したときの表示。
#[component]
fn MonthStatus(loading: bool, error: Option<String>, on_retry: EventHandler<()>) -> Element {
    rsx! {
        if loading {
            p { class: "mt-2 text-[13px] text-text-sub", "{text(Key::CalendarLoading)}" }
        }
        if let Some(message) = error {
            p { class: "mt-2 text-[13px] text-primary", "{message}" }
            button {
                class: "mt-1 min-h-11 border border-line px-3 text-[13px]",
                onclick: move |_| on_retry.call(()),
                "{text(Key::CalendarRetry)}"
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
    let number_class = day_number_class(&cell);
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
            "data-calendar-future": if cell.future { "true" } else { "false" },
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

/// 月の読み込み要求。応答を適用してよいかを、要求した月と部位と世代で決める。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MonthLoad {
    /// 要求の世代。要求を出すたびに進む。
    generation: u32,
    month: JstMonth,
    part: Option<BodyPart>,
}

impl MonthLoad {
    /// いまの世代が要求のままで、表示中の月と部位も要求のままなら応答を適用する。
    fn is_current(self, generation: u32, month: Option<JstMonth>, part: Option<BodyPart>) -> bool {
        self.generation == generation && month == Some(self.month) && self.part == part
    }
}

/// 日の読み込み要求。応答を適用してよいかを、要求した日と世代で決める。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DayLoad {
    /// 要求の世代。要求を出すたびに進む。
    generation: u32,
    date: JstDate,
}

impl DayLoad {
    /// いまの世代が要求のままで、選択中の日も要求のままなら応答を適用する。
    fn is_current(self, generation: u32, selected: Option<JstDate>) -> bool {
        self.generation == generation && selected == Some(self.date)
    }
}

async fn load_month(request: MonthLoad, mut state: CalendarSignals) {
    let result = calendar_api::month(request.month.start(), request.part).await;
    if !request.is_current(
        *state.month_generation.peek(),
        *state.month.peek(),
        *state.part.peek(),
    ) {
        return;
    }
    match result {
        Ok(view) => {
            state.month_data.set(Some(view));
            state.month_error.set(None);
        }
        Err(_) => {
            state.month_data.set(None);
            state
                .month_error
                .set(Some(text(Key::CalendarLoadFailed).to_owned()));
        }
    }
}

async fn load_day(request: DayLoad, mut state: CalendarSignals) {
    let result = calendar_api::day(request.date).await;
    if !request.is_current(*state.day_generation.peek(), *state.selected.peek()) {
        return;
    }
    match result {
        Ok(detail) => {
            state.day_data.set(Some(detail));
            state.day_error.set(None);
        }
        Err(_) => {
            state.day_data.set(None);
            state
                .day_error
                .set(Some(text(Key::CalendarDayLoadFailed).to_owned()));
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

#[cfg(test)]
mod tests {
    //! 月と日の読み込み結果を適用してよいかの単体テスト。
    //!
    //! 要求の受入基準は E2E が担う。応答の到着順は実ブラウザの通信に依存するため、
    //! ここでは古い応答を捨てる条件を直接確かめる。

    use warrior_core::calendar::next_month;
    use warrior_core::exercise::BodyPart;
    use warrior_core::jst::JstDate;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::{DayLoad, MonthLoad};

    #[wasm_bindgen_test]
    fn a_month_response_of_a_month_that_moved_away_is_not_applied() {
        let november = JstDate::new(2026, 11, 12).unwrap().month_range();
        let december = next_month(november).expect("a next month");
        let request = MonthLoad {
            generation: 2,
            month: november,
            part: None,
        };
        assert!(request.is_current(2, Some(november), None));
        assert!(
            !request.is_current(2, Some(december), None),
            "the displayed month moved before the response arrived"
        );
        assert!(
            !request.is_current(3, Some(november), None),
            "a newer request replaced this request"
        );
        assert!(
            !request.is_current(2, Some(november), Some(BodyPart::Chest)),
            "the part filter changed before the response arrived"
        );
    }

    #[wasm_bindgen_test]
    fn a_month_response_is_applied_while_the_filter_stays() {
        let november = JstDate::new(2026, 11, 12).unwrap().month_range();
        let request = MonthLoad {
            generation: 4,
            month: november,
            part: Some(BodyPart::Chest),
        };
        assert!(request.is_current(4, Some(november), Some(BodyPart::Chest)));
        assert!(!request.is_current(4, Some(november), None));
        assert!(!request.is_current(4, Some(november), Some(BodyPart::Legs)));
    }

    #[wasm_bindgen_test]
    fn a_day_response_of_an_unselected_day_is_not_applied() {
        let first = JstDate::new(2026, 11, 1).unwrap();
        let second = JstDate::new(2026, 11, 2).unwrap();
        let request = DayLoad {
            generation: 5,
            date: first,
        };
        assert!(request.is_current(5, Some(first)));
        assert!(
            !request.is_current(5, Some(second)),
            "the selected day changed before the response arrived"
        );
        assert!(
            !request.is_current(5, None),
            "the month moved and the day is no longer selected"
        );
        assert!(
            !request.is_current(6, Some(first)),
            "a newer request replaced this request"
        );
    }
}
