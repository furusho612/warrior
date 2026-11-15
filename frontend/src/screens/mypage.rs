//! マイページ。プロフィール、栄養、記録、表示、データ、アカウントの設定を並べる。

use dioxus::prelude::*;
use warrior_core::nutrition::MacroTarget;
use warrior_core::profile::Profile;
use warrior_core::settings::{DisplayMode, Settings};

use crate::export::{self as export_api, ExportJob};
use crate::profile::{self as profile_api, choice_class, pressed, ProfileForm};
use crate::push::PushSubscriptionSetting;
use crate::screens::profile_form::{ProfileFields, ProfileSummary, SafetyNoticePanel};
use crate::screens::record::{RestNotificationSetting, WakeLockSetting};
use crate::screens::AccountSection;
use crate::settings as settings_api;
use crate::text::{text, Key};
use crate::theme::{self, ThemePreference};

/// マイページの 6 つの区分で共有する設定の状態。
#[derive(Clone, Copy, PartialEq)]
struct SettingsState {
    settings: Signal<Settings>,
    loaded: Signal<bool>,
    saved: Signal<u32>,
    macro_error: Signal<Option<String>>,
    record_error: Signal<Option<String>>,
    display_error: Signal<Option<String>>,
}

#[component]
pub fn MyPageScreen() -> Element {
    let state = SettingsState {
        settings: use_signal(Settings::default),
        loaded: use_signal(|| false),
        saved: use_signal(|| 0u32),
        macro_error: use_signal(|| Option::<String>::None),
        record_error: use_signal(|| Option::<String>::None),
        display_error: use_signal(|| Option::<String>::None),
    };
    let theme_preference = use_context::<ThemePreference>();
    use_effect(move || {
        spawn(async move {
            let settings = settings_api::load().await;
            let mut current = state.settings;
            current.set(settings);
            let mut loaded = state.loaded;
            loaded.set(true);
            let mut mode = theme_preference.mode;
            mode.set(settings.display_mode);
        });
    });

    rsx! {
        section {
            class: "flex min-h-full flex-col gap-4 p-5",
            "data-route": "/mypage",
            "data-settings-loaded": if (state.loaded)() { "true" } else { "false" },
            "data-settings-saved": "{(state.saved)()}",
            h1 { class: "text-lg font-bold", "{text(Key::MyPageTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::MyPageDescription)}" }
            ProfileSettings {}
            NutritionSettings { state }
            RecordSettings { state }
            DisplaySettings { state }
            DataSection {}
            AccountSection {}
        }
    }
}

#[component]
fn DataSection() -> Element {
    let job = use_signal(|| Option::<ExportJob>::None);
    let starting = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);

    rsx! {
        div {
            class: "flex flex-col gap-3 border border-line bg-surface p-3",
            "data-export-section": "true",
            h2 { class: "text-sm font-bold", "{text(Key::DataSectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::DataSectionDescription)}" }
            button {
                r#type: "button",
                class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                "data-export-start": "true",
                disabled: starting(),
                onclick: move |_| {
                    spawn(start_export(job, starting, error));
                },
                if starting() {
                    "{text(Key::ExportStarting)}"
                } else {
                    "{text(Key::ExportStart)}"
                }
            }
            if let Some(message) = error() {
                p { class: "text-[13px] text-primary", "{message}" }
            }
            if let Some(job) = job() {
                p { class: "text-[13px] leading-5", "data-export-ready": "true", "{text(Key::ExportReady)}" }
                a {
                    class: "flex min-h-11 items-center justify-center border border-primary px-3 text-[13px] font-bold text-primary",
                    href: "/api/exports/{job.id}",
                    download: "warrior-records.csv",
                    "data-export-download": "true",
                    "{text(Key::ExportDownload)}"
                }
            }
        }
    }
}

async fn start_export(
    mut job: Signal<Option<ExportJob>>,
    mut starting: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    starting.set(true);
    error.set(None);
    match export_api::start().await {
        Ok(created) => job.set(Some(created)),
        Err(_) => error.set(Some(text(Key::ExportFailed).to_owned())),
    }
    starting.set(false);
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
            "data-settings-section": "profile",
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

#[component]
fn NutritionSettings(state: SettingsState) -> Element {
    let current = (state.settings)().macro_target;
    let mut protein = use_signal(|| format_protein(current.protein_g_per_kg));
    let mut fat = use_signal(|| format_fat_percent(current.fat_ratio));
    use_effect(use_reactive((&current,), move |(target,)| {
        protein.set(format_protein(target.protein_g_per_kg));
        fat.set(format_fat_percent(target.fat_ratio));
    }));

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-settings-section": "nutrition",
            "data-nutrition-settings": "true",
            h2 { class: "text-sm font-bold", "{text(Key::NutritionSectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::NutritionSectionDescription)}" }
            label { class: "flex min-h-11 items-center justify-between gap-2 text-[13px]",
                span { "{text(Key::MacroProteinLabel)}" }
                input {
                    class: "min-h-11 w-24 border border-line bg-bg px-2 text-[13px] tabular-nums",
                    r#type: "text",
                    inputmode: "decimal",
                    "data-pfc-protein": "true",
                    value: "{protein()}",
                    oninput: move |event| {
                        let text_value = event.value();
                        let fat_value = fat();
                        protein.set(text_value.clone());
                        save_macro_texts(state, text_value, fat_value);
                    },
                }
            }
            label { class: "flex min-h-11 items-center justify-between gap-2 text-[13px]",
                span { "{text(Key::MacroFatLabel)}" }
                input {
                    class: "min-h-11 w-24 border border-line bg-bg px-2 text-[13px] tabular-nums",
                    r#type: "text",
                    inputmode: "decimal",
                    "data-pfc-fat": "true",
                    value: "{fat()}",
                    oninput: move |event| {
                        let text_value = event.value();
                        let protein_value = protein();
                        fat.set(text_value.clone());
                        save_macro_texts(state, protein_value, text_value);
                    },
                }
            }
            if let Some(message) = (state.macro_error)() {
                p {
                    class: "text-[11px] text-primary",
                    "data-pfc-error": "true",
                    "{message}"
                }
            }
            button {
                r#type: "button",
                class: "min-h-11 border border-line px-3 text-[13px]",
                "data-pfc-reset": "true",
                onclick: move |_| {
                    spawn(reset_macro(state));
                },
                "{text(Key::MacroReset)}"
            }
        }
    }
}

fn save_macro_texts(state: SettingsState, protein_text: String, fat_text: String) {
    let Ok(protein) = protein_text.trim().parse::<f64>() else {
        set_macro_error(state, Key::MacroInvalid);
        return;
    };
    let Ok(percent) = fat_text.trim().parse::<f64>() else {
        set_macro_error(state, Key::MacroInvalid);
        return;
    };
    if !(0.0..=100.0).contains(&percent) {
        set_macro_error(state, Key::MacroInvalid);
        return;
    }
    let target = MacroTarget {
        protein_g_per_kg: protein,
        fat_ratio: percent / 100.0,
    };
    if !target.is_valid() {
        set_macro_error(state, Key::MacroInvalid);
        return;
    }
    spawn(save_macro(state, target));
}

async fn save_macro(state: SettingsState, target: MacroTarget) {
    save_macro_result(state, settings_api::set_macro_target(target).await);
}

async fn reset_macro(state: SettingsState) {
    save_macro_result(state, settings_api::reset_macro_target().await);
}

#[component]
fn RecordSettings(state: SettingsState) -> Element {
    let current = (state.settings)().rest_default_seconds;
    let mut value = use_signal(|| current.to_string());
    use_effect(use_reactive((&current,), move |(seconds,)| {
        value.set(seconds.to_string());
    }));

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-settings-section": "record",
            "data-rest-settings": "true",
            "data-settings-record": "true",
            h2 { class: "text-sm font-bold", "{text(Key::RecordSectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::RecordSectionDescription)}" }
            label { class: "flex min-h-11 items-center justify-between gap-2 text-[13px]",
                span { "{text(Key::RestTimerDefault)}" }
                input {
                    class: "min-h-11 w-24 border border-line bg-bg px-2 text-[13px] tabular-nums",
                    r#type: "text",
                    inputmode: "numeric",
                    "data-rest-default": "true",
                    "data-settings-rest-default": "true",
                    "data-rest-target": "default",
                    value: "{value()}",
                    placeholder: "90",
                    oninput: move |event| {
                        let text_value = event.value();
                        value.set(text_value.clone());
                        save_rest_default(state, text_value);
                    },
                }
            }
            WakeLockSetting {}
            RestNotificationSetting {}
            PushSubscriptionSetting {}
            if let Some(message) = (state.record_error)() {
                p {
                    class: "text-[11px] text-primary",
                    "data-rest-seconds-error": "true",
                    "{message}"
                }
            }
        }
    }
}

fn save_rest_default(state: SettingsState, text_value: String) {
    let Ok(seconds) = text_value.trim().parse::<u32>() else {
        set_record_error(state, Key::RestTimerInvalid);
        return;
    };
    if !warrior_core::rest_timer::is_valid_seconds(seconds) {
        set_record_error(state, Key::RestTimerInvalid);
        return;
    }
    spawn(async move {
        save_record_result(state, settings_api::set_rest_default_seconds(seconds).await);
    });
}

#[component]
fn DisplaySettings(state: SettingsState) -> Element {
    let selected = (state.settings)().display_mode;
    let theme_preference = use_context::<ThemePreference>();
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-settings-section": "display",
            "data-display-settings": "true",
            h2 { class: "text-sm font-bold", "{text(Key::DisplaySectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::DisplaySectionDescription)}" }
            div { class: "flex flex-wrap gap-2",
                for mode in [DisplayMode::System, DisplayMode::Light, DisplayMode::Dark] {
                    {
                        let active = selected == mode;
                        let class = choice_class(active);
                        rsx! {
                            button {
                                r#type: "button",
                                class,
                                "data-display-mode-option": mode.as_str(),
                                "data-display-mode": mode.as_str(),
                                "aria-pressed": pressed(active),
                                onclick: move |_| {
                                    let mut theme_mode = theme_preference.mode;
                                    theme_mode.set(mode);
                                    theme::remember_display_mode(mode);
                                    theme::apply_display_mode(mode);
                                    spawn(async move {
                                        save_display_result(
                                            state,
                                            settings_api::set_display_mode(mode).await,
                                        );
                                    });
                                },
                                "{text(display_mode_key(mode))}"
                            }
                        }
                    }
                }
            }
            if let Some(message) = (state.display_error)() {
                p {
                    class: "text-[11px] text-primary",
                    "data-display-error": "true",
                    "{message}"
                }
            }
        }
    }
}

#[component]
fn DataSettings() -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-settings-section": "data",
            h2 { class: "text-sm font-bold", "{text(Key::DataSectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::DataSectionDescription)}" }
        }
    }
}

fn display_mode_key(mode: DisplayMode) -> Key {
    match mode {
        DisplayMode::System => Key::DisplayModeSystem,
        DisplayMode::Light => Key::DisplayModeLight,
        DisplayMode::Dark => Key::DisplayModeDark,
    }
}

fn save_macro_result(state: SettingsState, result: Result<Settings, settings_api::UpdateError>) {
    match result {
        Ok(settings) => {
            apply_saved(state, settings);
            let mut error = state.macro_error;
            error.set(None);
        }
        Err(error) => set_macro_error(state, settings_api::update_error_key(error)),
    }
}

fn save_record_result(state: SettingsState, result: Result<Settings, settings_api::UpdateError>) {
    match result {
        Ok(settings) => {
            apply_saved(state, settings);
            let mut error = state.record_error;
            error.set(None);
        }
        Err(error) => set_record_error(state, settings_api::update_error_key(error)),
    }
}

fn save_display_result(state: SettingsState, result: Result<Settings, settings_api::UpdateError>) {
    match result {
        Ok(settings) => {
            apply_saved(state, settings);
            let mut error = state.display_error;
            error.set(None);
        }
        Err(error) => set_display_error(state, settings_api::update_error_key(error)),
    }
}

fn apply_saved(state: SettingsState, settings: Settings) {
    let mut current = state.settings;
    current.set(settings);
    let mut saved = state.saved;
    saved += 1;
}

fn set_macro_error(state: SettingsState, key: Key) {
    let mut error = state.macro_error;
    error.set(Some(text(key).to_owned()));
}

fn set_record_error(state: SettingsState, key: Key) {
    let mut error = state.record_error;
    error.set(Some(text(key).to_owned()));
}

fn set_display_error(state: SettingsState, key: Key) {
    let mut error = state.display_error;
    error.set(Some(text(key).to_owned()));
}

fn format_protein(value: f64) -> String {
    format!("{value:.1}")
}

fn format_fat_percent(value: f64) -> String {
    format!("{:.0}", value * 100.0)
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
