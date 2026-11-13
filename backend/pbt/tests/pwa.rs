//! PWA のキャッシュのキーの性質テスト。
//!
//! 対応する要求: `REQ-NFR-001`。

use proptest::prelude::*;
use warrior_core::pwa::{cache_key, normalize_path};

proptest! {
    #[test]
    fn cache_keys_are_unique_by_version_and_path(
        version in ".{0,32}",
        path in "/[A-Za-z0-9_./-]{0,80}",
    ) {
        let key = cache_key(&version, &path);
        let prefix = format!("{version}:");
        prop_assert!(key.starts_with(&prefix));
        prop_assert!(key.ends_with(&normalize_path(&path)));
        prop_assert_eq!(cache_key(&version, &path), key);
    }

    #[test]
    fn query_and_fragment_do_not_change_the_key(
        version in ".{0,16}",
        path in "/[A-Za-z0-9_./-]{0,40}",
        query in ".{0,20}",
    ) {
        let with_query = format!("{path}?{query}");
        prop_assert_eq!(cache_key(&version, &path), cache_key(&version, &with_query));
    }
}
