//! 利用者共通の設定 (REQ-MYP-002、REQ-MYP-003、REQ-MYP-005、REQ-NFR-019)。
//!
//! 利用者共通の設定はレスト時間の既定値、表示モード、PFC の配分を持つ。
//! 画面の消灯防止、ブラウザ通知、プッシュ通知は端末ごとの設定であり、ここには置かない。
//! レスト時間の既定値は 1 秒以上 3600 秒以下とする。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::nutrition::MacroTarget;
use crate::rest_timer;

/// 表示モード。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    /// 端末の設定に合わせる。
    #[default]
    System,
    /// ライト。
    Light,
    /// ダーク。
    Dark,
}

impl DisplayMode {
    /// API、D1、端末内の保存に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// 文字列を表示モードに戻す。
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    /// 端末の設定がダークかどうかから、実際に表示するモードがダークかを決める。
    pub fn prefers_dark(self, device_prefers_dark: bool) -> bool {
        match self {
            Self::System => device_prefers_dark,
            Self::Light => false,
            Self::Dark => true,
        }
    }
}

/// 利用者共通の設定。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// レスト時間の既定値 (秒)。
    pub rest_default_seconds: u32,
    /// 表示モード。
    pub display_mode: DisplayMode,
    /// PFC の配分。
    pub macro_target: MacroTarget,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rest_default_seconds: rest_timer::DEFAULT_SECONDS,
            display_mode: DisplayMode::System,
            macro_target: MacroTarget::default(),
        }
    }
}

/// 設定の検証の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsError {
    /// レスト時間の既定値が 1 秒以上 3600 秒以下でない。
    RestDefaultSecondsOutOfRange,
    /// PFC の配分が範囲外である。
    MacroTargetOutOfRange,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RestDefaultSecondsOutOfRange => {
                formatter.write_str("the rest default seconds are out of range")
            }
            Self::MacroTargetOutOfRange => formatter.write_str("the macro target is out of range"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl Settings {
    /// 設定の範囲を検証する。
    pub fn validate(self) -> Result<(), SettingsError> {
        if !rest_timer::is_valid_seconds(self.rest_default_seconds) {
            return Err(SettingsError::RestDefaultSecondsOutOfRange);
        }
        if !self.macro_target.is_valid() {
            return Err(SettingsError::MacroTargetOutOfRange);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! 利用者共通の設定の例示テスト。
    //!
    //! 対応する要求: `REQ-MYP-002`、`REQ-MYP-003`、`REQ-MYP-005`、`REQ-NFR-019`。

    use super::{DisplayMode, Settings, SettingsError};
    use crate::nutrition::MacroTarget;
    use crate::rest_timer::{DEFAULT_SECONDS, MAX_SECONDS, MIN_SECONDS};

    #[test]
    fn req_myp_003_the_default_rest_seconds_are_ninety() {
        assert_eq!(Settings::default().rest_default_seconds, DEFAULT_SECONDS);
        assert_eq!(Settings::default().display_mode, DisplayMode::System);
        assert_eq!(
            Settings::default().macro_target,
            MacroTarget::default(),
            "the PFC allocation must start at the calculation defaults"
        );
    }

    #[test]
    fn req_myp_003_accepts_the_rest_default_at_the_boundaries() {
        for seconds in [MIN_SECONDS, 90, MAX_SECONDS] {
            let settings = Settings {
                rest_default_seconds: seconds,
                ..Settings::default()
            };
            assert_eq!(settings.validate(), Ok(()));
        }
    }

    #[test]
    fn req_myp_003_rejects_the_rest_default_outside_the_boundaries() {
        for seconds in [0, MAX_SECONDS + 1] {
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

    #[test]
    fn req_myp_005_names_all_three_display_modes() {
        assert_eq!(DisplayMode::System.as_str(), "system");
        assert_eq!(DisplayMode::Light.as_str(), "light");
        assert_eq!(DisplayMode::Dark.as_str(), "dark");
        for mode in [DisplayMode::System, DisplayMode::Light, DisplayMode::Dark] {
            assert_eq!(DisplayMode::parse(mode.as_str()), Some(mode));
            assert_eq!(serde_json::to_value(mode).unwrap(), mode.as_str());
        }
        assert_eq!(DisplayMode::parse("other"), None);
    }

    #[test]
    fn req_nfr_012_system_follows_the_device_and_manual_modes_do_not() {
        assert!(DisplayMode::System.prefers_dark(true));
        assert!(!DisplayMode::System.prefers_dark(false));
        assert!(!DisplayMode::Light.prefers_dark(true));
        assert!(DisplayMode::Dark.prefers_dark(false));
    }

    #[test]
    fn req_myp_002_rejects_a_pfc_allocation_outside_the_ranges() {
        for macro_target in [
            MacroTarget {
                protein_g_per_kg: 1.5,
                fat_ratio: 0.2,
            },
            MacroTarget {
                protein_g_per_kg: 2.3,
                fat_ratio: 0.2,
            },
            MacroTarget {
                protein_g_per_kg: 2.0,
                fat_ratio: -0.1,
            },
            MacroTarget {
                protein_g_per_kg: 2.0,
                fat_ratio: 1.1,
            },
        ] {
            let settings = Settings {
                macro_target,
                ..Settings::default()
            };
            assert_eq!(
                settings.validate(),
                Err(SettingsError::MacroTargetOutOfRange)
            );
        }
    }

    #[test]
    fn req_myp_002_round_trips_the_settings_as_json() {
        let settings = Settings {
            rest_default_seconds: 120,
            display_mode: DisplayMode::Dark,
            macro_target: MacroTarget {
                protein_g_per_kg: 1.8,
                fat_ratio: 0.25,
            },
        };
        let value = serde_json::to_value(settings).expect("serializable settings");
        assert_eq!(
            value,
            serde_json::json!({
                "rest_default_seconds": 120,
                "display_mode": "dark",
                "macro_target": {
                    "protein_g_per_kg": 1.8,
                    "fat_ratio": 0.25
                }
            })
        );
        assert_eq!(
            serde_json::from_value::<Settings>(value).expect("deserializable settings"),
            settings
        );
    }

    #[test]
    fn req_myp_003_rejects_unknown_fields_and_invalid_display_modes() {
        let mut value = serde_json::to_value(Settings::default()).expect("serializable settings");
        value["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Settings>(value).is_err());

        let mut value = serde_json::to_value(Settings::default()).expect("serializable settings");
        value["display_mode"] = serde_json::json!("other");
        assert!(serde_json::from_value::<Settings>(value).is_err());
    }
}
