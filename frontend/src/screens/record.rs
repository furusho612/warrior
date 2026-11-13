use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn RecordScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::RecordTitle).to_owned(),
            description: text(Key::RecordDescription).to_owned()
        }
    }
}

#[component]
pub fn RecordSessionScreen(training_id: String) -> Element {
    rsx! {
        div { id: training_id,
            RecordScreen {}
        }
    }
}
