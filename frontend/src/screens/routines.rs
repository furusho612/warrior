use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn RoutinesScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::RoutinesTitle).to_owned(),
            description: text(Key::RoutinesDescription).to_owned()
        }
    }
}
