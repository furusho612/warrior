//! 記録画面のスーパーセットの組の表示と、組の作成、追加、解除。
//!
//! 選んだ種目は組に含め、候補の種目を選んでまとめる。選んだ種目が既に組に属する
//! ときは、その組に候補を加える。組の種目の順序は種目の順序に従う。

use dioxus::prelude::*;

use super::exercise_view::SupersetGroupView;
use crate::text::{text, Key};

/// 組に加えられる種目。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SupersetCandidate {
    /// 種目の行の ID。
    pub(super) training_exercise_id: String,
    /// 種目ライブラリの種目の ID。
    pub(super) exercise_id: String,
    /// 種目の名前。
    pub(super) name: String,
}

/// スーパーセットの組の表示と操作。
#[component]
pub(super) fn SupersetPanel(
    view_id: String,
    group: Option<SupersetGroupView>,
    members: Vec<(String, String)>,
    candidates: Vec<SupersetCandidate>,
    disabled: bool,
    on_create: EventHandler<(String, Vec<String>)>,
    on_delete: EventHandler<String>,
) -> Element {
    let selected = use_signal(Vec::<String>::new);
    let group_id = group.as_ref().map(|group| group.id.clone());
    let has_group = group.is_some();
    let create_view_id = view_id.clone();
    let selected_for_create = selected;
    let candidates_for_create = candidates.clone();
    let can_create = !disabled && !selected().is_empty() && !candidates_for_create.is_empty();
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-line p-2",
            "data-superset-panel": "true",
            h4 { class: "text-[11px] font-bold", "{text(Key::RecordSupersetTitle)}" }
            if let Some(group) = group {
                div {
                    class: "flex flex-wrap items-center gap-1",
                    "data-superset-group": "{group.id}",
                    for (index, (member_id, name)) in members.iter().enumerate() {
                        span {
                            class: "text-[11px]",
                            "data-superset-member": "{member_id}",
                            "data-superset-member-index": "{index}",
                            "{name}"
                        }
                    }
                    button {
                        class: "min-h-8 border border-line px-2 text-[10px] text-primary",
                        "data-superset-delete": "true",
                        disabled,
                        onclick: move |_| {
                            if let Some(group_id) = group_id.clone() {
                                on_delete.call(group_id);
                            }
                        },
                        "{text(Key::RecordSupersetDelete)}"
                    }
                }
            }
            if !has_group && !candidates.is_empty() {
                div { class: "flex flex-wrap items-center gap-2",
                    for candidate in candidates {
                        label {
                            class: "flex items-center gap-1 text-[11px]",
                            input {
                                r#type: "checkbox",
                                "data-superset-candidate": "{candidate.training_exercise_id}",
                                "data-exercise-id": "{candidate.exercise_id}",
                                checked: selected().contains(&candidate.training_exercise_id),
                                disabled,
                                onchange: {
                                    let id = candidate.training_exercise_id.clone();
                                    move |_| toggle_selected(selected, id.clone())
                                },
                            }
                            "{candidate.name}"
                        }
                    }
                    button {
                        class: "min-h-8 border border-primary px-2 text-[10px] font-bold text-primary",
                        "data-superset-create": "true",
                        disabled: !can_create,
                        onclick: move |_| {
                            on_create.call((create_view_id.clone(), selected_for_create()));
                        },
                        "{text(Key::RecordSupersetCreate)}"
                    }
                }
            }
        }
    }
}

fn toggle_selected(mut selected: Signal<Vec<String>>, id: String) {
    let mut ids = selected();
    if let Some(index) = ids.iter().position(|value| value == &id) {
        ids.remove(index);
    } else {
        ids.push(id);
    }
    selected.set(ids);
}
