use dioxus::prelude::*;

use crate::screens::AccountScreen;

#[component]
pub fn MyPageScreen() -> Element {
    rsx! {
        AccountScreen {}
    }
}
