//! アカウント削除の対象の表の分類と、R2 の接頭辞。
//!
//! 表は「利用者に属する」「マスター」「一時」のいずれかに分類する。
//! 利用者に属する表は、削除の対象の一覧と双方向に一致させる (issue 0017)。

/// 表の分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableClass {
    /// 利用者に属する行を持つ表。
    Owned,
    /// 利用者に紐づかない参照データ。
    Master,
    /// 短期の状態を持ち、利用者に紐づかない表。
    Temporary,
}

/// 1 つの表の分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Table {
    /// 表の名前。
    pub name: &'static str,
    /// 分類。
    pub class: TableClass,
    /// 分類の理由。
    pub reason: &'static str,
}

/// 現在の migrations に存在する表の分類。
pub const TABLES: &[Table] = &[
    Table {
        name: "users",
        class: TableClass::Owned,
        reason: "利用者自身の行。",
    },
    Table {
        name: "auth_sessions",
        class: TableClass::Owned,
        reason: "利用者のセッション。",
    },
    Table {
        name: "terminal_tokens",
        class: TableClass::Owned,
        reason: "利用者の端末トークン。記録中のトレーニングの編集権を持つ。",
    },
    Table {
        name: "passkeys",
        class: TableClass::Owned,
        reason: "利用者のパスキー。",
    },
    Table {
        name: "webauthn_challenges",
        class: TableClass::Owned,
        reason: "登録と再認証のチャレンジ。利用者 ID を持つ行を削除する。",
    },
    Table {
        name: "elevation_tokens",
        class: TableClass::Owned,
        reason: "利用者の再認証トークン。",
    },
    Table {
        name: "user_rate_limits",
        class: TableClass::Owned,
        reason: "利用者ごとのレート制限のカウンタ。",
    },
    Table {
        name: "ip_rate_limits",
        class: TableClass::Temporary,
        reason: "IP ごとのレート制限のカウンタ。利用者に紐づかない。",
    },
    Table {
        name: "user_exercises",
        class: TableClass::Owned,
        reason: "利用者の独自種目。",
    },
    Table {
        name: "user_exercise_muscles",
        class: TableClass::Owned,
        reason: "独自種目の筋肉。親の user_exercises から所有される。",
    },
    Table {
        name: "master_exercise_overrides",
        class: TableClass::Owned,
        reason: "利用者ごとのマスター種目の上書き。",
    },
    Table {
        name: "exercise_favorites",
        class: TableClass::Owned,
        reason: "利用者の種目のお気に入り。",
    },
    Table {
        name: "training_sessions",
        class: TableClass::Owned,
        reason: "利用者のトレーニング。",
    },
    Table {
        name: "training_exercises",
        class: TableClass::Owned,
        reason: "トレーニングに追加した種目。親の training_sessions から所有される。",
    },
    Table {
        name: "sets",
        class: TableClass::Owned,
        reason: "種目に記録したセット。親の training_exercises から所有される。",
    },
    Table {
        name: "set_segments",
        class: TableClass::Owned,
        reason: "ドロップセットの段。親の sets から所有される。",
    },
    Table {
        name: "supersets",
        class: TableClass::Owned,
        reason: "トレーニングのスーパーセットの組。",
    },
    Table {
        name: "superset_members",
        class: TableClass::Owned,
        reason: "スーパーセットに含める種目。親の supersets から所有される。",
    },
    Table {
        name: "applied_operations",
        class: TableClass::Owned,
        reason: "利用者の端末から適用した同期の操作。",
    },
    Table {
        name: "personal_bests",
        class: TableClass::Owned,
        reason: "利用者の種目別の自己ベスト。",
    },
    Table {
        name: "profiles",
        class: TableClass::Owned,
        reason: "利用者の身体情報、目的、日数、活動量の最新値。",
    },
    Table {
        name: "push_subscriptions",
        class: TableClass::Owned,
        reason: "利用者の端末のプッシュ購読。",
    },
    Table {
        name: "export_jobs",
        class: TableClass::Owned,
        reason: "利用者の記録の CSV の書き出しのメタデータ。",
    },
    Table {
        name: "muscles",
        class: TableClass::Master,
        reason: "筋肉の単位。利用者に紐づかない。",
    },
    Table {
        name: "master_exercises",
        class: TableClass::Master,
        reason: "種目マスター。利用者に紐づかない。",
    },
    Table {
        name: "master_exercise_muscles",
        class: TableClass::Master,
        reason: "種目マスターの筋肉。利用者に紐づかない。",
    },
    Table {
        name: "oauth_states",
        class: TableClass::Temporary,
        reason: "5 分で失効する OAuth の状態。セッションを無効にすれば残っても認証に使えない。",
    },
];

/// 利用者に属する表を、子から親の順に並べた削除の対象。
///
/// `users` を最後に置き、外部キーの子を先に消す。
pub const USER_OWNED_TABLES: &[&str] = &[
    "webauthn_challenges",
    "user_exercise_muscles",
    "user_exercises",
    "master_exercise_overrides",
    "exercise_favorites",
    "user_rate_limits",
    "elevation_tokens",
    "passkeys",
    "auth_sessions",
    "push_subscriptions",
    "export_jobs",
    "terminal_tokens",
    "superset_members",
    "set_segments",
    "sets",
    "supersets",
    "training_exercises",
    "applied_operations",
    "personal_bests",
    "training_sessions",
    "profiles",
    "users",
];

/// 利用者の R2 の接頭辞。
pub fn r2_prefixes(user_id: &str) -> [String; 2] {
    [format!("users/{user_id}/"), format!("exports/{user_id}/")]
}

/// 表を分類する。未知の表は None。
pub fn classify(name: &str) -> Option<TableClass> {
    TABLES
        .iter()
        .find(|table| table.name == name)
        .map(|table| table.class)
}

#[cfg(test)]
mod tests {
    use super::{classify, r2_prefixes, TableClass, TABLES, USER_OWNED_TABLES};

    #[test]
    fn every_current_table_is_classified_once() {
        for name in [
            "users",
            "auth_sessions",
            "terminal_tokens",
            "passkeys",
            "webauthn_challenges",
            "elevation_tokens",
            "user_rate_limits",
            "ip_rate_limits",
            "muscles",
            "master_exercises",
            "master_exercise_muscles",
            "user_exercises",
            "user_exercise_muscles",
            "master_exercise_overrides",
            "exercise_favorites",
            "training_sessions",
            "training_exercises",
            "sets",
            "set_segments",
            "supersets",
            "superset_members",
            "applied_operations",
            "personal_bests",
            "oauth_states",
            "profiles",
            "push_subscriptions",
            "export_jobs",
        ] {
            assert!(classify(name).is_some(), "{name} must be classified");
        }
        let owned = TABLES
            .iter()
            .filter(|table| table.class == TableClass::Owned)
            .map(|table| table.name)
            .collect::<Vec<_>>();
        assert_eq!(owned.len(), USER_OWNED_TABLES.len());
        for name in USER_OWNED_TABLES {
            assert!(owned.contains(name), "{name} must be owned");
        }
    }

    #[test]
    fn the_users_row_is_deleted_last() {
        assert_eq!(USER_OWNED_TABLES.last(), Some(&"users"));
    }

    #[test]
    fn oauth_states_are_temporary_even_though_they_can_carry_a_user_id() {
        assert_eq!(classify("oauth_states"), Some(TableClass::Temporary));
    }

    #[test]
    fn r2_prefixes_are_scoped_to_the_user() {
        assert_eq!(
            r2_prefixes("user-1"),
            ["users/user-1/".to_owned(), "exports/user-1/".to_owned()]
        );
    }
}
