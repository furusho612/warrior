//! 初回の身体情報の登録と、入力をあとで行う操作。
//!
//! 登録した直後に、満年齢 18 歳未満または BMI 18.5 未満の利用者へ注意書きと
//! 相談のガイドを表示する (REQ-NFR-018)。

use dioxus::prelude::*;
use warrior_core::profile::Profile;

use crate::profile::{self as profile_api, ProfileForm};
use crate::screens::profile_form::{ProfileFields, ProfileSummary, SafetyNoticePanel};
use crate::text::{text, Key};

#[component]
pub fn OnboardingPanel(on_done: EventHandler<()>) -> Element {
    let form = use_signal(ProfileForm::default);
    let saving = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let saved = use_signal(|| Option::<Profile>::None);
    let saved_profile = saved();
    let has_saved = saved_profile.is_some();
    let empty = form().is_empty();

    rsx! {
        section {
            class: "flex flex-col gap-3 border border-line bg-surface p-4",
            "data-onboarding": "true",
            "data-onboarding-saved": if has_saved { "true" } else { "false" },
            h2 { class: "text-sm font-bold", "{text(Key::OnboardingTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::OnboardingDescription)}" }
            if let Some(profile) = saved_profile {
                p { class: "text-[13px] text-primary", "{text(Key::OnboardingSaved)}" }
                ProfileSummary { profile: profile.clone() }
                SafetyNoticePanel { notice: profile_api::today().map_or(Default::default(), |today| profile.safety_notice_on(today)) }
                button {
                    r#type: "button",
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-onboarding-done": "true",
                    onclick: move |_| on_done.call(()),
                    "{text(Key::OnboardingDone)}"
                }
            } else {
                ProfileFields { form, show_settings: false }
                if let Some(message) = error() {
                    p { class: "text-[13px] text-primary", "{message}" }
                }
                div { class: "flex flex-wrap gap-2",
                    button {
                        r#type: "button",
                        class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                        "data-onboarding-save": "true",
                        disabled: saving() || empty,
                        onclick: move |_| {
                            spawn(register(form, saving, error, saved));
                        },
                        if saving() {
                            "{text(Key::OnboardingSaving)}"
                        } else {
                            "{text(Key::OnboardingSave)}"
                        }
                    }
                    button {
                        r#type: "button",
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        "data-onboarding-skip": "true",
                        onclick: move |_| on_done.call(()),
                        "{text(Key::OnboardingSkip)}"
                    }
                }
            }
        }
    }
}

async fn register(
    mut form: Signal<ProfileForm>,
    mut saving: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut saved: Signal<Option<Profile>>,
) {
    let profile = match form()
        .to_profile()
        .and_then(|profile| profile_api::validate(&profile).map(|_| profile))
    {
        Ok(profile) => profile,
        Err(key) => {
            error.set(Some(text(key).to_owned()));
            return;
        }
    };
    saving.set(true);
    error.set(None);
    match profile_api::save(&profile).await {
        Ok(saved_profile) => {
            form.set(ProfileForm::from_profile(&saved_profile));
            saved.set(Some(saved_profile));
        }
        Err(_) => error.set(Some(text(Key::OnboardingFailed).to_owned())),
    }
    saving.set(false);
}
