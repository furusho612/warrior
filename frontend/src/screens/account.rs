//! アカウントの画面。Google アカウントの表示、パスキーの一覧、ログアウト、
//! アカウントの削除の確認と端末の後始末を置く。

use dioxus::prelude::*;
use dioxus_router::Link;
use serde::{Deserialize, Serialize};

use crate::http;
use crate::passkey;
use crate::routes::Route;
use crate::storage;
use crate::text::{text, Key};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct AccountInfo {
    display_name: String,
    email: String,
}

#[derive(Debug, Serialize)]
struct DeleteConfirm {
    confirm: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeleteState {
    Idle,
    Confirming,
    Deleting,
    Cleaning,
    RequestFailed,
    CleanupFailed,
    Done,
}

/// マイページのアカウントの区分。アカウントの画面への入口を置く。
#[component]
pub fn AccountSection() -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-3 border border-line bg-surface p-3",
            "data-account-section": "true",
            h2 { class: "text-sm font-bold", "{text(Key::AccountSectionTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::AccountSectionDescription)}" }
            Link {
                to: Route::Account {},
                class: "flex min-h-11 items-center justify-center border border-primary px-3 text-[13px] font-bold text-primary",
                "{text(Key::AccountOpen)}"
            }
        }
    }
}

/// アカウントの画面。
#[component]
pub fn AccountScreen() -> Element {
    rsx! {
        section {
            class: "flex min-h-full flex-col gap-4 p-5",
            "data-route": "/account",
            "data-account-settings": "true",
            h1 { class: "text-lg font-bold", "{text(Key::AccountTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::AccountDescription)}" }
            AccountInfoPanel {}
            AccountSettings {}
            LogoutPanel {}
            DeleteAccountPanel {}
        }
    }
}

#[component]
fn AccountInfoPanel() -> Element {
    let info = use_signal(|| Option::<AccountInfo>::None);
    let error = use_signal(|| Option::<String>::None);
    let reload = use_signal(|| 0u32);

    use_effect(move || {
        let _ = reload();
        spawn(load_account(info, error));
    });

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-account-info": "true",
            h2 { class: "text-sm font-bold", "{text(Key::AccountGoogleTitle)}" }
            if let Some(account) = info() {
                dl { class: "flex flex-col gap-1 text-[13px]",
                    div { class: "flex justify-between gap-3",
                        dt { class: "text-text-sub", "{text(Key::AccountDisplayNameLabel)}" }
                        dd {
                            class: "truncate font-bold",
                            "data-account-display-name": "true",
                            "{account.display_name}"
                        }
                    }
                    div { class: "flex justify-between gap-3",
                        dt { class: "text-text-sub", "{text(Key::AccountEmailLabel)}" }
                        dd {
                            class: "truncate",
                            "data-account-email": "true",
                            "{account.email}"
                        }
                    }
                }
            } else if let Some(message) = error() {
                p { class: "text-[13px] text-primary", "{message}" }
                button {
                    r#type: "button",
                    class: "min-h-11 border border-line px-3 text-[13px]",
                    "data-account-retry": "true",
                    onclick: move |_| {
                        let mut reload = reload;
                        reload += 1;
                    },
                    "{text(Key::AccountRetry)}"
                }
            } else {
                p { class: "text-[13px] text-text-sub", "{text(Key::AccountLoading)}" }
            }
        }
    }
}

#[component]
fn LogoutPanel() -> Element {
    let logging_out = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);

    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-account-logout-panel": "true",
            button {
                r#type: "button",
                class: "min-h-11 border border-line px-3 text-[13px]",
                "data-account-logout": "true",
                disabled: logging_out(),
                onclick: move |_| {
                    spawn(logout(logging_out, error));
                },
                if logging_out() {
                    "{text(Key::LogoutInProgress)}"
                } else {
                    "{text(Key::LogoutButton)}"
                }
            }
            if let Some(message) = error() {
                p {
                    class: "text-[13px] text-primary",
                    "data-account-logout-error": "true",
                    "{message}"
                }
            }
        }
    }
}

#[component]
fn DeleteAccountPanel() -> Element {
    let state = use_signal(|| DeleteState::Idle);
    let current = state();

    rsx! {
        div {
            class: "flex flex-col gap-3 border border-line bg-surface p-3",
            "data-account-delete-panel": "true",
            h2 { class: "text-sm font-bold", "{text(Key::DeleteAccountTitle)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::DeleteAccountDescription)}" }
            if current == DeleteState::Idle {
                button {
                    r#type: "button",
                    class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                    "data-account-delete": "true",
                    onclick: move |_| {
                        let mut state = state;
                        state.set(DeleteState::Confirming);
                    },
                    "{text(Key::DeleteAccountButton)}"
                }
            }
            if current == DeleteState::Confirming {
                DeleteConfirmation {
                    state,
                    on_confirm: move |_| {
                        spawn(delete_account(state));
                    },
                }
            }
            if current == DeleteState::Deleting {
                p { class: "text-[13px] text-text-sub", "{text(Key::DeleteAccountDeleting)}" }
            }
            if current == DeleteState::Cleaning {
                p { class: "text-[13px] text-text-sub", "{text(Key::DeleteAccountCleaning)}" }
            }
            if current == DeleteState::RequestFailed {
                DeleteRetry {
                    message: text(Key::DeleteAccountFailed).to_owned(),
                    on_retry: move |_| {
                        spawn(delete_account(state));
                    },
                }
            }
            if current == DeleteState::CleanupFailed {
                DeleteRetry {
                    message: text(Key::DeleteAccountCleanupFailed).to_owned(),
                    on_retry: move |_| {
                        spawn(cleanup_after_delete(state));
                    },
                }
            }
            if current == DeleteState::Done {
                p { class: "text-[13px] text-text-sub", "{text(Key::DeleteAccountDone)}" }
            }
        }
    }
}

#[component]
fn DeleteConfirmation(state: Signal<DeleteState>, on_confirm: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-primary p-3",
            "data-account-delete-confirmation": "true",
            h3 { class: "text-sm font-bold", "{text(Key::DeleteAccountConfirmTitle)}" }
            p { class: "text-[13px] leading-5", "{text(Key::DeleteAccountDataWarning)}" }
            p { class: "text-[13px] leading-5 text-primary", "{text(Key::DeleteAccountIrreversible)}" }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::DeleteAccountBackup)}" }
            button {
                r#type: "button",
                class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                "data-account-delete-confirm": "true",
                onclick: move |_| on_confirm.call(()),
                "{text(Key::DeleteAccountConfirm)}"
            }
            button {
                r#type: "button",
                class: "min-h-11 border border-line px-3 text-[13px]",
                "data-account-delete-cancel": "true",
                onclick: move |_| {
                    let mut state = state;
                    state.set(DeleteState::Idle);
                },
                "{text(Key::DeleteAccountCancel)}"
            }
        }
    }
}

#[component]
fn DeleteRetry(message: String, on_retry: EventHandler<()>) -> Element {
    rsx! {
        p {
            class: "text-[13px] leading-5 text-primary",
            "data-account-delete-error": "true",
            "{message}"
        }
        button {
            r#type: "button",
            class: "min-h-11 border border-line px-3 text-[13px]",
            "data-account-delete-retry": "true",
            onclick: move |_| on_retry.call(()),
            "{text(Key::DeleteAccountRetry)}"
        }
    }
}

async fn load_account(mut info: Signal<Option<AccountInfo>>, mut error: Signal<Option<String>>) {
    info.set(None);
    error.set(None);
    match http::request_json::<AccountInfo>("GET", "/api/me").await {
        Ok(account) => info.set(Some(account)),
        Err(_) => error.set(Some(text(Key::AccountLoadFailed).to_owned())),
    }
}

async fn logout(mut logging_out: Signal<bool>, mut error: Signal<Option<String>>) {
    logging_out.set(true);
    error.set(None);
    match http::request_status("POST", "/api/auth/logout").await {
        Ok(status) if (200..300).contains(&status) => {
            crate::settings::forget_session().await;
            dioxus_router::router().push(Route::Login {});
            return;
        }
        _ => error.set(Some(text(Key::LogoutFailed).to_owned())),
    }
    logging_out.set(false);
}

async fn delete_account(mut state: Signal<DeleteState>) {
    if matches!(
        state(),
        DeleteState::Deleting | DeleteState::Cleaning | DeleteState::Done
    ) {
        return;
    }
    state.set(DeleteState::Deleting);
    let confirm = DeleteConfirm {
        confirm: "delete-account",
    };
    match http::request_status_with_body("DELETE", "/api/account", &confirm).await {
        Ok(204) => {}
        _ => {
            state.set(DeleteState::RequestFailed);
            return;
        }
    }
    cleanup_after_delete(state).await;
}

async fn cleanup_after_delete(mut state: Signal<DeleteState>) {
    if state() == DeleteState::Cleaning {
        return;
    }
    state.set(DeleteState::Cleaning);
    if storage::clear_after_deletion().await.is_err() {
        state.set(DeleteState::CleanupFailed);
        return;
    }
    state.set(DeleteState::Done);
    dioxus_router::router().push(Route::Login {});
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct PasskeyItem {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct PasskeyList {
    passkeys: Vec<PasskeyItem>,
}

#[derive(Debug, Deserialize)]
struct ApiResult {
    status: u16,
    body: Option<serde_json::Value>,
}

#[component]
pub fn AccountSettings() -> Element {
    let passkeys = use_signal(Vec::<PasskeyItem>::new);
    let message = use_signal(String::new);
    let needs_reauth = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            refresh(passkeys, message, needs_reauth).await;
        });
    });

    rsx! {
        div { class: "flex flex-col gap-3", "data-passkey-settings": "true",
            div { class: "flex flex-col gap-2 border border-line bg-surface p-3",
                h2 { class: "text-sm font-bold", "{text(Key::PasskeyTitle)}" }
                p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::PasskeyDescription)}" }
                if passkeys().is_empty() {
                    p { class: "text-[13px] text-text-sub", "{text(Key::PasskeyEmpty)}" }
                } else {
                    ul { class: "flex flex-col gap-2",
                        for item in passkeys() {
                            li {
                                class: "flex min-h-11 items-center justify-between gap-2 border border-line px-3",
                                span { class: "truncate text-[13px]", "{item.name}" }
                                button {
                                    class: "min-h-11 border border-line px-3 text-[13px] text-primary",
                                    onclick: {
                                        let id = item.id.clone();
                                        move |_| {
                                            let id = id.clone();
                                            spawn(async move {
                                                remove(id, passkeys, message, needs_reauth).await;
                                            });
                                        }
                                    },
                                    "{text(Key::PasskeyDelete)}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex flex-wrap gap-2",
                    button {
                        class: "min-h-11 border border-primary bg-primary px-3 text-[13px] font-bold text-on-primary",
                        onclick: move |_| {
                            spawn(async move {
                                add(passkeys, message, needs_reauth).await;
                            });
                        },
                        "{text(Key::PasskeyAdd)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| {
                            spawn(async move {
                                reauthenticate(message, needs_reauth).await;
                            });
                        },
                        "{text(Key::PasskeyPasskeyReauth)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-3 text-[13px]",
                        onclick: move |_| {
                            if let Some(window) = web_sys::window() {
                                let _ = window
                                    .location()
                                    .set_href("/api/auth/reauthenticate/google/start");
                            }
                        },
                        "{text(Key::PasskeyGoogleReauth)}"
                    }
                }
                if needs_reauth() {
                    p { class: "text-[13px] text-primary", "{text(Key::PasskeyNeedsReauth)}" }
                }
                if !message().is_empty() {
                    p { class: "text-[13px] text-text-sub", "{message()}" }
                }
            }
            p { class: "text-[13px] leading-5 text-text-sub", "{text(Key::PasskeyRecovery)}" }
        }
    }
}

async fn refresh(
    mut passkeys: Signal<Vec<PasskeyItem>>,
    mut message: Signal<String>,
    mut needs_reauth: Signal<bool>,
) {
    match passkey::call("list", "").await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                if let Some(body) = result.body {
                    if let Ok(list) = serde_json::from_value::<PasskeyList>(body) {
                        passkeys.set(list.passkeys);
                    }
                }
                needs_reauth.set(false);
            }
            Ok(result) if result.status == 401 => {
                message.set(text(Key::LoginError).to_owned());
            }
            Ok(result) if result.status == 403 => {
                needs_reauth.set(true);
            }
            _ => message.set(text(Key::PasskeyFailed).to_owned()),
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}

async fn add(
    passkeys: Signal<Vec<PasskeyItem>>,
    mut message: Signal<String>,
    mut needs_reauth: Signal<bool>,
) {
    match passkey::call("register", text(Key::PasskeyTitle)).await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                message.set(text(Key::PasskeyRegistered).to_owned());
                refresh(passkeys, message, needs_reauth).await;
            }
            Ok(result) if result.status == 403 => {
                needs_reauth.set(true);
            }
            _ => message.set(text(Key::PasskeyFailed).to_owned()),
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}

async fn remove(
    id: String,
    passkeys: Signal<Vec<PasskeyItem>>,
    mut message: Signal<String>,
    mut needs_reauth: Signal<bool>,
) {
    match passkey::call("remove", &id).await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                message.set(text(Key::PasskeyDeleted).to_owned());
                refresh(passkeys, message, needs_reauth).await;
            }
            Ok(result) if result.status == 403 => {
                needs_reauth.set(true);
            }
            _ => message.set(text(Key::PasskeyFailed).to_owned()),
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}

async fn reauthenticate(mut message: Signal<String>, mut needs_reauth: Signal<bool>) {
    match passkey::call("reauthenticate", "").await {
        Ok(result_json) => match serde_json::from_str::<ApiResult>(&result_json) {
            Ok(result) if result.status == 200 => {
                needs_reauth.set(false);
                message.set(text(Key::PasskeyReauthenticated).to_owned());
            }
            _ => {
                needs_reauth.set(true);
                message.set(text(Key::PasskeyFailed).to_owned());
            }
        },
        Err(_) => message.set(text(Key::PasskeyFailed).to_owned()),
    }
}
