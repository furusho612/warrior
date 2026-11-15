//! 独自種目の追加、編集、削除と、マスターの種目の編集、削除、初期化の結合テスト。
//!
//! 対応する要求: `REQ-LIB-005`、`REQ-LIB-013`。
//! 名称、読み、部位区分、記録方式の欠落を拒否すること、独自種目の往復、マスターの表示の
//! 上書きと初期化、削除の復活、利用者ごとの分離、過去の記録のスナップショットの不変を
//! 確かめる。

mod support;

use serde_json::{json, Value};
use support::{DevServer, HttpResponse};
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::exercise::ExerciseInput;
use warrior_core::library::{ExerciseChange, ExerciseList, ExerciseSource, LibraryExercise};

const PORT: u16 = 8870;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4c01";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4c02";
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";

#[test]
fn custom_exercises_and_master_overrides_are_saved_per_user() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[51u8; 32]);
    let other_token = base64url::encode(&[52u8; 32]);
    seed_users(&server, &token, &other_token);
    let cookie = format!("warrior_session={token}");
    let other_cookie = format!("warrior_session={other_token}");

    check_required_fields(&server, &cookie);
    let custom_id = check_custom_exercise(&server, &cookie);
    check_other_user_cannot_change_custom(&server, &other_cookie, &custom_id);
    check_custom_delete(&server, &cookie, &custom_id);
    check_master_override(&server, &cookie, &other_cookie);
    check_master_delete_and_reset(&server, &cookie);
    check_snapshot_is_unchanged(&server, &cookie);
}

fn check_required_fields(server: &DevServer, cookie: &str) {
    for input in [
        json!({
            "name": " ",
            "name_kana": "から",
            "body_part": "chest",
            "record_mode": "weight_and_reps"
        }),
        json!({
            "name": "追加",
            "name_kana": "ついか",
            "body_part": "chest"
        }),
    ] {
        let response = mutate(server, "POST", "/api/exercises", cookie, input, &new_id());
        assert_eq!(response.status, 400, "{}", response.body);
    }
}

fn check_custom_exercise(server: &DevServer, cookie: &str) -> String {
    let created = mutate(
        server,
        "POST",
        "/api/exercises",
        cookie,
        custom_input("ケーブルプレス"),
        &new_id(),
    );
    assert_eq!(created.status, 200, "{}", created.body);
    let change: ExerciseChange = serde_json::from_str(&created.body).expect("the change");
    assert_eq!(change.source, ExerciseSource::Custom);
    assert!(!change.deleted);
    let id = change.id;

    let list = list(server, cookie);
    assert!(list
        .exercises
        .iter()
        .any(|item| item.exercise.id == id && item.source == ExerciseSource::Custom));
    let detail = fetch_detail(server, cookie, &id);
    assert_eq!(detail.exercise.name, "ケーブルプレス");
    assert_eq!(
        detail.exercise.body_part,
        warrior_core::exercise::BodyPart::Chest
    );
    assert_eq!(
        detail.exercise.equipment,
        warrior_core::exercise::Equipment::Cable
    );
    assert_eq!(
        detail.exercise.record_mode,
        warrior_core::exercise::RecordMode::WeightAndReps
    );

    let mut edited = ExerciseInput::from_exercise(&detail.exercise);
    edited.name = "ケーブルプレス(編集)".to_owned();
    edited.tips = "胸を張る".to_owned();
    let updated = mutate(
        server,
        "PUT",
        &format!("/api/exercises/{id}"),
        cookie,
        serde_json::to_value(&edited).expect("the exercise input"),
        &new_id(),
    );
    assert_eq!(updated.status, 200, "{}", updated.body);
    let detail = fetch_detail(server, cookie, &id);
    assert_eq!(detail.exercise.name, "ケーブルプレス(編集)");
    assert_eq!(detail.exercise.tips, "胸を張る");

    id
}

fn check_custom_delete(server: &DevServer, cookie: &str, id: &str) {
    let deleted = mutate(
        server,
        "DELETE",
        &format!("/api/exercises/{id}"),
        cookie,
        Value::Null,
        &new_id(),
    );
    assert_eq!(deleted.status, 200, "{}", deleted.body);
    assert!(!list(server, cookie)
        .exercises
        .iter()
        .any(|item| item.exercise.id == id));
    assert_eq!(
        server
            .request(
                "GET",
                &format!("/api/exercises/{id}"),
                &[("Cookie", cookie)]
            )
            .status,
        404
    );
}

fn check_other_user_cannot_change_custom(server: &DevServer, cookie: &str, id: &str) {
    let detail = fetch_detail(server, cookie, id);
    let input = ExerciseInput::from_exercise(&detail.exercise);
    let update = mutate(
        server,
        "PUT",
        &format!("/api/exercises/{id}"),
        cookie,
        serde_json::to_value(&input).expect("the exercise input"),
        &new_id(),
    );
    assert_eq!(update.status, 404, "{}", update.body);
    let delete = mutate(
        server,
        "DELETE",
        &format!("/api/exercises/{id}"),
        cookie,
        Value::Null,
        &new_id(),
    );
    assert_eq!(delete.status, 404, "{}", delete.body);
}

fn check_master_override(server: &DevServer, cookie: &str, other_cookie: &str) {
    let master = fetch_detail(server, cookie, BENCH);
    assert_eq!(master.source, ExerciseSource::Master);
    let mut input = ExerciseInput::from_exercise(&master.exercise);
    input.name = "ベンチプレス(利用者)".to_owned();
    input.tips = "自分のコツ".to_owned();
    let updated = mutate(
        server,
        "PUT",
        &format!("/api/exercises/{BENCH}"),
        cookie,
        serde_json::to_value(&input).expect("the exercise input"),
        &new_id(),
    );
    assert_eq!(updated.status, 200, "{}", updated.body);
    let change: ExerciseChange = serde_json::from_str(&updated.body).expect("the change");
    assert_eq!(change.source, ExerciseSource::Overridden);

    let own = fetch_detail(server, cookie, BENCH);
    assert_eq!(own.source, ExerciseSource::Overridden);
    assert_eq!(own.exercise.name, "ベンチプレス(利用者)");
    assert_eq!(own.exercise.tips, "自分のコツ");
    let other = fetch_detail(server, other_cookie, BENCH);
    assert_eq!(other.source, ExerciseSource::Master);
    assert_eq!(other.exercise.name, "ベンチプレス");
    assert_eq!(other.exercise.tips, "");

    let reset = mutate(
        server,
        "POST",
        &format!("/api/exercises/{BENCH}/reset"),
        cookie,
        Value::Null,
        &new_id(),
    );
    assert_eq!(reset.status, 200, "{}", reset.body);
    let change: ExerciseChange = serde_json::from_str(&reset.body).expect("the change");
    assert_eq!(change.source, ExerciseSource::Master);
    let own = fetch_detail(server, cookie, BENCH);
    assert_eq!(own.exercise.name, "ベンチプレス");
    assert_eq!(own.source, ExerciseSource::Master);
}

fn check_master_delete_and_reset(server: &DevServer, cookie: &str) {
    let deleted = mutate(
        server,
        "DELETE",
        &format!("/api/exercises/{BENCH}"),
        cookie,
        Value::Null,
        &new_id(),
    );
    assert_eq!(deleted.status, 200, "{}", deleted.body);
    assert!(!list(server, cookie)
        .exercises
        .iter()
        .any(|item| item.exercise.id == BENCH));
    let hidden = fetch_detail(server, cookie, BENCH);
    assert!(hidden.deleted);
    assert_eq!(hidden.source, ExerciseSource::Overridden);

    let reset = mutate(
        server,
        "POST",
        &format!("/api/exercises/{BENCH}/reset"),
        cookie,
        Value::Null,
        &new_id(),
    );
    assert_eq!(reset.status, 200, "{}", reset.body);
    let detail = fetch_detail(server, cookie, BENCH);
    assert_eq!(detail.exercise.name, "ベンチプレス");
    assert_eq!(detail.source, ExerciseSource::Master);
    assert!(!detail.deleted);
}

fn check_snapshot_is_unchanged(server: &DevServer, cookie: &str) {
    let snapshot = json!({
        "exercise_id": BENCH,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4d01', '{USER_ID}', '2026-11-12', '2026-11-12T00:00:00Z', '', '2026-11-12T00:00:00Z', '2026-11-12T00:00:00Z')"
        ))
        .expect("the training must be seeded");
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4d02', '{USER_ID}', '3f2504e0-4f89-41d3-9a0c-0305e82c4d01', '{BENCH}', '{}', 1, '2026-11-12T00:00:00Z', '2026-11-12T00:00:00Z')",
            snapshot.to_string().replace('\'', "''")
        ))
        .expect("the training exercise must be seeded");

    let master = fetch_detail(server, cookie, BENCH);
    let mut input = ExerciseInput::from_exercise(&master.exercise);
    input.name = "変更後の種目名".to_owned();
    mutate(
        server,
        "PUT",
        &format!("/api/exercises/{BENCH}"),
        cookie,
        serde_json::to_value(&input).expect("the exercise input"),
        &new_id(),
    );

    let result = server
        .execute_d1_json(
            "SELECT snapshot FROM training_exercises WHERE id = '3f2504e0-4f89-41d3-9a0c-0305e82c4d02'",
        )
        .expect("the snapshot must be readable");
    let text = result.to_string();
    assert!(text.contains("ベンチプレス"), "{text}");
    assert!(!text.contains("変更後の種目名"), "{text}");

    let deleted = mutate(
        server,
        "DELETE",
        &format!("/api/exercises/{BENCH}"),
        cookie,
        Value::Null,
        &new_id(),
    );
    assert_eq!(deleted.status, 200, "{}", deleted.body);
    let result = server
        .execute_d1_json(
            "SELECT snapshot FROM training_exercises WHERE id = '3f2504e0-4f89-41d3-9a0c-0305e82c4d02'",
        )
        .expect("the snapshot must be readable");
    let text = result.to_string();
    assert!(text.contains("ベンチプレス"), "{text}");
    assert!(!text.contains("変更後の種目名"), "{text}");
    let _ = mutate(
        server,
        "POST",
        &format!("/api/exercises/{BENCH}/reset"),
        cookie,
        Value::Null,
        &new_id(),
    );
}

fn custom_input(name: &str) -> Value {
    json!({
        "name": name,
        "name_kana": "けーぶるぷれす",
        "body_part": "chest",
        "equipment": "cable",
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "instructions": "押す",
        "cautions": "肩を守る",
        "tips": "胸を張る",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "grip_options": []
    })
}

fn mutate(
    server: &DevServer,
    method: &str,
    path: &str,
    cookie: &str,
    body: Value,
    operation_id: &str,
) -> HttpResponse {
    let body = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    server.request_with_body(
        method,
        path,
        &[
            ("Cookie", cookie),
            ("Origin", ORIGIN),
            ("X-Operation-Id", operation_id),
            ("Content-Type", "application/json"),
        ],
        &body,
    )
}

fn list(server: &DevServer, cookie: &str) -> ExerciseList {
    let response = server.request("GET", "/api/exercises", &[("Cookie", cookie)]);
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the exercise list response")
}

fn fetch_detail(server: &DevServer, cookie: &str, id: &str) -> LibraryExercise {
    let response = server.request(
        "GET",
        &format!("/api/exercises/{id}"),
        &[("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the exercise detail response")
}

fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let value = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("3f2504e0-4f89-41d3-9a0c-{value:012x}")
}

fn seed_users(server: &DevServer, token: &str, other_token: &str) {
    let now = "2026-11-12T00:00:00Z";
    for (user_id, sub, email, name) in [
        (
            USER_ID,
            "sub-exercise-change",
            "exercise-change@example.test",
            "種目の利用者",
        ),
        (
            OTHER_USER_ID,
            "sub-other-exercise-change",
            "other-exercise-change@example.test",
            "別の利用者",
        ),
    ] {
        server
            .execute_d1_json(&format!(
                "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
                 VALUES ('{user_id}', '{sub}', '{email}', '{name}', '{now}', '{now}')"
            ))
            .expect("the user must be seeded");
    }
    for (session_id, user_id, token) in [
        ("3f2504e0-4f89-41d3-9a0c-0305e82c4c10", USER_ID, token),
        (
            "3f2504e0-4f89-41d3-9a0c-0305e82c4c11",
            OTHER_USER_ID,
            other_token,
        ),
    ] {
        server
            .execute_d1_json(&format!(
                "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
                 VALUES ('{session_id}', '{user_id}', '{}', '{now}', '2099-01-01T00:00:00Z')",
                hash_session_token(token)
            ))
            .expect("the session must be seeded");
    }
}
