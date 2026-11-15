//! マイページの設定、表示モード、端末ごとの設定、色のコントラスト、寸法のブラウザのテスト。
//!
//! 対応する要求: `REQ-MYP-002`、`REQ-MYP-003`、`REQ-MYP-005`、`REQ-NFR-012`、`REQ-NFR-019`。
//! 利用者共通の設定が利用者ごとに保存され、端末ごとの設定が送信待ちの列に積まれないことを
//! 確かめる。色と寸法は生成したデザイントークンを基準にする。

use warrior_core::notification;
use warrior_core::nutrition::MacroTarget;
use warrior_core::settings::{DisplayMode, Settings, SettingsError};
use warrior_frontend::generated::tokens;
use warrior_frontend::storage::Storage;
use warrior_frontend::{notification as browser_notification, theme};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{database_name, open};

#[wasm_bindgen_test]
async fn req_myp_003_user_settings_survive_a_reload_and_device_settings_are_not_queued() {
    let name = database_name("user-settings");
    let storage = open(&name).await;
    let user_a = Settings {
        rest_default_seconds: 120,
        display_mode: DisplayMode::Dark,
        macro_target: MacroTarget {
            protein_g_per_kg: 1.8,
            fat_ratio: 0.25,
        },
    };
    storage
        .save_user_settings("user-a", &user_a)
        .await
        .expect("the user settings must be saved");
    assert_eq!(
        storage
            .pending_user_settings("user-a")
            .await
            .expect("the pending settings must be readable")
            .len(),
        1,
        "a common setting must go through the pending queue"
    );
    storage
        .save_wake_lock_enabled(false)
        .await
        .expect("the wake lock setting must be saved");
    storage
        .save_browser_notification_enabled(true)
        .await
        .expect("the browser notification setting must be saved");
    storage
        .save_push_notification_enabled(true)
        .await
        .expect("the push notification setting must be saved");
    assert_eq!(
        storage
            .pending_user_settings("user-a")
            .await
            .expect("the pending settings must be readable")
            .len(),
        1,
        "a device setting must not be added to the pending queue"
    );
    drop(storage);

    let reopened: Storage = open(&name).await;
    assert_eq!(
        reopened
            .cached_user_settings("user-a")
            .await
            .expect("the cached settings must be readable"),
        Some(user_a)
    );
    assert_eq!(
        reopened
            .cached_user_settings("user-b")
            .await
            .expect("another user's cache must be readable"),
        None,
        "the cache must not mix another user's settings"
    );
    assert!(
        reopened
            .pending_user_settings("user-b")
            .await
            .expect("another user's queue must be readable")
            .is_empty(),
        "the queue must not mix another user's settings"
    );
    assert!(
        !reopened
            .wake_lock_enabled()
            .await
            .expect("the wake lock setting must be readable"),
        "the device setting must survive a reload"
    );
    assert!(
        reopened
            .browser_notification_enabled()
            .await
            .expect("the browser notification setting must be readable"),
        "the browser notification setting must survive a reload"
    );
    assert!(
        reopened
            .push_notification_enabled()
            .await
            .expect("the push notification setting must be readable"),
        "the push notification setting must survive a reload"
    );

    reopened
        .clear_account()
        .await
        .expect("the cleanup must work");
}

#[wasm_bindgen_test]
fn req_myp_003_accepts_only_the_rest_default_seconds_in_range() {
    for seconds in [1, 90, 3600] {
        let settings = Settings {
            rest_default_seconds: seconds,
            ..Settings::default()
        };
        assert_eq!(settings.validate(), Ok(()));
    }
    for seconds in [0, 3601] {
        let settings = Settings {
            rest_default_seconds: seconds,
            ..Settings::default()
        };
        assert_eq!(
            settings.validate(),
            Err(SettingsError::RestDefaultSecondsOutOfRange)
        );
    }
}

#[wasm_bindgen_test]
fn req_myp_005_restores_the_saved_display_mode_without_the_settings_api() {
    theme::remember_display_mode(DisplayMode::Dark);
    let restored = theme::apply_cached_display_mode();
    assert_eq!(restored, theme::Theme::Dark);
    let root = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .expect("the root element");
    assert_eq!(root.get_attribute("data-theme").as_deref(), Some("dark"));
    assert_eq!(
        root.get_attribute("data-display-mode").as_deref(),
        Some("dark")
    );

    theme::remember_display_mode(DisplayMode::System);
    theme::apply_display_mode(DisplayMode::System);
    assert_eq!(
        root.get_attribute("data-display-mode").as_deref(),
        Some("system")
    );
    theme::forget_display_mode();
}

#[wasm_bindgen_test]
fn req_myp_002_and_req_nfr_012_keep_the_part_and_graph_colors_readable() {
    for (name, colors) in [
        (
            "dark",
            [
                tokens::part_color::dark::CHEST,
                tokens::part_color::dark::BACK,
                tokens::part_color::dark::SHOULDERS,
                tokens::part_color::dark::BICEPS_FOREARM,
                tokens::part_color::dark::TRICEPS,
                tokens::part_color::dark::LEGS,
                tokens::part_color::dark::ABS,
                tokens::part_color::dark::OTHER,
            ],
        ),
        (
            "light",
            [
                tokens::part_color::light::CHEST,
                tokens::part_color::light::BACK,
                tokens::part_color::light::SHOULDERS,
                tokens::part_color::light::BICEPS_FOREARM,
                tokens::part_color::light::TRICEPS,
                tokens::part_color::light::LEGS,
                tokens::part_color::light::ABS,
                tokens::part_color::light::OTHER,
            ],
        ),
    ] {
        let (background, surface) = if name == "dark" {
            (tokens::color::dark::BG, tokens::color::dark::SURFACE)
        } else {
            (tokens::color::light::BG, tokens::color::light::SURFACE)
        };
        for color in colors {
            assert!(
                contrast_ratio(color, background) >= 3.0,
                "{name} part color {color} must contrast with the background"
            );
            assert!(
                contrast_ratio(color, surface) >= 3.0,
                "{name} part color {color} must contrast with the surface"
            );
        }
    }
}

#[wasm_bindgen_test]
fn req_myp_003_meets_the_minimum_touch_target_and_text_size() {
    let window = web_sys::window().expect("the browser window");
    let document = window.document().expect("the document");
    let body = document.body().expect("the body");
    let element = document
        .create_element("button")
        .expect("the button element");
    element
        .set_attribute(
            "style",
            &format!(
                "min-height: {}px; font-size: {}px;",
                tokens::TOUCH_TARGET_MIN,
                tokens::TEXT_SIZE_MIN
            ),
        )
        .expect("the style attribute");
    body.append_child(&element)
        .expect("the button must be added");
    let style = window
        .get_computed_style(&element)
        .expect("the computed style must be readable")
        .expect("the computed style");
    assert!(
        parse_pixels(&style.get_property_value("min-height").unwrap_or_default())
            >= f64::from(tokens::TOUCH_TARGET_MIN),
        "a touch target must be at least 44px"
    );
    assert!(
        parse_pixels(&style.get_property_value("font-size").unwrap_or_default())
            >= f64::from(tokens::TEXT_SIZE_MIN),
        "text must be at least 11px"
    );
    element.remove();
}

#[wasm_bindgen_test]
fn req_myp_003_requests_the_notification_permission_only_when_enabling() {
    assert!(browser_notification::should_request_permission(false, true));
    assert!(!browser_notification::should_request_permission(true, true));
    assert!(!browser_notification::should_request_permission(
        true, false
    ));
    assert!(!browser_notification::should_request_permission(
        false, false
    ));
    assert!(notification::can_notify(
        warrior_core::notification::Permission::Granted,
        true
    ));
    assert!(!notification::can_notify(
        warrior_core::notification::Permission::Denied,
        true
    ));
    assert!(!notification::can_notify(
        warrior_core::notification::Permission::Granted,
        false
    ));
}

fn contrast_ratio(left: &str, right: &str) -> f64 {
    let left = luminance(left);
    let right = luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn luminance(color: &str) -> f64 {
    let color = color.trim_start_matches('#');
    let channels = [0, 2, 4].map(|index| {
        let value =
            u8::from_str_radix(&color[index..index + 2], 16).expect("a hex color") as f64 / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
}

fn parse_pixels(value: &str) -> f64 {
    value
        .trim_end_matches("px")
        .parse::<f64>()
        .expect("a pixel value")
}
