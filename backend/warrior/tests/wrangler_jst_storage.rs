//! 日本時間の日付を D1 に文字列として保存し、読み出す結合テスト。
//!
//! 対応する要求はない。ADR-005 が定める日付の保存形式の内部条件を確かめる。

mod support;

use support::DevServer;

#[test]
fn wrangler_jst_date_string_round_trips_through_d1() {
    let server = DevServer::start(8794);
    server.apply_migrations().expect("migrations must apply");
    server
        .execute_d1_json("CREATE TABLE jst_check (date TEXT NOT NULL)")
        .expect("the test table must be created");
    server
        .execute_d1_json("INSERT INTO jst_check (date) VALUES ('2026-11-12')")
        .expect("the date must be inserted");

    let result = server
        .execute_d1_json("SELECT date FROM jst_check")
        .expect("the date must be read");
    let date = result
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("date"))
        .and_then(|value| value.as_str())
        .expect("a date string");

    assert_eq!(date, "2026-11-12");
}
