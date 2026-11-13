use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn SummaryScreen(training_id: String) -> Element {
    rsx! {
        div { id: training_id,
            ScreenFrame {
                title: text(Key::SummaryTitle).to_owned(),
                description: text(Key::SummaryDescription).to_owned()
            }
        }
    }
}
