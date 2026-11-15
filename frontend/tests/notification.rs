//! レスト終了のブラウザ通知の許可、表示、押したときの画面の移動のブラウザのテスト。
//!
//! 対応する要求: `REQ-TMR-007`、`REQ-NFR-008`。
//! 実際の通知の表示と、押したときのウィンドウのフォーカスはテストから観測できないため、
//! `window.Notification` と `window.focus` をテストの中だけ差し替え、許可の状態、
//! 通知の題名と本文、押したときのフォーカスの要求を数える。
//! 終了の画面表示は E2E のレストタイマーのテストが確かめる。

use warrior_core::notification::{PageVisibility, Permission};
use warrior_frontend::notification;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{counter, install_sound_stub};

#[wasm_bindgen_test]
async fn req_tmr_007_permission_takes_default_granted_and_denied() {
    install_notification_stub("default", "default");
    assert_eq!(notification::permission(), Some(Permission::Default));
    assert_eq!(
        notification::request_permission().await,
        Some(Permission::Default)
    );

    install_notification_stub("granted", "granted");
    assert_eq!(notification::permission(), Some(Permission::Granted));
    assert_eq!(
        notification::request_permission().await,
        Some(Permission::Granted)
    );

    install_notification_stub("denied", "denied");
    assert_eq!(notification::permission(), Some(Permission::Denied));
    assert_eq!(
        notification::request_permission().await,
        Some(Permission::Denied)
    );
}

#[wasm_bindgen_test]
fn req_tmr_007_finished_rest_shows_the_fixed_text_without_record_content() {
    install_notification_stub("granted", "granted");
    assert!(notification::show_finished());
    assert_eq!(notification_count(), 1);
    assert_eq!(notification_field("title"), "レスト終了");
    assert_eq!(notification_field("body"), "レストタイマーが終了しました。");
    for record_content in ["ベンチプレス", "100", "5 回"] {
        assert!(
            !notification_field("body").contains(record_content),
            "the notification must not contain the record content {record_content}"
        );
    }
}

#[wasm_bindgen_test]
fn req_tmr_007_clicking_the_notification_focuses_the_open_window() {
    install_notification_stub("granted", "granted");
    install_focus_stub();
    assert!(notification::show_finished());
    js_sys::eval("window.__notifications[0].onclick(new Event('click'));")
        .expect("the click handler must run");
    assert_eq!(counter("__windowFocuses"), 1.0);
}

#[wasm_bindgen_test]
fn req_tmr_007_the_notification_is_shown_only_while_the_page_is_hidden() {
    install_notification_stub("granted", "granted");
    install_sound_stub();
    assert!(notification::notify_rest_finished(
        true,
        PageVisibility::Visible
    ));
    assert_eq!(notification_count(), 0);
    assert!(notification::notify_rest_finished(
        true,
        PageVisibility::Hidden
    ));
    assert_eq!(notification_count(), 1);
}

#[wasm_bindgen_test]
fn req_nfr_008_denied_notification_still_plays_the_finish_sound() {
    install_notification_stub("denied", "denied");
    install_sound_stub();
    assert!(
        notification::notify_rest_finished(true, PageVisibility::Hidden),
        "the finish sound must play without the notification"
    );
    assert_eq!(counter("__restAudioStarts"), 1.0);
    assert_eq!(counter("__restVibrations"), 1.0);
    assert_eq!(notification_count(), 0);
}

#[wasm_bindgen_test]
async fn req_nfr_008_missing_notification_api_does_not_stop_the_finish() {
    remove_notification_stub();
    install_sound_stub();
    assert_eq!(notification::permission(), None);
    assert_eq!(notification::request_permission().await, None);
    assert!(
        notification::notify_rest_finished(true, PageVisibility::Hidden),
        "the finish sound must play without the notification API"
    );
    assert_eq!(counter("__restAudioStarts"), 1.0);
}

/// ブラウザの通知の API を差し替える。許可と要求の結果は `window` の値で切り替える。
fn install_notification_stub(permission: &str, request: &str) {
    js_sys::eval(&format!(
        r#"
        window.__notifications = [];
        window.__notificationPermission = '{permission}';
        window.__notificationRequest = '{request}';
        class FakeNotification {{
            constructor(title, options) {{
                this.title = title;
                this.body = options && options.body ? options.body : '';
                this.onclick = null;
                window.__notifications.push(this);
            }}
        }}
        Object.defineProperty(FakeNotification, 'permission', {{
            configurable: true,
            get: function () {{ return window.__notificationPermission; }}
        }});
        FakeNotification.requestPermission = function () {{
            return Promise.resolve(window.__notificationRequest);
        }};
        Object.defineProperty(window, 'Notification', {{
            configurable: true,
            value: FakeNotification
        }});
        "#,
    ))
    .expect("the notification stub must be installed");
}

fn remove_notification_stub() {
    js_sys::eval("delete window.Notification;").expect("the notification stub must be removed");
}

/// 画面のフォーカスの要求を数える。
fn install_focus_stub() {
    js_sys::eval(
        r#"
        window.__windowFocuses = 0;
        const focus = function () { window.__windowFocuses++; };
        try {
            Object.defineProperty(window, 'focus', { configurable: true, value: focus });
        } catch (error) {
            Object.defineProperty(Window.prototype, 'focus', { configurable: true, value: focus });
        }
        "#,
    )
    .expect("the focus stub must be installed");
}

fn notification_count() -> usize {
    js_sys::eval("window.__notifications.length")
        .expect("the notification count must be readable")
        .as_f64()
        .map_or(0, |count| count as usize)
}

fn notification_field(field: &str) -> String {
    js_sys::eval(&format!("window.__notifications[0].{field} || ''"))
        .expect("the notification field must be readable")
        .as_string()
        .unwrap_or_default()
}
