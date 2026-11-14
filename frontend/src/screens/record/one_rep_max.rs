//! セットの推定 1RM の表示。
//!
//! 対応する要求: `REQ-SET-008`。
//! 式、回数 1 の扱い、参考値の判定は `warrior_core::set_calc` に任せ、ここは表示だけを
//! 担う。値が無いときは何も表示しない。

use dioxus::prelude::*;
use warrior_core::set_calc::OneRepMax;

use crate::text::{text, Key};

/// セットの推定 1RM の表示。
#[component]
pub(super) fn OneRepMaxBadge(value: Option<OneRepMax>) -> Element {
    let Some(value) = value else {
        return rsx! {};
    };
    let label = format!("{} {:.1}", text(Key::RecordOneRepMax), value.value_kg);
    rsx! {
        span {
            class: "shrink-0 text-[10px] text-text-sub",
            "data-one-rm": "true",
            "data-one-rm-value": "{value.value_kg}",
            "data-one-rm-reference": if value.is_reference { "true" } else { "false" },
            "{label}"
            if value.is_reference {
                span {
                    class: "ml-1 text-primary",
                    "data-one-rm-reference-mark": "true",
                    "{text(Key::RecordOneRepMaxReference)}"
                }
            }
        }
    }
}
