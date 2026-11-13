use dioxus::prelude::*;

use crate::routes::navigate;
use crate::screens::ScreenFrame;
use crate::storage;
use crate::text::{text, Key};

#[component]
pub fn HomeScreen() -> Element {
    use_effect(move || {
        spawn(async move {
            let Ok(storage) = storage::current().await else {
                return;
            };
            if let Ok(Some(session)) = storage.active_session().await {
                navigate(&format!("/record/{}", session.id));
            }
        });
    });

    rsx! {
        ScreenFrame {
            title: text(Key::HomeTitle).to_owned(),
            description: text(Key::HomeDescription).to_owned()
        }
    }
}
