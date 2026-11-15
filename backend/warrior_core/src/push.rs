//! Web Push の購読の型と endpoint の検証 (REQ-TMR-008、REQ-NFR-008、ADR-008)。
//!
//! サーバーは、HTTPS で、かつ許可したプッシュサービスのホストを持つ endpoint だけを
//! 購読として受け付ける。許可するホストは ADR-008 の一覧と同じにし、ホストを増やすときは
//! 同じ変更で ADR を更新する。通知の題名と本文は固定の文言とし、記録の内容を含めない。

use serde::Deserialize;

/// プッシュ通知の題名。
pub const NOTIFICATION_TITLE: &str = "レスト終了";

/// プッシュ通知の本文。
pub const NOTIFICATION_BODY: &str = "レストタイマーが終了しました。";

/// 購読を受け付けるプッシュサービスのホスト (ADR-008)。
pub const ALLOWED_HOSTS: &[&str] = &["web.push.apple.com", "fcm.googleapis.com"];

/// endpoint の最大の長さ。
pub const ENDPOINT_MAX_LENGTH: usize = 2048;

/// 購読の鍵の最大の長さ。
pub const KEY_MAX_LENGTH: usize = 512;

/// 購読の入力。
#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionInput {
    /// プッシュサービスの endpoint。
    pub endpoint: String,
    /// 端末の公開鍵 (base64url)。
    pub p256dh: String,
    /// 認証の秘密 (base64url)。
    pub auth: String,
}

/// 検証済みの購読。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    endpoint: String,
    p256dh: String,
    auth: String,
}

impl Subscription {
    /// 入力を検証して購読にする。endpoint か鍵が条件を満たさない場合はエラー。
    pub fn new(input: SubscriptionInput) -> Result<Self, SubscriptionError> {
        if !is_allowed_endpoint(&input.endpoint) {
            return Err(SubscriptionError::Endpoint);
        }
        if !is_valid_key(&input.p256dh) || !is_valid_key(&input.auth) {
            return Err(SubscriptionError::Key);
        }
        Ok(Self {
            endpoint: input.endpoint,
            p256dh: input.p256dh,
            auth: input.auth,
        })
    }

    /// プッシュサービスの endpoint。
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// 端末の公開鍵 (base64url)。
    pub fn p256dh(&self) -> &str {
        &self.p256dh
    }

    /// 認証の秘密 (base64url)。
    pub fn auth(&self) -> &str {
        &self.auth
    }
}

/// 購読を受け付けない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionError {
    /// endpoint が HTTPS でない、または許可しないホストである。
    Endpoint,
    /// 鍵が空、または長すぎる。
    Key,
}

impl SubscriptionError {
    /// 応答に載せる英語の説明。
    pub fn message(self) -> &'static str {
        match self {
            Self::Endpoint => "the push endpoint is not allowed",
            Self::Key => "the push subscription keys are invalid",
        }
    }
}

/// endpoint のホストを返す。HTTPS でない、または URL として読めない場合は None。
///
/// 利用者情報、IPv6 のアドレス、数値でないポートは受け付けない。ホストの大文字と
/// 小文字は区別しない。
pub fn https_host(endpoint: &str) -> Option<&str> {
    if endpoint.len() > ENDPOINT_MAX_LENGTH {
        return None;
    }
    let (scheme, rest) = endpoint.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest
        .find(['/', '?', '#'])
        .map_or(rest, |index| &rest[..index]);
    if authority.is_empty()
        || authority.contains('@')
        || authority.starts_with('[')
        || authority.ends_with(':')
    {
        return None;
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }
    if host.is_empty() {
        return None;
    }
    Some(host)
}

/// 許可したホストの HTTPS の endpoint か。
pub fn is_allowed_endpoint(endpoint: &str) -> bool {
    https_host(endpoint).is_some_and(|host| {
        ALLOWED_HOSTS
            .iter()
            .any(|allowed| host.eq_ignore_ascii_case(allowed))
    })
}

/// iOS でホーム画面に追加していないために、追加の手順を案内するか。
pub fn needs_install_hint(user_agent: &str, standalone: bool) -> bool {
    let is_ios = ["iPhone", "iPad", "iPod"]
        .iter()
        .any(|device| user_agent.contains(device));
    is_ios && !standalone
}

fn is_valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= KEY_MAX_LENGTH
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-TMR-008`、`REQ-NFR-008`。

    use super::{
        https_host, is_allowed_endpoint, needs_install_hint, Subscription, SubscriptionError,
        SubscriptionInput, ALLOWED_HOSTS, ENDPOINT_MAX_LENGTH, KEY_MAX_LENGTH, NOTIFICATION_BODY,
        NOTIFICATION_TITLE,
    };

    const ADR: &str = include_str!("../../../docs/adr/008-server-scheduled-rest-push.md");

    fn input(endpoint: &str) -> SubscriptionInput {
        SubscriptionInput {
            endpoint: endpoint.to_owned(),
            p256dh: "p256dh".to_owned(),
            auth: "auth".to_owned(),
        }
    }

    #[test]
    fn req_tmr_008_the_allowed_hosts_accept_https_with_any_path() {
        for host in ALLOWED_HOSTS {
            for endpoint in [
                format!("https://{host}/send/abc"),
                format!("https://{host}/send/abc?x=1#y"),
                format!("https://{host}"),
                format!("https://{host}:443/send/abc"),
                format!("HTTPS://{host}/send/abc"),
            ] {
                assert!(is_allowed_endpoint(&endpoint), "{endpoint} must be allowed");
            }
        }
    }

    #[test]
    fn req_tmr_008_http_and_other_hosts_are_rejected() {
        for endpoint in [
            "http://web.push.apple.com/send/abc",
            "http://fcm.googleapis.com/send/abc",
            "https://evil.example/send/abc",
            "https://web.push.apple.com.evil.example/send/abc",
            "https://sub.fcm.googleapis.com/send/abc",
            "https://fcm.googleapis.com./send/abc",
            "https://fcm.googleapis.com@evil.example/send/abc",
            "https://user@fcm.googleapis.com/send/abc",
            "https://[::1]/send/abc",
            "https://fcm.googleapis.com:https/send/abc",
            "fcm.googleapis.com/send/abc",
            "",
        ] {
            assert!(
                !is_allowed_endpoint(endpoint),
                "{endpoint} must be rejected"
            );
        }
    }

    #[test]
    fn the_endpoint_length_boundary_is_fixed() {
        let host = ALLOWED_HOSTS[0];
        let prefix = format!("https://{host}/");
        let allowed = format!("{prefix}{}", "a".repeat(ENDPOINT_MAX_LENGTH - prefix.len()));
        assert_eq!(allowed.len(), ENDPOINT_MAX_LENGTH);
        assert!(is_allowed_endpoint(&allowed));
        let too_long = format!("{allowed}a");
        assert!(!is_allowed_endpoint(&too_long));
    }

    #[test]
    fn https_host_reads_only_the_authority() {
        assert_eq!(
            https_host("https://web.push.apple.com/send/abc?x=1"),
            Some("web.push.apple.com")
        );
        assert_eq!(
            https_host("https://FCM.GOOGLEAPIS.COM:443"),
            Some("FCM.GOOGLEAPIS.COM")
        );
        assert_eq!(https_host("https:///send"), None);
        assert_eq!(https_host("https://fcm.googleapis.com:/send"), None);
        assert_eq!(https_host("http://fcm.googleapis.com/send"), None);
    }

    #[test]
    fn req_tmr_008_the_subscription_is_built_only_from_an_allowed_endpoint_and_keys() {
        let endpoint = "https://fcm.googleapis.com/fcm/send/abc";
        let subscription = Subscription::new(input(endpoint)).expect("an allowed subscription");
        assert_eq!(subscription.endpoint(), endpoint);
        assert_eq!(subscription.p256dh(), "p256dh");
        assert_eq!(subscription.auth(), "auth");

        assert_eq!(
            Subscription::new(input("http://fcm.googleapis.com/fcm/send/abc")),
            Err(SubscriptionError::Endpoint)
        );
        assert_eq!(
            Subscription::new(input("https://evil.example/fcm/send/abc")),
            Err(SubscriptionError::Endpoint)
        );

        let mut empty_endpoint_key = input(endpoint);
        empty_endpoint_key.p256dh = String::new();
        assert_eq!(
            Subscription::new(empty_endpoint_key),
            Err(SubscriptionError::Key)
        );
        let mut empty_auth = input(endpoint);
        empty_auth.auth = String::new();
        assert_eq!(Subscription::new(empty_auth), Err(SubscriptionError::Key));
        let mut long_key = input(endpoint);
        long_key.p256dh = "a".repeat(KEY_MAX_LENGTH + 1);
        assert_eq!(Subscription::new(long_key), Err(SubscriptionError::Key));
        let mut boundary_key = input(endpoint);
        boundary_key.auth = "a".repeat(KEY_MAX_LENGTH);
        assert_eq!(
            Subscription::new(boundary_key).map(|subscription| subscription.auth().len()),
            Ok(KEY_MAX_LENGTH)
        );
    }

    #[test]
    fn the_allowed_hosts_are_the_ones_in_the_adr() {
        for host in ALLOWED_HOSTS {
            assert!(
                ADR.contains(&format!("`{host}`")),
                "the ADR must list the allowed host {host}"
            );
        }
    }

    #[test]
    fn the_notification_text_is_fixed_and_has_no_record_content() {
        assert_eq!(NOTIFICATION_TITLE, "レスト終了");
        assert_eq!(NOTIFICATION_BODY, "レストタイマーが終了しました。");
        for content in ["ベンチプレス", "100", "kg", "5 回"] {
            assert!(
                !NOTIFICATION_BODY.contains(content),
                "the body must not contain {content}"
            );
        }
    }

    #[test]
    fn req_tmr_008_the_install_hint_is_shown_only_for_ios_outside_the_home_screen() {
        assert!(needs_install_hint(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)",
            false
        ));
        assert!(needs_install_hint(
            "Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X)",
            false
        ));
        assert!(needs_install_hint("iPod touch", false));
        assert!(!needs_install_hint(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)",
            true
        ));
        assert!(!needs_install_hint(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
            false
        ));
        assert!(!needs_install_hint(
            "Mozilla/5.0 (Linux; Android 15)",
            false
        ));
    }
}
