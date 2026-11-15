//! レスト終了のブラウザ通知の表示の条件の性質テスト。
//!
//! 対応する要求: `REQ-TMR-007`、`REQ-NFR-008`。
//! 通知を出すかは、通知を使ってよいかに画面の表示状態だけを掛けたものであることを確かめる。
//! 画面の表示状態が隠れているときだけ通知する。通知の許可と設定は、画面の表示状態の
//! 条件を変えない。

use proptest::prelude::*;
use warrior_core::notification::{can_notify, should_notify, PageVisibility, Permission};

fn any_permission() -> impl Strategy<Value = Permission> {
    prop_oneof![
        Just(Permission::Default),
        Just(Permission::Granted),
        Just(Permission::Denied),
    ]
}

fn any_visibility() -> impl Strategy<Value = PageVisibility> {
    prop_oneof![Just(PageVisibility::Visible), Just(PageVisibility::Hidden)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn req_tmr_007_the_notification_depends_only_on_the_page_visibility(
        permission in any_permission(),
        enabled in any::<bool>(),
        visibility in any_visibility(),
    ) {
        let notifies = can_notify(permission, enabled) && should_notify(visibility);
        prop_assert_eq!(
            notifies,
            can_notify(permission, enabled) && visibility == PageVisibility::Hidden
        );
        if visibility == PageVisibility::Visible {
            prop_assert!(!notifies);
        }
    }

    #[test]
    fn req_nfr_008_the_notification_is_used_only_when_allowed_and_enabled(
        permission in any_permission(),
        enabled in any::<bool>(),
    ) {
        prop_assert_eq!(
            can_notify(permission, enabled),
            enabled && permission == Permission::Granted
        );
        prop_assert!(!can_notify(Permission::Default, enabled));
        prop_assert!(!can_notify(Permission::Denied, enabled));
    }
}
