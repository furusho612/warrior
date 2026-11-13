//! 画面の経路の台帳とルーター。

use dioxus::prelude::*;
use dioxus_router::Routable;

use crate::layout::AppShell;
use crate::screens::{
    AnalysisScreen, CalendarScreen, ExerciseScreen, HomeScreen, LibraryScreen, LoginScreen,
    MyPageScreen, NotFoundScreen, RecordScreen, RecordSessionScreen, RoutinesScreen, SummaryScreen,
};
use crate::text::Key;

/// 経路の台帳の 1 行。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppRoute {
    /// ログに載せる経路の名前。
    pub name: &'static str,
    /// Dioxus のルーターに登録するパターン。
    pub pattern: &'static str,
}

const fn route(pattern: &'static str) -> AppRoute {
    AppRoute {
        name: pattern,
        pattern,
    }
}

/// 画面の経路の台帳。
pub const APP_ROUTES: [AppRoute; 12] = [
    route("/login"),
    route("/login/error"),
    route("/"),
    route("/record"),
    route("/record/:training_id"),
    route("/calendar"),
    route("/library"),
    route("/library/:exercise_id"),
    route("/analysis"),
    route("/routines"),
    route("/summary/:training_id"),
    route("/mypage"),
];

/// 画面の経路。
#[derive(Routable, Clone, PartialEq, Debug)]
pub enum Route {
    /// ログイン。共通の枠と下部のタブを出さない。
    #[route("/login", LoginScreen)]
    Login {},
    /// ログインの誤り。共通の枠と下部のタブを出さない。
    #[route("/login/error", LoginScreen)]
    LoginError {},
    #[layout(AppShell)]
    /// ホーム。
    #[route("/", HomeScreen)]
    Home {},
    /// 記録中のトレーニング。
    #[route("/record", RecordScreen)]
    Record {},
    /// 記録中のトレーニング (ID あり)。
    #[route("/record/:training_id", RecordSessionScreen)]
    RecordSession { training_id: String },
    /// カレンダー。
    #[route("/calendar", CalendarScreen)]
    Calendar {},
    /// 種目ライブラリ。
    #[route("/library", LibraryScreen)]
    Library {},
    /// 種目詳細。
    #[route("/library/:exercise_id", ExerciseScreen)]
    Exercise { exercise_id: String },
    /// 分析。
    #[route("/analysis", AnalysisScreen)]
    Analysis {},
    /// ルーティンとプログラム。
    #[route("/routines", RoutinesScreen)]
    Routines {},
    /// 終了サマリー。
    #[route("/summary/:training_id", SummaryScreen)]
    Summary { training_id: String },
    /// マイページ。
    #[route("/mypage", MyPageScreen)]
    MyPage {},
    /// 未知の経路。
    #[route("/:..segments", NotFoundScreen)]
    NotFound { segments: Vec<String> },
}

/// 経路に対応する画面の題のキー。
pub fn screen_key(route: &Route) -> Key {
    match route {
        Route::Login {} => Key::LoginTitle,
        Route::LoginError {} => Key::LoginTitle,
        Route::Home {} => Key::HomeTitle,
        Route::Record {} | Route::RecordSession { .. } => Key::RecordTitle,
        Route::Calendar {} => Key::CalendarTitle,
        Route::Library {} => Key::LibraryTitle,
        Route::Exercise { .. } => Key::ExerciseTitle,
        Route::Analysis {} => Key::AnalysisTitle,
        Route::Routines {} => Key::RoutinesTitle,
        Route::Summary { .. } => Key::SummaryTitle,
        Route::MyPage {} => Key::MyPageTitle,
        Route::NotFound { .. } => Key::NotFoundTitle,
    }
}

/// 経路のパターン。E2E の画面数の集計に使う。
pub fn route_name(route: &Route) -> &'static str {
    match route {
        Route::Login {} => "/login",
        Route::LoginError {} => "/login/error",
        Route::Home {} => "/",
        Route::Record {} => "/record",
        Route::RecordSession { .. } => "/record/:training_id",
        Route::Calendar {} => "/calendar",
        Route::Library {} => "/library",
        Route::Exercise { .. } => "/library/:exercise_id",
        Route::Analysis {} => "/analysis",
        Route::Routines {} => "/routines",
        Route::Summary { .. } => "/summary/:training_id",
        Route::MyPage {} => "/mypage",
        Route::NotFound { .. } => "/:..segments",
    }
}

/// 下部のタブ。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    /// ホーム。
    Home,
    /// カレンダー。
    Calendar,
    /// 種目ライブラリ。
    Library,
    /// 分析。
    Analysis,
    /// マイページ。
    MyPage,
}

/// 下部のタブの並び。
pub const TABS: [Tab; 5] = [
    Tab::Home,
    Tab::Calendar,
    Tab::Library,
    Tab::Analysis,
    Tab::MyPage,
];

/// タブの遷移先。
pub fn tab_route(tab: Tab) -> Route {
    match tab {
        Tab::Home => Route::Home {},
        Tab::Calendar => Route::Calendar {},
        Tab::Library => Route::Library {},
        Tab::Analysis => Route::Analysis {},
        Tab::MyPage => Route::MyPage {},
    }
}

/// タブの文言のキー。
pub fn tab_key(tab: Tab) -> Key {
    match tab {
        Tab::Home => Key::NavHome,
        Tab::Calendar => Key::NavCalendar,
        Tab::Library => Key::NavLibrary,
        Tab::Analysis => Key::NavAnalysis,
        Tab::MyPage => Key::NavMypage,
    }
}

/// 現在の経路がタブの範囲に入るか。
pub fn tab_is_active(current: &Route, tab: Tab) -> bool {
    match tab {
        Tab::Home => matches!(current, Route::Home {}),
        Tab::Calendar => matches!(current, Route::Calendar {}),
        Tab::Library => matches!(current, Route::Library {} | Route::Exercise { .. }),
        Tab::Analysis => matches!(current, Route::Analysis {}),
        Tab::MyPage => matches!(current, Route::MyPage {}),
    }
}

/// 画面を読み込み直して移動する。
///
/// Dioxus のルーターの遷移の途中でスコープを破棄すると、runtime が再入することがある。
/// 記録の開始と復元, 破棄の後はブラウザの読み込みで移動する。
pub fn navigate(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        route_name, screen_key, tab_is_active, tab_key, tab_route, Route, Tab, APP_ROUTES, TABS,
    };
    use crate::text::{text, Key};

    #[test]
    fn the_ledger_has_the_twelve_routes() {
        let patterns = [
            "/login",
            "/login/error",
            "/",
            "/record",
            "/record/:training_id",
            "/calendar",
            "/library",
            "/library/:exercise_id",
            "/analysis",
            "/routines",
            "/summary/:training_id",
            "/mypage",
        ];
        assert_eq!(APP_ROUTES.len(), patterns.len());
        for (index, pattern) in patterns.iter().enumerate() {
            assert_eq!(APP_ROUTES[index].pattern, *pattern);
            assert_eq!(APP_ROUTES[index].name, *pattern);
        }
    }

    #[test]
    fn the_route_patterns_are_unique() {
        let mut patterns = APP_ROUTES
            .iter()
            .map(|route| route.pattern)
            .collect::<Vec<&str>>();
        patterns.sort_unstable();
        patterns.dedup();
        assert_eq!(patterns.len(), APP_ROUTES.len());
    }

    #[test]
    fn every_static_route_is_registered() {
        for app_route in APP_ROUTES {
            if app_route.pattern.contains(':') {
                continue;
            }
            let parsed = Route::from_str(app_route.pattern).expect("registered route");
            assert_eq!(parsed.to_string().trim_end_matches('?'), app_route.pattern);
        }
    }

    #[test]
    fn dynamic_routes_keep_their_identifiers() {
        assert_eq!(
            Route::from_str("/record/abc").unwrap(),
            Route::RecordSession {
                training_id: "abc".to_owned()
            }
        );
        assert_eq!(
            Route::from_str("/library/abc").unwrap(),
            Route::Exercise {
                exercise_id: "abc".to_owned()
            }
        );
        assert_eq!(
            Route::from_str("/summary/abc").unwrap(),
            Route::Summary {
                training_id: "abc".to_owned()
            }
        );
    }

    #[test]
    fn route_names_are_the_ledger_patterns() {
        assert_eq!(route_name(&Route::Home {}), "/");
        assert_eq!(route_name(&Route::Record {}), "/record");
        assert_eq!(
            route_name(&Route::RecordSession {
                training_id: "abc".to_owned()
            }),
            "/record/:training_id"
        );
        assert_eq!(
            route_name(&Route::NotFound { segments: vec![] }),
            "/:..segments"
        );
    }

    #[test]
    fn screen_keys_are_defined_for_every_route() {
        assert_eq!(screen_key(&Route::Home {}), Key::HomeTitle);
        assert_eq!(screen_key(&Route::Record {}), Key::RecordTitle);
        assert_eq!(
            screen_key(&Route::Exercise {
                exercise_id: "abc".to_owned()
            }),
            Key::ExerciseTitle
        );
        assert_eq!(screen_key(&Route::Login {}), Key::LoginTitle);
    }

    #[test]
    fn tabs_have_routes_and_labels() {
        assert_eq!(TABS.len(), 5);
        for tab in TABS {
            assert!(!text(tab_key(tab)).is_empty());
        }
        assert_eq!(tab_route(Tab::Home), Route::Home {});
        assert_eq!(tab_route(Tab::MyPage), Route::MyPage {});
    }

    #[test]
    fn library_tab_stays_active_on_exercise_details() {
        assert!(tab_is_active(&Route::Library {}, Tab::Library));
        assert!(tab_is_active(
            &Route::Exercise {
                exercise_id: "abc".to_owned()
            },
            Tab::Library
        ));
        assert!(!tab_is_active(
            &Route::Exercise {
                exercise_id: "abc".to_owned()
            },
            Tab::Home
        ));
    }

    #[test]
    fn record_routes_do_not_activate_tabs() {
        for tab in TABS {
            assert!(!tab_is_active(&Route::Record {}, tab));
        }
    }
}
