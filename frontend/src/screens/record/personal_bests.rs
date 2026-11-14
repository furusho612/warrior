//! セットの完了で更新した自己ベストの表示。
//!
//! 対応する要求: `REQ-SET-009`。
//! セットの完了の応答で受け取った項目を、記録中のトレーニングの画面へ並べる。
//! 種類の名前と値の書式はサマリーと同じものを使う。

use dioxus::prelude::*;
use warrior_core::training_session::PersonalBestItem;

use crate::screens::summary::{best_equipment, best_label, best_value};
use crate::text::{text, Key};

/// 記録中に更新した自己ベストの一覧。
#[component]
pub(super) fn RecordPersonalBests(items: Vec<PersonalBestItem>) -> Element {
    if items.is_empty() {
        return rsx! {};
    }
    rsx! {
        section {
            class: "flex flex-col gap-1 border border-line bg-surface p-3",
            "data-record-personal-bests": "true",
            h2 { class: "text-sm font-bold", "{text(Key::SummaryPersonalBests)}" }
            ul { class: "flex flex-col gap-1",
                for item in items {
                    li {
                        class: "flex min-h-9 items-center gap-2 border-b border-line",
                        "data-record-personal-best": item.kind.as_str(),
                        "data-record-personal-best-value": "{item.value}",
                        if let Some(name) = item.exercise_name.clone() {
                            span {
                                class: "min-w-0 flex-1 truncate text-[13px]",
                                "data-record-personal-best-name": "true",
                                "{name}"
                            }
                        } else {
                            span { class: "min-w-0 flex-1" }
                        }
                        if let Some(equipment) = best_equipment(&item) {
                            span {
                                class: "shrink-0 text-[10px] text-text-sub",
                                "data-record-personal-best-equipment": "true",
                                "{equipment}"
                            }
                        }
                        span {
                            class: "shrink-0 text-[11px] text-text-sub",
                            "data-record-personal-best-kind": "true",
                            "{best_label(item.kind)}"
                        }
                        span {
                            class: "shrink-0 text-right text-[13px] font-bold",
                            "data-record-personal-best-amount": "true",
                            "{best_value(&item)}"
                        }
                    }
                }
            }
        }
    }
}
