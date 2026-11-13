//! テーマ属性の値。
//!
//! 実際の DOM の属性の変更と利用者の選択の保存は、テーマの設定を担う変更で実装する。

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

#[cfg(test)]
mod tests {
    use super::Theme;

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
}
