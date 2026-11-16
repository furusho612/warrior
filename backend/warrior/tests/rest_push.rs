//! レスト終了のプッシュ通知の予約の結合テスト。
//!
//! 対応する要求: `REQ-TMR-008`、`REQ-TMR-009`、`REQ-TMR-010`、`REQ-TMR-011`。
//! 予約の作成、再予約、取消、所有者の分離、消費側の送信、受信時刻の記録、
//! トレーニングの終了とアカウントの削除による行の削除を、ローカルの D1 で確かめる。

mod support;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use support::{DevServer, HttpResponse};
use warrior_core::auth::{hash_session_token, hash_terminal_token};
use warrior_core::base64url;

const PORT: u16 = 8833;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a01";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a02";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a10";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a20";
const OTHER_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5a21";
const SESSION_SEED: u8 = 71;
const OTHER_SESSION_SEED: u8 = 72;
const TERMINAL_SEED: u8 = 73;
const OTHER_TERMINAL_SEED: u8 = 74;
const NOW: &str = "2026-11-12T03:00:00Z";
const DATE: &str = "2026-11-12";
const VAPID_PRIVATE_KEY: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
const VAPID_PUBLIC_KEY: &str =
    "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
const P256DH: &str =
    "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";

#[test]
fn the_rest_push_reservation_follows_the_timer_and_sends_once() {
    let server = DevServer::start_with_vars(
        PORT,
        &[
            ("VAPID_PRIVATE_KEY", VAPID_PRIVATE_KEY),
            ("VAPID_PUBLIC_KEY", VAPID_PUBLIC_KEY),
            ("VAPID_SUBJECT", "mailto:owner@example.test"),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed(&server);
    let cookie = cookies(SESSION_SEED, TERMINAL_SEED);
    let other_cookie = cookies(OTHER_SESSION_SEED, OTHER_TERMINAL_SEED);

    let created = create(&server, &cookie, TRAINING_ID, 30);
    assert_eq!(created.status, 201, "{}", created.body);
    let first: Value = serde_json::from_str(&created.body).expect("the reservation is JSON");
    let first_id = first["id"].as_str().expect("the reservation id").to_owned();
    assert_eq!(first["version"], 1);
    assert_eq!(
        rows(
            &server,
            "SELECT active, version FROM rest_push_reservations"
        )[0]["active"],
        1
    );

    let replaced = create(&server, &cookie, TRAINING_ID, 20);
    assert_eq!(replaced.status, 201, "{}", replaced.body);
    let second: Value = serde_json::from_str(&replaced.body).expect("the reservation is JSON");
    let second_id = second["id"]
        .as_str()
        .expect("the reservation id")
        .to_owned();
    assert_ne!(first_id, second_id);
    let all = rows(
        &server,
        "SELECT id, active, version FROM rest_push_reservations ORDER BY created_at, id",
    );
    assert_eq!(all.len(), 2);
    let old = all
        .iter()
        .find(|row| row["id"] == first_id)
        .expect("the old reservation");
    assert_eq!(old["active"], 0);
    assert_eq!(old["version"], 2);
    let current = all
        .iter()
        .find(|row| row["id"] == second_id)
        .expect("the current reservation");
    assert_eq!(current["active"], 1);
    assert_eq!(current["version"], 1);

    assert_eq!(
        patch(&server, &other_cookie, &second_id, 10).status,
        404,
        "another account must not change the reservation"
    );
    assert_eq!(
        delete(&server, &other_cookie, &second_id).status,
        404,
        "another account must not cancel the reservation"
    );

    let changed = patch(&server, &cookie, &second_id, 10);
    assert_eq!(changed.status, 200, "{}", changed.body);
    let changed: Value = serde_json::from_str(&changed.body).expect("the reservation is JSON");
    assert_eq!(changed["version"], 2);
    let changed_row = rows(
        &server,
        &format!("SELECT version FROM rest_push_reservations WHERE id = '{second_id}'"),
    );
    assert_eq!(changed_row[0]["version"], 2);

    assert_eq!(delete(&server, &cookie, &second_id).status, 204);
    let cancelled = rows(
        &server,
        &format!("SELECT active, version FROM rest_push_reservations WHERE id = '{second_id}'"),
    );
    assert_eq!(cancelled[0]["active"], 0);
    assert_eq!(cancelled[0]["version"], 3);

    // 送信まで待つ。送信の可否にかかわらず、送信の直前に sent_at を立てる。
    let sent = create(&server, &cookie, TRAINING_ID, 1);
    assert_eq!(sent.status, 201, "{}", sent.body);
    let sent: Value = serde_json::from_str(&sent.body).expect("the reservation is JSON");
    let sent_id = sent["id"].as_str().expect("the reservation id").to_owned();
    let end_at = sent["end_at"].as_i64().expect("the end time");
    wait_until(end_at);
    assert_eq!(
        server
            .request("GET", "/cdn-cgi/local/scheduled", &[])
            .status,
        200
    );
    let sent_row = wait_for_sent(&server, &sent_id);
    assert_eq!(sent_row["active"], 1, "the reservation stays active");
    assert!(sent_row["sent_at"].as_i64().is_some());

    let received = server.request_with_body(
        "POST",
        "/api/push/received",
        &[("Origin", ORIGIN), ("Cookie", cookie.as_str())],
        &json!({ "reservation_id": sent_id.clone(), "version": 1 }).to_string(),
    );
    assert_eq!(received.status, 204, "{}", received.body);
    let received_row = rows(
        &server,
        &format!("SELECT received_at FROM rest_push_reservations WHERE id = '{sent_id}'"),
    );
    assert!(received_row[0]["received_at"].as_i64().is_some());

    let finished = server.request_with_body(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", cookie.as_str())],
        "",
    );
    assert_eq!(finished.status, 200, "{}", finished.body);
    let active = rows(
        &server,
        "SELECT COUNT(*) AS count FROM rest_push_reservations WHERE active = 1",
    );
    assert_eq!(
        active[0]["count"], 0,
        "the training finish must cancel every active reservation"
    );

    let deleted = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Origin", ORIGIN), ("Cookie", cookie.as_str())],
        r#"{"confirm":"delete-account"}"#,
    );
    assert_eq!(
        deleted.status, 204,
        "the account deletion must remove the reservations: {}",
        deleted.body
    );
    let remaining = rows(
        &server,
        "SELECT COUNT(*) AS count FROM rest_push_reservations",
    );
    assert_eq!(remaining[0]["count"], 0);
}

fn create(server: &DevServer, cookie: &str, training_id: &str, seconds: u32) -> HttpResponse {
    server.request_with_body(
        "POST",
        "/api/rest/reservations",
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        &json!({
            "training_id": training_id,
            "remaining_seconds": seconds,
        })
        .to_string(),
    )
}

fn patch(server: &DevServer, cookie: &str, id: &str, seconds: u32) -> HttpResponse {
    server.request_with_body(
        "PATCH",
        &format!("/api/rest/reservations/{id}"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        &json!({ "remaining_seconds": seconds }).to_string(),
    )
}

fn delete(server: &DevServer, cookie: &str, id: &str) -> HttpResponse {
    server.request_with_body(
        "DELETE",
        &format!("/api/rest/reservations/{id}"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        "",
    )
}

fn wait_until(end_at: i64) {
    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock must be after the epoch")
            .as_secs();
        if now as i64 >= end_at {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_sent(server: &DevServer, id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let rows = rows(
            server,
            &format!("SELECT active, sent_at FROM rest_push_reservations WHERE id = '{id}'"),
        );
        if let Some(row) = rows.first() {
            if row["sent_at"].as_i64().is_some() {
                return row.clone();
            }
        }
        assert!(Instant::now() < deadline, "the reservation was not sent");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn rows(server: &DevServer, sql: &str) -> Vec<Value> {
    server
        .execute_d1_json(sql)
        .expect("the query must succeed")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn cookies(session_seed: u8, terminal_seed: u8) -> String {
    format!(
        "warrior_session={}; warrior_terminal={}",
        base64url::encode(&[session_seed; 32]),
        base64url::encode(&[terminal_seed; 32])
    )
}

fn seed(server: &DevServer) {
    let session_token = base64url::encode(&[SESSION_SEED; 32]);
    let other_session_token = base64url::encode(&[OTHER_SESSION_SEED; 32]);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-rest-push', 'rest-push@example.test', '利用者', \
             '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-rest-push-other', \
             'rest-push-other@example.test', '別の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5a30', '{USER_ID}', '{}', '{NOW}', \
             '2099-01-01T00:00:00Z')",
            hash_session_token(&session_token)
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5a31', '{OTHER_USER_ID}', '{}', \
             '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(&other_session_token)
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{TERMINAL_ID}', '{USER_ID}', '{}', '{NOW}', '{NOW}')",
            hash_terminal_token(&base64url::encode(&[TERMINAL_SEED; 32]))
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{OTHER_TERMINAL_ID}', '{OTHER_USER_ID}', '{}', '{NOW}', '{NOW}')",
            hash_terminal_token(&base64url::encode(&[OTHER_TERMINAL_SEED; 32]))
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, \
             owner_terminal_id, created_at, updated_at) \
             VALUES ('{TRAINING_ID}', '{USER_ID}', '{DATE}', '{NOW}', '', '{TERMINAL_ID}', \
             '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth, \
             terminal_token_id, created_at, updated_at) \
             VALUES ('{USER_ID}', 'https://fcm.googleapis.com/fcm/send/rest-push', '{P256DH}', \
             '{AUTH}', '{TERMINAL_ID}', '{NOW}', '{NOW}')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the rest push fixture must be seeded");
    }
}
