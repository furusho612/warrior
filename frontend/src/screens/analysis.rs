use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn AnalysisScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::AnalysisTitle).to_owned(),
            description: text(Key::AnalysisDescription).to_owned()
        }
    }
}
