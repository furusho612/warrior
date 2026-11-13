use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn MyPageScreen() -> Element {
    rsx! {
        ScreenFrame {
            title: text(Key::MyPageTitle).to_owned(),
            description: text(Key::MyPageDescription).to_owned()
        }
    }
}
