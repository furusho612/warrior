use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn CalendarScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::CalendarTitle).to_owned(),
            description: text(Key::CalendarDescription).to_owned()
        }
    }
}
