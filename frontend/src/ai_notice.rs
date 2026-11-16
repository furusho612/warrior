//! AI の評価と提案のプライバシー通知 (REQ-NFR-016)。
//!
//! 要約は常に表示し、詳細は利用者が開いたときに表示する。モーダルで操作を遮らない。
//! 通知の文面は `frontend/src/text.rs` に置き、画面ごとに同じ内容を表示する。

use dioxus::prelude::*;

use crate::text::{text, Key};

/// AI に送る情報と、送らない情報、学習と保存の扱いを表示する。
#[component]
pub fn AiPrivacyNotice() -> Element {
    let mut open = use_signal(|| false);
    let opened = open();
    rsx! {
        div {
            class: "flex flex-col gap-1 border border-line bg-surface p-3",
            "data-ai-notice": "true",
            "data-ai-notice-open": if opened { "true" } else { "false" },
            p {
                class: "text-[11px] leading-5 text-text-sub",
                "data-ai-notice-summary": "true",
                "{text(Key::AiNoticeSummary)}"
            }
            button {
                r#type: "button",
                class: "min-h-11 self-start text-[11px] text-primary",
                "data-ai-notice-toggle": "true",
                onclick: move |_| {
                    let next = !(open)();
                    open.set(next);
                },
                if opened {
                    "{text(Key::AiNoticeClose)}"
                } else {
                    "{text(Key::AiNoticeDetails)}"
                }
            }
            if opened {
                ul {
                    class: "flex list-disc flex-col gap-1 pl-4 text-[11px] leading-5 text-text-sub",
                    "data-ai-notice-details": "true",
                    li { "{text(Key::AiNoticeSent)}" }
                    li { "{text(Key::AiNoticeNotSent)}" }
                    li { "{text(Key::AiNoticeProvider)}" }
                    li { "{text(Key::AiNoticeTraining)}" }
                    li { "{text(Key::AiNoticeStorage)}" }
                }
            }
        }
    }
}
