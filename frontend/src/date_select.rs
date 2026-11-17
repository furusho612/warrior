//! 年、月、日を 3 つの選択欄で入力する日付の部品。

use dioxus::prelude::*;

/// 年、月、日を選ぶ日付の入力。
#[component]
pub fn DateSelect(
    value: String,
    min_year: i32,
    max_year: i32,
    name: &'static str,
    on_change: EventHandler<String>,
) -> Element {
    let (year, month, day) = split_date(&value, min_year, max_year);
    let days = days_in_month(year, month);
    let year_value = year.to_string();
    let month_value = format!("{month:02}");
    let day_value = format!("{day:02}");
    let years = min_year..=max_year;
    let months = 1..=12;
    let day_options = 1..=days;
    let year_change = (year, month, day);
    let month_change = (year, month, day);
    let day_change = (year, month, day);
    rsx! {
        div {
            class: "grid grid-cols-3 gap-1",
            "data-date-input": name,
            select {
                class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                "data-date-year": "true",
                aria_label: "年",
                value: "{year_value}",
                onchange: move |event| {
                    let Ok(next_year) = event.value().parse::<i32>() else {
                        return;
                    };
                    let (_, month, day) = year_change;
                    on_change.call(format_date(next_year, month, day.min(days_in_month(next_year, month))));
                },
                for year in years {
                    option { value: "{year}", selected: year == year_change.0, "{year} 年" }
                }
            }
            select {
                class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                "data-date-month": "true",
                aria_label: "月",
                value: "{month_value}",
                onchange: move |event| {
                    let Ok(next_month) = event.value().parse::<u32>() else {
                        return;
                    };
                    let (year, _, day) = month_change;
                    on_change.call(format_date(year, next_month, day.min(days_in_month(year, next_month))));
                },
                for month in months {
                    option { value: "{month:02}", selected: month == month_change.1, "{month} 月" }
                }
            }
            select {
                class: "min-h-11 border border-line bg-bg px-2 text-[13px] text-text",
                "data-date-day": "true",
                aria_label: "日",
                value: "{day_value}",
                onchange: move |event| {
                    let Ok(next_day) = event.value().parse::<u32>() else {
                        return;
                    };
                    let (year, month, _) = day_change;
                    on_change.call(format_date(year, month, next_day));
                },
                for day in day_options {
                    option { value: "{day:02}", selected: day == day_change.2, "{day} 日" }
                }
            }
        }
    }
}

fn split_date(value: &str, min_year: i32, max_year: i32) -> (i32, u32, u32) {
    let mut parts = value.split('-');
    let year = parts
        .next()
        .and_then(|part| part.parse::<i32>().ok())
        .filter(|year| (min_year..=max_year).contains(year))
        .unwrap_or((max_year - 30).max(min_year));
    let month = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .filter(|month| (1..=12).contains(month))
        .unwrap_or(1);
    let day = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .filter(|day| (1..=days_in_month(year, month)).contains(day))
        .unwrap_or(1);
    (year, month, day)
}

fn format_date(year: i32, month: u32, day: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::{days_in_month, format_date, split_date};

    #[test]
    fn the_default_date_is_inside_the_range_and_the_format_is_fixed() {
        assert_eq!(split_date("", 1900, 2026), (1996, 1, 1));
        assert_eq!(split_date("1990-05-10", 1900, 2026), (1990, 5, 10));
        assert_eq!(split_date("2026-02-31", 1900, 2026), (2026, 2, 1));
        assert_eq!(format_date(2026, 2, 3), "2026-02-03");
    }

    #[test]
    fn the_days_follow_the_calendar() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2025, 2), 28);
        assert_eq!(days_in_month(2026, 4), 30);
    }
}
