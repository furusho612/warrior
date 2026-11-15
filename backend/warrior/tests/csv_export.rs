//! 記録の CSV の書き出しとダウンロードの結合テスト。
//!
//! 対応する要求: `REQ-MYP-008`、`REQ-NFR-019`。
//! ローカルの D1 と R2 を使い、書き出しの保存、メタデータ、認証済みのダウンロード、
//! 認証なしの 401、別のアカウントと期限切れの 404、アカウント削除を確かめる。

mod support;

use serde_json::{json, Value};
use support::DevServer;
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::csv::{self, COLUMNS};

const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6401";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6402";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6410";
const ORIGIN: (&str, &str) = ("Origin", "http://localhost:8787");
const SESSION_MEMO: &str = "腰が痛い; 次回は軽くする \\ 注意";
const EXERCISE_MEMO: &str = "フォーム; 肘を閉じる \\ ゆっくり";
const EXERCISE_NAME: &str = "=ベンチプレス, \"重い\"";

#[tokio::test]
async fn req_myp_008_the_export_is_stored_in_r2_and_downloaded_with_authentication() {
    let server = DevServer::start(8868);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_user(&server, USER_ID, &base64url::encode(&[7u8; 32]));
    seed_user(&server, OTHER_USER_ID, &base64url::encode(&[8u8; 32]));
    seed_training(&server, USER_ID);
    let cookie = format!("warrior_session={}", base64url::encode(&[7u8; 32]));
    let other_cookie = format!("warrior_session={}", base64url::encode(&[8u8; 32]));

    let without_auth = server.request("POST", "/api/exports", &[ORIGIN]);
    assert_eq!(without_auth.status, 401);

    let created = server.request(
        "POST",
        "/api/exports",
        &[("Cookie", cookie.as_str()), ORIGIN],
    );
    assert_eq!(created.status, 201);
    let job: Value = serde_json::from_str(&created.body).expect("the export response is JSON");
    let export_id = job["id"].as_str().expect("the export id").to_owned();
    assert_eq!(job["status"], "ready");
    assert!(!job["created_at"].as_str().unwrap_or_default().is_empty());
    assert!(
        job.get("r2_key").is_none(),
        "the response must not expose the R2 key"
    );

    let r2_key = format!("exports/{USER_ID}/{export_id}.csv");
    assert_eq!(
        d1_text(
            &server,
            &format!(
                "SELECT r2_key FROM export_jobs WHERE id = '{export_id}' AND user_id = '{USER_ID}'"
            ),
            "r2_key"
        ),
        r2_key
    );
    assert_eq!(
        d1_text(
            &server,
            &format!("SELECT status FROM export_jobs WHERE id = '{export_id}'"),
            "status"
        ),
        "ready"
    );
    assert!(!d1_text(
        &server,
        &format!("SELECT created_at FROM export_jobs WHERE id = '{export_id}'"),
        "created_at"
    )
    .is_empty());

    let stored = server
        .get_r2_object(&r2_key)
        .expect("the R2 read must work")
        .expect("the export object must exist");
    assert!(stored.starts_with(b"\xEF\xBB\xBF"));
    let stored_text = String::from_utf8(stored).expect("the export is UTF-8");
    let rows = csv::parse_document(&stored_text).expect("the export is CSV");
    assert_eq!(rows.len(), 6, "the header and five sets");
    assert_eq!(rows[0], COLUMNS);
    assert_eq!(rows[1][4], "ウォームアップ");
    assert_eq!(rows[2][4], "通常");
    assert_eq!(rows[3][4], "ドロップセット");
    assert_eq!(rows[3][5], "100;80");
    assert_eq!(rows[3][6], "8;6");
    assert_eq!(rows[4][5], "10/12.5");
    assert_eq!(rows[4][6], "5/4");
    assert_eq!(rows[4][9], "2/1");
    assert_eq!(rows[5][5], "");
    assert_eq!(rows[5][6], "");
    assert_eq!(rows[5][7], "90");
    assert_eq!(rows[2][8], "8.0");
    assert_eq!(rows[3][8], "9.0");
    assert_eq!(rows[1][8], "");
    assert_eq!(rows[2][10], "順手 ワイド");
    assert_eq!(rows[1][10], "");
    assert_eq!(rows[1][2], "胸");
    assert_eq!(rows[1][1], "'=ベンチプレス, \"重い\"");
    assert_eq!(rows[1][0], "2026-11-12");
    assert_eq!(
        csv::split_memos(&rows[1][11]),
        vec![SESSION_MEMO.to_owned(), EXERCISE_MEMO.to_owned()]
    );
    assert_eq!(rows[1][11].matches(';').count(), 3);

    let downloaded = server.request(
        "GET",
        &format!("/api/exports/{export_id}"),
        &[("Cookie", cookie.as_str())],
    );
    assert_eq!(downloaded.status, 200);
    assert_eq!(
        downloaded.header("content-type"),
        Some("text/csv; charset=utf-8")
    );
    let disposition = downloaded.header("content-disposition").unwrap_or_default();
    assert!(disposition.starts_with("attachment;"));
    assert!(disposition.contains(".csv"));
    assert!(downloaded.body.starts_with('\u{feff}'));
    assert_eq!(downloaded.body, stored_text);

    let without_cookie = server.request("GET", &format!("/api/exports/{export_id}"), &[]);
    assert_eq!(without_cookie.status, 401);

    let unknown = server.request(
        "GET",
        "/api/exports/3f2504e0-4f89-41d3-9a0c-0305e82c6499",
        &[("Cookie", cookie.as_str())],
    );
    assert_eq!(unknown.status, 404);

    server
        .execute_d1_json(&format!(
            "INSERT INTO export_jobs (id, user_id, status, r2_key, created_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c6498', '{OTHER_USER_ID}', 'ready', \
             'exports/{OTHER_USER_ID}/other.csv', '2026-11-12T00:00:00Z')"
        ))
        .expect("the other user's export must be seeded");
    let other = server.request(
        "GET",
        "/api/exports/3f2504e0-4f89-41d3-9a0c-0305e82c6498",
        &[("Cookie", cookie.as_str())],
    );
    assert_eq!(other.status, 404, "another account's export is hidden");
    let other_owner = server.request(
        "GET",
        "/api/exports/3f2504e0-4f89-41d3-9a0c-0305e82c6498",
        &[("Cookie", other_cookie.as_str())],
    );
    assert_eq!(
        other_owner.status, 404,
        "a missing R2 object is not found even for the owner"
    );

    server
        .delete_r2_object(&r2_key)
        .expect("the export object must be deleted");
    let expired = server.request(
        "GET",
        &format!("/api/exports/{export_id}"),
        &[("Cookie", cookie.as_str())],
    );
    assert_eq!(expired.status, 404, "the expired file is not found");
    assert_eq!(export_count(&server, USER_ID), 1);
}

#[tokio::test]
async fn req_nfr_019_account_deletion_removes_the_export_file_and_metadata() {
    let server = DevServer::start(8869);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed_user(&server, USER_ID, &base64url::encode(&[9u8; 32]));
    seed_training(&server, USER_ID);
    let cookie = format!("warrior_session={}", base64url::encode(&[9u8; 32]));

    let created = server.request(
        "POST",
        "/api/exports",
        &[("Cookie", cookie.as_str()), ORIGIN],
    );
    assert_eq!(created.status, 201);
    let job: Value = serde_json::from_str(&created.body).expect("the export response is JSON");
    let export_id = job["id"].as_str().expect("the export id").to_owned();
    let r2_key = format!("exports/{USER_ID}/{export_id}.csv");
    assert!(server
        .get_r2_object(&r2_key)
        .expect("the R2 read must work")
        .is_some());
    assert_eq!(export_count(&server, USER_ID), 1);

    let deleted = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Cookie", cookie.as_str()), ORIGIN],
        r#"{"confirm":"delete-account"}"#,
    );
    assert_eq!(deleted.status, 204);
    assert_eq!(export_count(&server, USER_ID), 0);
    assert_eq!(
        server
            .get_r2_object(&r2_key)
            .expect("the R2 read must work"),
        None
    );
    let after = server.request(
        "GET",
        &format!("/api/exports/{export_id}"),
        &[("Cookie", cookie.as_str())],
    );
    assert_eq!(after.status, 401);
}

fn seed_user(server: &DevServer, user_id: &str, session_token: &str) {
    let session_hash = hash_session_token(session_token);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{user_id}', 'sub-{user_id}', '{user_id}@example.test', 'テスト利用者', \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('{user_id}-session', '{user_id}', '{session_hash}', \
             '2026-01-01T00:00:00Z', '2099-01-01T00:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the user rows must be seeded");
    }
}

fn seed_training(server: &DevServer, user_id: &str) {
    let bench = json!({
        "exercise_id": "master-bench",
        "name": EXERCISE_NAME,
        "body_part": "chest",
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "grip",
        "grip_options": [{"grip": {"style": "pronated", "width": "wide"}}]
    })
    .to_string();
    let dumbbell = json!({
        "exercise_id": "master-dumbbell",
        "name": "ダンベルフライ",
        "body_part": "chest",
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "unilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    let plank = json!({
        "exercise_id": "master-plank",
        "name": "プランク",
        "body_part": "abs",
        "muscles": [],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    let grip = json!({"grip": {"style": "pronated", "width": "wide"}}).to_string();
    let statements = [
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{TRAINING_ID}', '{user_id}', '2026-11-12', '2026-11-12T03:00:00Z', \
             '{SESSION_MEMO}', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, memo, created_at, updated_at) \
             VALUES ('{TRAINING_ID}-bench', '{user_id}', '{TRAINING_ID}', 'master-bench', \
             '{bench}', 0, '{EXERCISE_MEMO}', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z'), \
             ('{TRAINING_ID}-dumbbell', '{user_id}', '{TRAINING_ID}', 'master-dumbbell', \
             '{dumbbell}', 1, '', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z'), \
             ('{TRAINING_ID}-plank', '{user_id}', '{TRAINING_ID}', 'master-plank', \
             '{plank}', 2, '', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z')"
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, rpe, grip, created_at, updated_at) \
             VALUES ('{TRAINING_ID}-warmup', '{user_id}', '{TRAINING_ID}-bench', 1, 'warmup', 1, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-normal', '{user_id}', '{TRAINING_ID}-bench', 2, 'normal', 1, 8.0, '{grip}', \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-drop', '{user_id}', '{TRAINING_ID}-bench', 3, 'drop', 1, 9.0, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-side', '{user_id}', '{TRAINING_ID}-dumbbell', 1, 'normal', 1, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-time', '{user_id}', '{TRAINING_ID}-plank', 1, 'normal', 1, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, duration_seconds, left_weight_kg, left_reps, right_weight_kg, right_reps, created_at, updated_at) \
             VALUES ('{TRAINING_ID}-warmup-1', '{user_id}', '{TRAINING_ID}-warmup', 1, 60.0, 10, 0, 0, NULL, NULL, NULL, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-normal-1', '{user_id}', '{TRAINING_ID}-normal', 1, 100.0, 5, 0, 0, NULL, NULL, NULL, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-drop-1', '{user_id}', '{TRAINING_ID}-drop', 1, 100.0, 8, 0, 0, NULL, NULL, NULL, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-drop-2', '{user_id}', '{TRAINING_ID}-drop', 2, 80.0, 6, 0, 0, NULL, NULL, NULL, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-side-1', '{user_id}', '{TRAINING_ID}-side', 1, NULL, NULL, 2, 1, NULL, 10.0, 5, 12.5, 4, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-time-1', '{user_id}', '{TRAINING_ID}-time', 1, NULL, NULL, 0, 0, 90, NULL, NULL, NULL, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
    ];
    for sql in statements {
        server
            .execute_d1_json(&sql)
            .expect("the training rows must be seeded");
    }
}

fn d1_text(server: &DevServer, sql: &str, column: &str) -> String {
    server
        .execute_d1_json(sql)
        .expect("the query must run")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get(column))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn export_count(server: &DevServer, user_id: &str) -> i64 {
    server
        .execute_d1_json(&format!(
            "SELECT COUNT(*) AS count FROM export_jobs WHERE user_id = '{user_id}'"
        ))
        .expect("the count must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_i64())
        .expect("a count")
}
