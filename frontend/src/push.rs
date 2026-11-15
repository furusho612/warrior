//! プッシュ通知の購読と解除、iOS のホーム画面への追加の案内 (REQ-TMR-008、REQ-NFR-008)。
//!
//! 通知の許可は、利用者が記録画面の設定でプッシュ通知を有効にしたときにだけ求める。
//! 購読の情報は endpoint と 2 つの鍵をサーバーへ送り、サーバーは利用者と端末トークンに
//! 紐づけて保存する。購読に失敗しても、レストタイマーは音と画面表示で動く。
//! iOS でホーム画面に追加していない場合だけ、追加の手順を案内する (ADR-008、ADR-020)。

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    PushEncryptionKeyName, PushSubscription, PushSubscriptionOptionsInit, ServiceWorkerContainer,
    ServiceWorkerRegistration,
};

use warrior_core::base64url;
use warrior_core::notification::Permission;

use crate::http::{self, HttpError};
use crate::notification;
use crate::pwa;
use crate::text::{text, Key};

/// 購読と解除の失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationError {
    /// 通知の API が無い。
    Unsupported,
    /// 通知が許可されていない。
    Permission(Permission),
    /// Service Worker かプッシュの API が無い。
    Unavailable,
    /// サーバーが購読を受け付けない。
    Rejected,
    /// 通信できない、または応答が不正である。
    Failed,
}

impl RegistrationError {
    fn message_key(self) -> Key {
        match self {
            Self::Unsupported => Key::PushNotificationUnsupported,
            Self::Permission(Permission::Denied) => Key::RestNotificationDenied,
            Self::Permission(Permission::Default) => Key::RestNotificationPending,
            Self::Permission(Permission::Granted) => Key::PushNotificationFailed,
            Self::Unavailable => Key::PushNotificationUnsupported,
            Self::Rejected | Self::Failed => Key::PushNotificationFailed,
        }
    }
}

impl From<HttpError> for RegistrationError {
    fn from(error: HttpError) -> Self {
        match error {
            HttpError::Rejected => Self::Rejected,
            HttpError::InvalidRequest => Self::Failed,
            HttpError::Unavailable | HttpError::InvalidResponse => Self::Failed,
        }
    }
}

#[derive(Debug, Serialize)]
struct SubscriptionRequest<'a> {
    endpoint: &'a str,
    p256dh: &'a str,
    auth: &'a str,
}

#[derive(Debug, Serialize)]
struct EndpointRequest<'a> {
    endpoint: &'a str,
}

#[derive(Debug, Deserialize)]
struct PublicKey {
    public_key: String,
}

/// プッシュ通知を購読し、サーバーへ登録する。通知の許可が無い場合は何も登録しない。
pub async fn subscribe() -> Result<(), RegistrationError> {
    match notification::request_permission().await {
        Some(Permission::Granted) => {}
        Some(permission) => return Err(RegistrationError::Permission(permission)),
        None => return Err(RegistrationError::Unsupported),
    }
    let registration = registration().await?;
    let public_key = public_key().await?;
    let application_server_key_bytes =
        base64url::decode(&public_key).ok_or(RegistrationError::Unavailable)?;
    let options = PushSubscriptionOptionsInit::new();
    options.set_user_visible_only(true);
    let key_length = u32::try_from(application_server_key_bytes.len())
        .map_err(|_| RegistrationError::Unavailable)?;
    let application_server_key = js_sys::Uint8Array::new_with_length(key_length);
    application_server_key.copy_from(&application_server_key_bytes);
    options.set_application_server_key_opt_u8_array(Some(&application_server_key));
    let manager = registration
        .push_manager()
        .map_err(|_| RegistrationError::Unavailable)?;
    let subscription = manager
        .subscribe_with_options(&options)
        .map_err(|_| RegistrationError::Unavailable)?;
    let subscription = JsFuture::from(subscription)
        .await
        .map_err(|_| RegistrationError::Failed)?
        .dyn_into::<PushSubscription>()
        .map_err(|_| RegistrationError::Unavailable)?;
    let endpoint = subscription.endpoint();
    let p256dh = encoded_key(&subscription, PushEncryptionKeyName::P256dh)?;
    let auth = encoded_key(&subscription, PushEncryptionKeyName::Auth)?;
    let request = SubscriptionRequest {
        endpoint: &endpoint,
        p256dh: &p256dh,
        auth: &auth,
    };
    http::send_json_without_response("POST", "/api/push/subscriptions", &request)
        .await
        .map_err(RegistrationError::from)
}

/// 端末の購読を解除し、サーバーから登録を消す。購読が無い場合は何もしない。
pub async fn unsubscribe() -> Result<(), RegistrationError> {
    let registration = registration().await?;
    let manager = registration
        .push_manager()
        .map_err(|_| RegistrationError::Unavailable)?;
    let subscription = JsFuture::from(
        manager
            .get_subscription()
            .map_err(|_| RegistrationError::Unavailable)?,
    )
    .await
    .map_err(|_| RegistrationError::Failed)?;
    if subscription.is_null() || subscription.is_undefined() {
        return Ok(());
    }
    let subscription: PushSubscription = subscription.unchecked_into();
    let endpoint = subscription.endpoint();
    // 端末側の解除に失敗しても、サーバーの購読を消せば以後の通知は届かない。
    if let Ok(promise) = subscription.unsubscribe() {
        let _ = JsFuture::from(promise).await;
    }
    let request = EndpointRequest {
        endpoint: &endpoint,
    };
    http::send_json_without_response("DELETE", "/api/push/subscriptions", &request)
        .await
        .map_err(RegistrationError::from)
}

async fn registration() -> Result<ServiceWorkerRegistration, RegistrationError> {
    let window = web_sys::window().ok_or(RegistrationError::Unavailable)?;
    let container = window.navigator().service_worker();
    let ready =
        ServiceWorkerContainer::ready(&container).map_err(|_| RegistrationError::Unavailable)?;
    let ready = JsFuture::from(ready)
        .await
        .map_err(|_| RegistrationError::Unavailable)?;
    ready
        .dyn_into::<ServiceWorkerRegistration>()
        .map_err(|_| RegistrationError::Unavailable)
}

async fn public_key() -> Result<String, RegistrationError> {
    let response: PublicKey = http::request_json("GET", "/api/push/public-key")
        .await
        .map_err(RegistrationError::from)?;
    Ok(response.public_key)
}

fn encoded_key(
    subscription: &PushSubscription,
    name: PushEncryptionKeyName,
) -> Result<String, RegistrationError> {
    let key = subscription
        .get_key(name)
        .map_err(|_| RegistrationError::Unavailable)?
        .ok_or(RegistrationError::Unavailable)?;
    Ok(base64url::encode(
        &js_sys::Uint8Array::new(key.unchecked_ref()).to_vec(),
    ))
}

/// プッシュ通知の購読を有効にする設定。iOS ではホーム画面への追加を案内する。
#[component]
pub fn PushSubscriptionSetting() -> Element {
    let mut enabled = use_signal(|| false);
    let busy = use_signal(|| false);
    let mut message = use_signal(|| Option::<String>::None);
    let install_hint = use_signal(pwa::ios_install_hint);
    let enabled_value = enabled();
    rsx! {
        div {
            class: "flex flex-col gap-1",
            "data-push-setting": "true",
            "data-push-enabled": if enabled_value { "true" } else { "false" },
            label {
                class: "flex min-h-11 items-center gap-2 text-[13px]",
                input {
                    r#type: "checkbox",
                    class: "h-5 w-5",
                    "data-push-subscription": "true",
                    checked: enabled_value,
                    disabled: busy(),
                    onchange: move |event| {
                        let next = event.checked();
                        message.set(None);
                        if !next {
                            enabled.set(false);
                            spawn(disable_push(busy, message));
                            return;
                        }
                        spawn(enable_push(busy, message, enabled));
                    },
                }
                "{text(Key::PushNotificationLabel)}"
            }
            if let Some(value) = message() {
                p {
                    class: "text-[11px] text-primary",
                    "data-push-message": "true",
                    "{value}"
                }
            }
            if install_hint() {
                p {
                    class: "text-[11px] leading-4 text-text-sub",
                    "data-push-ios-hint": "true",
                    "{text(Key::IosInstallHint)}"
                }
            }
        }
    }
}

async fn enable_push(
    mut busy: Signal<bool>,
    mut message: Signal<Option<String>>,
    mut enabled: Signal<bool>,
) {
    busy.set(true);
    match subscribe().await {
        Ok(()) => enabled.set(true),
        Err(error) => {
            enabled.set(false);
            message.set(Some(text(error.message_key()).to_owned()));
        }
    }
    busy.set(false);
}

async fn disable_push(mut busy: Signal<bool>, mut message: Signal<Option<String>>) {
    busy.set(true);
    if unsubscribe().await.is_err() {
        message.set(Some(
            text(Key::PushNotificationUnsubscribeFailed).to_owned(),
        ));
    }
    busy.set(false);
}
