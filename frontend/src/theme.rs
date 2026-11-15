//! 画面のテーマ、表示モード、端末の設定への追従 (REQ-MYP-005、REQ-NFR-012)。
//!
//! 表示モードはライト、ダーク、端末の設定に合わせるの 3 つとする。
//! 実際のテーマはルート要素の `data-theme` で切り替え、選んだモードは
//! 初期表示のために端末内にも保存する。設定の取得を待たずに同じモードで表示するため、
//! アプリの起動時に端末内の保存を読む。

use dioxus::prelude::Signal;
pub use warrior_core::settings::DisplayMode;

/// 端末内に表示モードを保存するキー。
pub const DISPLAY_MODE_STORAGE_KEY: &str = "warrior.display_mode";

/// 画面のテーマ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// ダーク。
    Dark,
    /// ライト。
    Light,
}

impl Theme {
    /// ルート要素の `data-theme` に設定する値。
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// 端末の設定がダークかどうかからテーマを選ぶ。
    pub fn from_prefers_dark(prefers_dark: bool) -> Self {
        if prefers_dark {
            Theme::Dark
        } else {
            Theme::Light
        }
    }
}

/// 表示モードと端末の設定から、実際に表示するテーマを決める。
pub fn resolve_theme(mode: DisplayMode, device_prefers_dark: bool) -> Theme {
    Theme::from_prefers_dark(mode.prefers_dark(device_prefers_dark))
}

/// ルート要素のテーマ属性を切り替える。
pub fn apply_theme(theme: Theme) {
    if let Some(element) = document_element() {
        let _ = element.set_attribute("data-theme", theme.as_str());
    }
}

/// 表示モードをルート要素へ適用し、実際に表示するテーマを返す。
pub fn apply_display_mode(mode: DisplayMode) -> Theme {
    let theme = resolve_theme(mode, device_prefers_dark());
    apply_theme(theme);
    if let Some(element) = document_element() {
        let _ = element.set_attribute("data-display-mode", mode.as_str());
    }
    theme
}

/// 端末内に保存した表示モードを読む。保存が無い、または読めないときは端末の設定に合わせる。
pub fn cached_display_mode() -> DisplayMode {
    local_storage()
        .and_then(|storage| storage.get_item(DISPLAY_MODE_STORAGE_KEY).ok().flatten())
        .and_then(|value| DisplayMode::parse(&value))
        .unwrap_or_default()
}

/// 表示モードを端末内へ保存する。
pub fn remember_display_mode(mode: DisplayMode) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(DISPLAY_MODE_STORAGE_KEY, mode.as_str());
    }
}

/// 端末内に保存した表示モードを消す。
pub fn forget_display_mode() {
    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(DISPLAY_MODE_STORAGE_KEY);
    }
}

/// 端末内に保存した表示モードを、設定の取得を待たずに適用する。
pub fn apply_cached_display_mode() -> Theme {
    apply_display_mode(cached_display_mode())
}

/// 端末の設定の変更に追従する。アプリの寿命の間だけ購読する。
pub fn watch_system_preference() {
    use std::cell::Cell;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    thread_local! {
        static WATCHING: Cell<bool> = const { Cell::new(false) };
    }

    if WATCHING.with(|watching| watching.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        WATCHING.with(|watching| watching.set(false));
        return;
    };
    let Ok(Some(query)) = window.match_media("(prefers-color-scheme: dark)") else {
        WATCHING.with(|watching| watching.set(false));
        return;
    };
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
        apply_display_mode(applied_display_mode());
    });
    if query
        .add_listener_with_opt_callback(Some(closure.as_ref().unchecked_ref()))
        .is_err()
    {
        WATCHING.with(|watching| watching.set(false));
        return;
    }
    // 購読はアプリの寿命の間だけ必要であり、画面の破棄で解除しない。
    closure.forget();
}

/// 画面が共有する表示モードの signal。
#[derive(Clone, Copy, PartialEq)]
pub struct ThemePreference {
    /// 利用者が選んだ表示モード。
    pub mode: Signal<DisplayMode>,
}

fn applied_display_mode() -> DisplayMode {
    document_element()
        .and_then(|element| element.get_attribute("data-display-mode"))
        .and_then(|value| DisplayMode::parse(&value))
        .unwrap_or_default()
}

fn device_prefers_dark() -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .match_media("(prefers-color-scheme: dark)")
                .ok()
                .flatten()
        })
        .map(|query| query.matches())
        .unwrap_or(false)
}

fn document_element() -> Option<web_sys::Element> {
    web_sys::window()?.document()?.document_element()
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::{resolve_theme, Theme};
    use warrior_core::settings::DisplayMode;

    #[test]
    fn maps_theme_to_attribute_value() {
        assert_eq!(Theme::Dark.as_str(), "dark");
        assert_eq!(Theme::Light.as_str(), "light");
    }

    #[test]
    fn selects_theme_from_device_preference() {
        assert_eq!(Theme::from_prefers_dark(true), Theme::Dark);
        assert_eq!(Theme::from_prefers_dark(false), Theme::Light);
    }

    #[test]
    fn req_myp_005_resolves_all_three_display_modes() {
        assert_eq!(resolve_theme(DisplayMode::System, true), Theme::Dark);
        assert_eq!(resolve_theme(DisplayMode::System, false), Theme::Light);
        assert_eq!(resolve_theme(DisplayMode::Dark, false), Theme::Dark);
        assert_eq!(resolve_theme(DisplayMode::Light, true), Theme::Light);
    }
}
