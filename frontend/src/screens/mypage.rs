//! マイページ。身体情報と目的の設定、パスキーの管理を並べる。

use dioxus::prelude::*;
use warrior_core::profile::Profile;

use crate::profile::{self as profile_api, ProfileForm};
use crate::screens::profile_form::{ProfileFields, ProfileSummary, SafetyNoticePanel};
use crate::screens::AccountSection;
use crate::text::{text, Key};

#[component]
pub fn MyPageScreen() -> Element {
    rsx! {
        section {
            class: "flex min-h-full flex-col gap-4 p-5",
            "data-route": "/mypage",
            h1 { class: "text-lg font-bold", "{text(Key::MyPageTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::MyPageDescription)}" }
            ProfileSettings {}
            AccountSection {}
        }
    }
}

#[component]
fn ProfileSettings() -> Element {
    let form = use_signal(ProfileForm::default);
    let saved = use_signal(|| Option::<Profile>::None);
    let loaded = use_signal(|| false);
    let reload = use_signal(|| 0u32);
    let load_error = use_signal(|| Option::<String>::None);
    let save_error = use_signal(|| Option::<String>::None);
    let saving = use_signal(|| false);
    let save_count = use_signal(|| 0u32);

    use_effect(move || {
        let _ = reload();
        spawn(load_profile(form, saved, loaded, load_error));
    });

    let saved_profile = saved();
    let editable = loaded() && load_error().is_none() && !form().is_empty();

    rsx! {
        div {
            class: "flex flex-col gap-3 border border-line bg-surface p-3",
            "data-profile-settings": "true",
            "data-profile-loaded": if loaded() { "true" } else { "false" },
            "data-profile-saved": "{save_count()}",
            h2 { class: "text-sm font-bold", "{text(Key::ProfileSectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::ProfileSectionDescription)}" }
            if let Some(profile) = saved_profile {
                ProfileSummary { profile: profile.clone() }
                SafetyNoticePanel { notice: profile_api::today().map_or(Default::default(), |today| profile.safety_notice_on(today)) }
            }
            ProfileFields { form, show_settings: true }
            if let Some(message) = load_error() {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    r#type: "button",
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-profile-retry": "true",
                    onclick: move |_| {
                        let mut reload = reload;
                        reload += 1;
                    },
                    "{text(Key::ProfileRetry)}"
                }
            }
            if let Some(message) = save_error() {
                p { class: "text-[13px] text-primary", "{message}" }
            }
            button {
                r#type: "button",
                class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                "data-profile-save": "true",
                disabled: saving() || !editable,
                onclick: move |_| {
                    spawn(save_profile(form, saving, save_error, saved, save_count));
                },
                if saving() {
                    "{text(Key::ProfileSaving)}"
                } else {
                    "{text(Key::ProfileSave)}"
                }
            }
        }
    }
}

async fn load_profile(
    mut form: Signal<ProfileForm>,
    mut saved: Signal<Option<Profile>>,
    mut loaded: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    loaded.set(false);
    error.set(None);
    match profile_api::fetch().await {
        Ok(profile) => {
            form.set(
                profile
                    .as_ref()
                    .map(ProfileForm::from_profile)
                    .unwrap_or_default(),
            );
            saved.set(profile);
            loaded.set(true);
        }
        Err(_) => error.set(Some(text(Key::ProfileLoadFailed).to_owned())),
    }
}

async fn save_profile(
    mut form: Signal<ProfileForm>,
    mut saving: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut saved: Signal<Option<Profile>>,
    mut save_count: Signal<u32>,
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
            save_count += 1;
        }
        Err(_) => error.set(Some(text(Key::ProfileSaveFailed).to_owned())),
    }
    saving.set(false);
}
