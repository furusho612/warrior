//! レスト終了のブラウザ通知 (REQ-TMR-007、REQ-NFR-008)。
//!
//! 通知の許可は、利用者が記録画面の設定で有効にしたときにだけ求める。画面の読み込み時には
//! 求めない。通知は、許可されていて画面が隠れている場合にだけ出し、押すと開いている画面へ
//! フォーカスする。通知の API が無い場合と通知に失敗した場合も、終了の音と画面表示は止めない。
//! 通知を出す条件は `warrior_core::notification` の純粋な関数が持つ。

use js_sys::{Array, Function, Object, Promise, Reflect};
use warrior_core::notification::{can_notify, should_notify, PageVisibility, Permission};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::sound;
use crate::text::{text, Key};

/// ブラウザ通知の題名と本文のキー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Content {
    /// 題名。
    pub title: Key,
    /// 本文。
    pub body: Key,
}

/// レスト終了の通知の題名と本文。記録の内容を含めない固定の文言を返す。
pub fn content() -> Content {
    Content {
        title: Key::RestNotificationTitle,
        body: Key::RestNotificationBody,
    }
}

/// 現在の通知の許可を読む。通知の API が無い場合は None。
pub fn permission() -> Option<Permission> {
    let constructor = notification_constructor()?;
    let value = Reflect::get(&constructor, &JsValue::from_str("permission")).ok()?;
    Permission::parse(value.as_string()?.as_str())
}

/// 通知の許可を求める。通知の API が無い場合と要求に失敗した場合は None。
pub async fn request_permission() -> Option<Permission> {
    let constructor = notification_constructor()?;
    let request = Reflect::get(&constructor, &JsValue::from_str("requestPermission")).ok()?;
    let request = request.dyn_ref::<Function>()?;
    let promise = request.call0(&constructor).ok()?;
    let promise: Promise = promise.dyn_into().ok()?;
    let value = JsFuture::from(promise).await.ok()?;
    Permission::parse(value.as_string()?.as_str())
}

/// レスト終了のブラウザ通知を出す。通知の API が無い場合と表示に失敗した場合は false。
pub fn show_finished() -> bool {
    let Some(constructor) = notification_constructor() else {
        return false;
    };
    let Ok(constructor) = constructor.dyn_into::<Function>() else {
        return false;
    };
    let Content { title, body } = content();
    let options = Object::new();
    let body_text = JsValue::from_str(text(body));
    if Reflect::set(&options, &JsValue::from_str("body"), &body_text).is_err() {
        return false;
    }
    let arguments = Array::of2(&JsValue::from_str(text(title)), options.as_ref());
    let Ok(notification) = Reflect::construct(&constructor, &arguments) else {
        return false;
    };
    let focus = Closure::once_into_js(move |_event: web_sys::Event| focus_window());
    // 押したときの処理を付けられなくても、通知そのものは出す。
    let _ = Reflect::set(&notification, &JsValue::from_str("onclick"), &focus);
    true
}

/// レスト終了を音と振動、ブラウザ通知で知らせる。音を鳴らせたかを返す。
///
/// 通知は、利用者が設定で有効にし、許可されていて、画面が隠れている場合だけ出す。
/// 通知の API が無い場合と失敗した場合も、音と画面の終了表示は止めない。
pub fn notify_rest_finished(enabled: bool, visibility: PageVisibility) -> bool {
    let sound = sound::notify_finished();
    if can_notify_current(enabled) && should_notify(visibility) {
        // 通知の失敗は音と画面の終了表示で補う。
        let _ = show_finished();
    }
    sound
}

fn can_notify_current(enabled: bool) -> bool {
    permission().is_some_and(|permission| can_notify(permission, enabled))
}

fn focus_window() {
    if let Some(window) = web_sys::window() {
        // フォーカスの失敗は、通知を押した後の操作を妨げない。
        let _ = window.focus();
    }
}

fn notification_constructor() -> Option<JsValue> {
    let window = web_sys::window()?;
    let notification = Reflect::get(&window, &JsValue::from_str("Notification")).ok()?;
    if notification.is_undefined() || notification.is_null() {
        return None;
    }
    Some(notification)
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-TMR-007`、`REQ-NFR-008`。

    use super::content;
    use crate::text::{text, Key};

    #[test]
    fn req_tmr_007_the_notification_text_is_fixed_and_has_no_record_content() {
        let content = content();
        assert_eq!(content.title, Key::RestNotificationTitle);
        assert_eq!(content.body, Key::RestNotificationBody);
        assert_eq!(text(content.title), "レスト終了");
        assert_eq!(text(content.body), "レストタイマーが終了しました。");
    }
}
