//! 共通の画面の枠、下部のタブ、記録中のレールとレストタイマーの枠。

use dioxus::prelude::*;
use dioxus_router::{router, Link, Outlet};

use crate::routes::{tab_is_active, tab_key, tab_route, Route, Tab, TABS};
use crate::text::{text, Key};

/// 全ての画面を包む共通の枠。
#[component]
pub fn AppShell() -> Element {
    let current = router().current::<Route>();
    let route_name = crate::routes::route_name(&current);
    let record = matches!(current, Route::Record {} | Route::RecordSession { .. });

    rsx! {
        div {
            class: "mx-auto flex min-h-dvh w-full max-w-[390px] flex-col overflow-x-hidden bg-bg text-text",
            "data-route": route_name,
            div { class: "shrink-0 pt-[54px]" }
            header { class: "flex h-11 shrink-0 items-center border-b border-line px-5",
                Link {
                    to: Route::Home {},
                    class: "flex min-h-11 min-w-11 items-center",
                    span { class: "font-condensed text-[22px] font-bold leading-none tracking-[0.16em]", "{text(Key::AppName)}" }
                }
            }
            if record {
                div { class: "flex min-h-0 flex-1",
                    aside {
                        class: "flex w-[60px] shrink-0 flex-col items-center border-r border-line bg-surface pt-3 text-[11px] text-text-sub",
                        "data-exercise-rail": "true",
                        span { class: "font-bold", "{text(Key::RecordRail)}" }
                    }
                    main { class: "min-h-0 min-w-0 flex-1 overflow-y-auto", Outlet::<Route> {} }
                }
                div {
                    class: "flex h-[72px] shrink-0 items-center justify-center border-t border-line bg-surface text-[11px] text-text-sub",
                    "data-rest-timer": "true",
                    "{text(Key::RestTimer)}"
                }
            } else {
                main { class: "min-h-0 flex-1 overflow-y-auto", Outlet::<Route> {} }
                TabBar { current }
            }
        }
    }
}

#[component]
fn TabBar(current: Route) -> Element {
    rsx! {
        nav { class: "flex shrink-0 border-t border-line bg-bg pb-6",
            for tab in TABS {
                {
                    let active = tab_is_active(&current, tab);
                    let class = if active {
                        "flex min-h-11 flex-1 flex-col items-center justify-center gap-[3px] text-[11px] font-bold text-primary"
                    } else {
                        "flex min-h-11 flex-1 flex-col items-center justify-center gap-[3px] text-[11px] font-medium text-text-sub"
                    };
                    rsx! {
                        Link { to: tab_route(tab), class,
                            svg {
                                class: "h-[22px] w-[22px]",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.8",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: tab_icon(tab) }
                            }
                            span { "{text(tab_key(tab))}" }
                        }
                    }
                }
            }
        }
    }
}

fn tab_icon(tab: Tab) -> &'static str {
    match tab {
        Tab::Home => "M3 11l9-7 9 7v9a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z",
        Tab::Calendar => "M3 5h18v16H3zM3 10h18M8 3v4M16 3v4",
        Tab::Library => "M6.5 6.5v11M3.5 9v6M17.5 6.5v11M20.5 9v6M6.5 12h11",
        Tab::Analysis => "M5 20V11M12 20V4M19 20v-6M2 20h20",
        Tab::MyPage => "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21c0-4 3.6-7 8-7s8 3 8 7",
    }
}
