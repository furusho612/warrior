use dioxus::prelude::*;

use crate::screens::ScreenFrame;
use crate::text::{text, Key};

#[component]
pub fn ExerciseScreen(exercise_id: String) -> Element {
    rsx! {
        div { id: exercise_id,
            ScreenFrame {
                title: text(Key::ExerciseTitle).to_owned(),
                description: text(Key::ExerciseDescription).to_owned()
            }
        }
    }
}
