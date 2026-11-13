use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn LibraryScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::LibraryTitle).to_owned(),
            description: text(Key::LibraryDescription).to_owned()
        }
    }
}
