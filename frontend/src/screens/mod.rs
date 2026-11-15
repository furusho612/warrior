//! 画面と共通の枠。
//!
//! 経路ごとの画面と、画面の共通の枠を持つ。

use dioxus::prelude::*;

pub mod account;
pub mod analysis;
pub mod calendar;
pub mod exercise;
pub mod exercise_history;
pub mod home;
pub mod library;
pub mod login;
pub mod mypage;
pub mod not_found;
pub mod onboarding;
pub mod profile_form;
pub mod record;
pub mod routines;
pub mod summary;

pub use account::AccountSettings;
pub use analysis::AnalysisScreen;
pub use calendar::CalendarScreen;
pub use exercise::ExerciseScreen;
pub use exercise_history::ExerciseHistoryScreen;
pub use home::HomeScreen;
pub use library::{ExerciseSelectScreen, LibraryScreen};
pub use login::LoginScreen;
pub use mypage::MyPageScreen;
pub use not_found::NotFoundScreen;
pub use onboarding::OnboardingPanel;
pub use record::{RecordScreen, RecordSessionScreen};
pub use routines::RoutinesScreen;
pub use summary::SummaryScreen;

/// 画面の共通の枠。題と説明と、任意の操作を表示する。
#[component]
pub fn ScreenFrame(title: String, description: String, action: Option<Element>) -> Element {
    rsx! {
        section { class: "flex min-h-full flex-col gap-3 p-5",
            h1 { class: "text-lg font-bold", "{title}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{description}" }
            if let Some(action) = action {
                {action}
            }
        }
    }
}
