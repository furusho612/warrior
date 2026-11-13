//! migrations の表の一覧と、アカウント削除の分類の一致を確かめる。
//!
//! 対応する要求: `REQ-NFR-010`。モックは使わず、migrations の SQL を直接読む。

use std::collections::BTreeSet;
use std::path::Path;

use warrior_core::account_deletion::{TABLES, USER_OWNED_TABLES};

#[test]
fn every_migration_table_is_classified_and_every_classification_exists() {
    let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut tables = BTreeSet::new();
    for entry in std::fs::read_dir(migrations).expect("the migrations directory must exist") {
        let path = entry.expect("a migration entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("sql") {
            continue;
        }
        let sql = std::fs::read_to_string(&path).expect("a migration must be readable");
        for line in sql.lines() {
            let line = line.trim_start();
            let Some(rest) = line.strip_prefix("CREATE TABLE ") else {
                continue;
            };
            let name = rest
                .split(|character: char| character.is_whitespace() || character == '(')
                .next()
                .expect("a table name");
            tables.insert(name.to_owned());
        }
    }

    let classified = TABLES
        .iter()
        .map(|table| table.name.to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        tables, classified,
        "migrations and the classification must match"
    );
    assert!(USER_OWNED_TABLES.contains(&"users"));
}
