use dioxus::prelude::*;
use dioxus_router::navigator;

use crate::routes::Route;
use crate::screens::ScreenFrame;
use crate::storage;
use crate::sync;
use crate::text::{text, Key};

#[component]
pub fn HomeScreen() -> Element {
    use_effect(move || {
        spawn(async move {
            sync::start().await;
            let Ok(storage) = storage::current().await else {
                return;
            };
            if let Ok(Some(session)) = storage.active_session().await {
                let _ = navigator().push(Route::RecordSession {
                    training_id: session.id,
                });
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
