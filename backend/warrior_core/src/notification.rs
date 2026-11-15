//! レスト終了のブラウザ通知の許可と表示の条件 (REQ-TMR-007、REQ-NFR-008)。
//!
//! ブラウザ通知は、利用者が設定で有効にし、ブラウザが許可している場合にだけ使う。
//! 通知を出すかは、画面の表示状態だけで決める。画面が見えているときは通知せず、
//! 画面の終了表示で知らせる。通知を許可しなくても、レストタイマーは音と画面表示で動く。

/// ブラウザ通知の許可の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// 利用者がまだ許可を選んでいない。
    Default,
    /// 許可されている。
    Granted,
    /// 拒否されている。
    Denied,
}

impl Permission {
    /// `Notification.permission` の値を許可の状態に変換する。未知の値は None。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "default" => Some(Self::Default),
            "granted" => Some(Self::Granted),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }

    /// ブラウザ通知が許可されているか。
    pub fn is_granted(self) -> bool {
        matches!(self, Self::Granted)
    }
}

/// 画面の表示状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageVisibility {
    /// 画面が見えている。
    Visible,
    /// 別のタブやアプリを表示している。
    Hidden,
}

/// ブラウザ通知を使ってよいか。許可されていて、利用者が設定で有効にしている場合だけ使う。
pub fn can_notify(permission: Permission, enabled: bool) -> bool {
    enabled && permission.is_granted()
}

/// ブラウザ通知を出すか。画面が隠れているときだけ出す。
pub fn should_notify(visibility: PageVisibility) -> bool {
    matches!(visibility, PageVisibility::Hidden)
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-TMR-007`、`REQ-NFR-008`。

    use super::{can_notify, should_notify, PageVisibility, Permission};

    #[test]
    fn req_nfr_008_the_notification_is_allowed_only_when_granted_and_enabled() {
        assert!(can_notify(Permission::Granted, true));
        assert!(!can_notify(Permission::Granted, false));
        assert!(!can_notify(Permission::Denied, true));
        assert!(!can_notify(Permission::Denied, false));
        assert!(!can_notify(Permission::Default, true));
        assert!(!can_notify(Permission::Default, false));
    }

    #[test]
    fn req_tmr_007_the_notification_is_shown_only_while_the_page_is_hidden() {
        assert!(should_notify(PageVisibility::Hidden));
        assert!(!should_notify(PageVisibility::Visible));
    }

    #[test]
    fn permission_parses_the_browser_values_and_rejects_unknown_values() {
        assert_eq!(Permission::parse("default"), Some(Permission::Default));
        assert_eq!(Permission::parse("granted"), Some(Permission::Granted));
        assert_eq!(Permission::parse("denied"), Some(Permission::Denied));
        assert_eq!(Permission::parse("unsupported"), None);
        assert_eq!(Permission::parse(""), None);
    }

    #[test]
    fn only_the_granted_permission_allows_the_notification() {
        assert!(Permission::Granted.is_granted());
        assert!(!Permission::Default.is_granted());
        assert!(!Permission::Denied.is_granted());
    }
}
