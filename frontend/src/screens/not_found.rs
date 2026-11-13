use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn NotFoundScreen(segments: Vec<String>) -> Element {
    let _ = segments;
    rsx! {
        ScreenFrame {
            title: text(Key::NotFoundTitle).to_owned(),
            description: text(Key::NotFoundDescription).to_owned()
        }
    }
}
