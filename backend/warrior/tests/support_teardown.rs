//! E2E の teardown が、テストの利用者の資源だけを削除することを確かめる。
//!
//! 対応する要求はない。ADR-024 が定める E2E の資源の分離を確かめる。

mod support;

use support::{unique_user_id, DevServer};

#[test]
fn wrangler_e2e_teardown_removes_only_the_test_users_prefixes() {
    let server = DevServer::start(8793);
    server.apply_migrations().expect("migrations must apply");

    let user_a = unique_user_id();
    let user_b = unique_user_id();
    insert_user(&server, &user_a, "google-a");
    insert_user(&server, &user_b, "google-b");

    let keys_a = [
        format!("users/{user_a}/dummy.txt"),
        format!("exports/{user_a}/dummy.csv"),
    ];
    let keys_b = [
        format!("users/{user_b}/dummy.txt"),
        format!("exports/{user_b}/dummy.csv"),
    ];
    for key in keys_a.iter().chain(keys_b.iter()) {
        server
            .put_r2_object(key, b"dummy")
            .unwrap_or_else(|error| panic!("{key}: {error}"));
    }

    teardown_user(&server, &user_a, &keys_a);

    assert!(server.get_r2_object(&keys_a[0]).unwrap().is_none());
    assert!(server.get_r2_object(&keys_a[1]).unwrap().is_none());
    assert_eq!(
        server.get_r2_object(&keys_b[0]).unwrap(),
        Some(b"dummy".to_vec())
    );
    assert_eq!(
        server.get_r2_object(&keys_b[1]).unwrap(),
        Some(b"dummy".to_vec())
    );
    assert_eq!(count_users(&server, &user_a), 0);
    assert_eq!(count_users(&server, &user_b), 1);

    teardown_user(&server, &user_b, &keys_b);
    assert!(server.get_r2_object(&keys_b[0]).unwrap().is_none());
    assert_eq!(count_users(&server, &user_b), 0);
}

fn insert_user(server: &DevServer, user_id: &str, google_sub: &str) {
    let sql = format!(
        "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
         VALUES ('{user_id}', '{google_sub}', '{google_sub}@example.test', 'Test', \
         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
    );
    server
        .execute_d1_json(&sql)
        .unwrap_or_else(|error| panic!("insert user: {error}"));
}

fn teardown_user(server: &DevServer, user_id: &str, keys: &[String]) {
    for key in keys {
        server
            .delete_r2_object(key)
            .unwrap_or_else(|error| panic!("delete {key}: {error}"));
    }
    server
        .execute_d1_json(&format!("DELETE FROM users WHERE id = '{user_id}'"))
        .unwrap_or_else(|error| panic!("delete user: {error}"));
}

fn count_users(server: &DevServer, user_id: &str) -> i64 {
    let result = server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM users WHERE id = '{user_id}'"
        ))
        .expect("count users");
    result
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_i64())
        .unwrap_or_else(|| panic!("the count must be an integer: {result}"))
}
