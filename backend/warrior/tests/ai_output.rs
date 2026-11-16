//! AI の評価、提案、メニュー、作り直しの取得の結合テスト。
//!
//! 対応する要求: `REQ-AI-001`、`REQ-AI-004`、`REQ-AI-006`、`REQ-AI-010`、`REQ-AI-013`。

mod support;

use serde_json::{json, Value};
use support::record_common::{cookie, first_row, seed_auth_session, seed_terminal, seed_user};
use support::DevServer;
use warrior_core::base64url;

const PORT: u16 = 8838;
const ORIGIN: &str = "http://localhost:8787";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c01";
const SESSION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c10";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c20";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c30";
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const CURL: &str = "a7c4e2f0-0000-4000-8000-00000000001d";
const EVALUATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c40";
const PROPOSAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c41";
const MENU_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c42";
const ROUTINE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c50";
const ROUTINE_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c51";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c02";
const OTHER_SESSION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c11";
const OTHER_TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5c21";
const SESSION_SEED: u8 = 91;
const TERMINAL_SEED: u8 = 92;

#[test]
fn generated_outputs_are_listed_and_a_redo_returns_to_pending() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed(&server);
    let cookie_a = cookie(
        &base64url::encode(&[SESSION_SEED; 32]),
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );
    let other_cookie = cookie(&base64url::encode(&[93; 32]), &base64url::encode(&[94; 32]));
    seed_proposal(&server);
    seed_menu(&server);

    let evaluations = get(&server, "/api/ai/evaluations?date=2026-11-10", &cookie_a);
    assert_eq!(evaluations.status, 200, "{}", evaluations.body);
    let evaluations: Value =
        serde_json::from_str(&evaluations.body).expect("the evaluations are JSON");
    let entries = evaluations["evaluations"]
        .as_array()
        .expect("the evaluation entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["status"], "generated");
    assert_eq!(
        entries[0]["evaluation"]["good"][0]["text"],
        "70 kg で 8 回できた"
    );
    assert_eq!(
        entries[0]["evaluation"]["good"][0]["certainty"],
        "assertion"
    );

    let proposals = get(
        &server,
        &format!("/api/ai/proposals?exercise_id={BENCH}"),
        &cookie_a,
    );
    assert_eq!(proposals.status, 200, "{}", proposals.body);
    let proposals: Value = serde_json::from_str(&proposals.body).expect("the proposals are JSON");
    let entries = proposals["proposals"]
        .as_array()
        .expect("the proposal entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["status"], "generated");
    assert_eq!(entries[0]["proposal"]["sets"], 4);
    assert_eq!(entries[0]["proposal"]["weight_kg"], 72.5);

    let updated = server.request_with_body(
        "PUT",
        &format!("/api/ai/proposals/{PROPOSAL_ID}"),
        &[("Origin", ORIGIN), ("Cookie", cookie_a.as_str())],
        &json!({
            "weight_kg": 75.0,
            "reps": 6,
            "duration_seconds": null,
            "sets": 5,
            "reason": "前回の記録に合わせて修正する"
        })
        .to_string(),
    );
    assert_eq!(updated.status, 200, "{}", updated.body);
    let proposals = get(
        &server,
        &format!("/api/ai/proposals?exercise_id={BENCH}"),
        &cookie_a,
    );
    let proposals: Value = serde_json::from_str(&proposals.body).expect("the proposals are JSON");
    assert_eq!(proposals["proposals"][0]["proposal"]["sets"], 5);
    assert_eq!(proposals["proposals"][0]["proposal"]["weight_kg"], 75.0);
    assert_eq!(
        proposals["proposals"][0]["proposal"]["reason"],
        "前回の記録に合わせて修正する"
    );

    let foreign_update = server.request_with_body(
        "PUT",
        &format!("/api/ai/proposals/{PROPOSAL_ID}"),
        &[("Origin", ORIGIN), ("Cookie", other_cookie.as_str())],
        &json!({
            "weight_kg": 80.0,
            "reps": 8,
            "duration_seconds": null,
            "sets": 3,
            "reason": "別の利用者"
        })
        .to_string(),
    );
    assert_eq!(foreign_update.status, 404);

    let menu = get(&server, "/api/ai/menu", &cookie_a);
    assert_eq!(menu.status, 200, "{}", menu.body);
    let menu: Value = serde_json::from_str(&menu.body).expect("the menu is JSON");
    assert_eq!(menu["status"], "generated");
    assert_eq!(menu["changes"][0]["kind"], "replace_exercise");
    assert_eq!(menu["changes"][0]["target_exercise_id"], CURL);

    let applied = post_json(
        &server,
        "/api/ai/menu/apply",
        &cookie_a,
        &json!({ "changes": menu["changes"].clone() }),
    );
    assert_eq!(applied.status, 200, "{}", applied.body);
    assert_eq!(
        count(
            &server,
            &format!(
                "SELECT COUNT(*) AS count FROM routine_exercises WHERE id = '{ROUTINE_EXERCISE_ID}' \
                 AND exercise_id = '{CURL}'"
            )
        ),
        1
    );

    let detail = get(
        &server,
        &format!("/api/evaluations/{EVALUATION_ID}"),
        &cookie_a,
    );
    assert_eq!(detail.status, 200, "{}", detail.body);
    let detail: Value = serde_json::from_str(&detail.body).expect("the detail is JSON");
    assert_eq!(detail["date"], "2026-11-10");
    assert_eq!(detail["goal"], "hypertrophy");
    assert_eq!(detail["evaluation"]["good"][0]["evidence"], "70 kg を 8 回");

    let foreign = get(
        &server,
        &format!("/api/evaluations/{EVALUATION_ID}"),
        &other_cookie,
    );
    assert_eq!(foreign.status, 404);

    let redo = post(
        &server,
        &format!("/api/ai/jobs/{EVALUATION_ID}/redo"),
        &cookie_a,
    );
    assert_eq!(redo.status, 200, "{}", redo.body);
    let redo: Value = serde_json::from_str(&redo.body).expect("the redo is JSON");
    assert_eq!(redo["status"], "pending");
    assert_eq!(
        count(&server, &format!("SELECT COUNT(*) AS count FROM ai_generation WHERE id = '{EVALUATION_ID}' AND status = 'pending'")),
        1
    );

    let foreign = get(
        &server,
        &format!("/api/ai/jobs/{EVALUATION_ID}"),
        &cookie(&base64url::encode(&[93; 32]), &base64url::encode(&[94; 32])),
    );
    assert_eq!(foreign.status, 404);
}

fn seed(server: &DevServer) {
    seed_user(
        server,
        USER_ID,
        "ai-output-user",
        "ai@example.test",
        "利用者",
    );
    seed_auth_session(
        server,
        SESSION_ID,
        USER_ID,
        &base64url::encode(&[SESSION_SEED; 32]),
    );
    seed_terminal(
        server,
        TERMINAL_ID,
        USER_ID,
        &base64url::encode(&[TERMINAL_SEED; 32]),
    );
    seed_user(
        server,
        OTHER_USER_ID,
        "ai-output-other",
        "ai-other@example.test",
        "別の利用者",
    );
    seed_auth_session(
        server,
        OTHER_SESSION_ID,
        OTHER_USER_ID,
        &base64url::encode(&[93; 32]),
    );
    seed_terminal(
        server,
        OTHER_TERMINAL_ID,
        OTHER_USER_ID,
        &base64url::encode(&[94; 32]),
    );
    let statements = [
        format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
             training_experience, goal, training_days_per_week, activity_level, created_at, \
             updated_at) VALUES ('{USER_ID}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', \
             'hypertrophy', 4, 'normal', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, finished_at, memo, \
             source_kind, created_at, updated_at) VALUES ('{TRAINING_ID}', '{USER_ID}', \
             '2026-11-10', '2026-11-10T09:00:00Z', '2026-11-10T10:00:00Z', '', 'empty', \
             '2026-11-10T09:00:00Z', '2026-11-10T09:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_generation (id, user_id, kind, training_id, exercise_id, period_start, \
             period_end, unique_key, status, attempts, output_id, created_at, updated_at) VALUES \
             ('{EVALUATION_ID}', '{USER_ID}', 'daily_evaluation', '{TRAINING_ID}', NULL, NULL, \
             NULL, 'daily:2026-11-10', 'generated', 1, '{EVALUATION_ID}', \
             '2026-11-10T10:00:00Z', '2026-11-10T10:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_outputs (id, user_id, kind, training_id, exercise_id, model, \
             prompt_version, input_key, output_key, created_at) VALUES ('{EVALUATION_ID}', \
             '{USER_ID}', 'daily_evaluation', '{TRAINING_ID}', NULL, 'test-model', 'v1', \
             'users/{USER_ID}/ai/{EVALUATION_ID}/input.json', \
             'users/{USER_ID}/ai/{EVALUATION_ID}/output.json', '2026-11-10T10:00:00Z')"
        ),
    ];
    for sql in statements {
        server
            .execute_d1_json(&sql)
            .expect("the ai generation must be seeded");
    }
    let snapshot = json!({
        "exercise_id": BENCH,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [{"muscle": "pectoralis_major_middle", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    for sql in [
        format!(
            "INSERT INTO routines (id, user_id, name, position, created_at, updated_at) \
             VALUES ('{ROUTINE_ID}', '{USER_ID}', '胸の日', 1, '2026-11-10T09:00:00Z', \
             '2026-11-10T09:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercises (id, user_id, routine_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{ROUTINE_EXERCISE_ID}', '{USER_ID}', \
             '{ROUTINE_ID}', '{BENCH}', '{snapshot}', 1, '2026-11-10T09:00:00Z', \
             '2026-11-10T09:00:00Z')"
        ),
        format!(
            "INSERT INTO routine_exercise_targets (routine_exercise_id, user_id, set_count, \
             target_reps, rest_seconds, created_at, updated_at) VALUES ('{ROUTINE_EXERCISE_ID}', \
             '{USER_ID}', 3, 8, 90, '2026-11-10T09:00:00Z', '2026-11-10T09:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the routine must be seeded");
    }
    let output = json!({
        "model": "test-model",
        "prompt_version": "v1",
        "output": {
            "evaluation": {
                "good": [{"text": "70 kg で 8 回できた", "evidence": "70 kg を 8 回",
                    "certainty": "assertion"}],
                "improvements": [],
                "next_actions": []
            }
        }
    });
    server
        .put_r2_object(
            &format!("users/{USER_ID}/ai/{EVALUATION_ID}/output.json"),
            output.to_string().as_bytes(),
        )
        .expect("the evaluation output must be stored");
}

fn seed_proposal(server: &DevServer) {
    let output_key = format!("users/{USER_ID}/ai/{PROPOSAL_ID}/output.json");
    for sql in [
        format!(
            "INSERT INTO ai_generation (id, user_id, kind, training_id, exercise_id, period_start, \
             period_end, unique_key, status, attempts, output_id, created_at, updated_at) VALUES \
             ('{PROPOSAL_ID}', '{USER_ID}', 'set_proposal', '{TRAINING_ID}', '{BENCH}', NULL, \
             NULL, 'proposal:{TRAINING_ID}:{BENCH}', 'generated', 1, '{PROPOSAL_ID}', \
             '2026-11-10T10:00:00Z', '2026-11-10T10:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_outputs (id, user_id, kind, training_id, exercise_id, model, \
             prompt_version, input_key, output_key, created_at) VALUES ('{PROPOSAL_ID}', \
             '{USER_ID}', 'set_proposal', '{TRAINING_ID}', '{BENCH}', 'test-model', 'v1', \
             'users/{USER_ID}/ai/{PROPOSAL_ID}/input.json', '{output_key}', \
             '2026-11-10T10:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the proposal must be seeded");
    }
    let output = json!({
        "model": "test-model",
        "prompt_version": "v1",
        "output": {
            "proposals": [{"exercise_id": BENCH, "weight_kg": 72.5, "reps": 8, "sets": 4,
                "reason": "前回は全セットで目標回数に達したため 2.5 kg 増やす"}]
        }
    });
    server
        .put_r2_object(&output_key, output.to_string().as_bytes())
        .expect("the proposal output must be stored");
}

fn seed_menu(server: &DevServer) {
    let output_key = format!("users/{USER_ID}/ai/{MENU_ID}/output.json");
    for sql in [
        format!(
            "INSERT INTO ai_generation (id, user_id, kind, training_id, exercise_id, period_start, \
             period_end, unique_key, status, attempts, output_id, created_at, updated_at) VALUES \
             ('{MENU_ID}', '{USER_ID}', 'menu_optimization', NULL, NULL, '2026-11-04', \
             '2026-11-10', 'menu:2026-11-10', 'generated', 1, '{MENU_ID}', \
             '2026-11-10T10:00:00Z', '2026-11-10T10:00:00Z')"
        ),
        format!(
            "INSERT INTO ai_outputs (id, user_id, kind, training_id, exercise_id, model, \
             prompt_version, input_key, output_key, created_at) VALUES ('{MENU_ID}', '{USER_ID}', \
             'menu_optimization', NULL, NULL, 'test-model', 'v1', \
             'users/{USER_ID}/ai/{MENU_ID}/input.json', '{output_key}', '2026-11-10T10:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the menu must be seeded");
    }
    let output = json!({
        "model": "test-model",
        "prompt_version": "v1",
        "output": {
            "menu_changes": [{"kind": "replace_exercise", "exercise_id": BENCH,
                "target_exercise_id": CURL, "reason": "胸の種目が重複しているため"}]
        }
    });
    server
        .put_r2_object(&output_key, output.to_string().as_bytes())
        .expect("the menu output must be stored");
}

fn get(server: &DevServer, path: &str, cookie: &str) -> support::HttpResponse {
    server.request("GET", path, &[("Origin", ORIGIN), ("Cookie", cookie)])
}

fn post_json(server: &DevServer, path: &str, cookie: &str, body: &Value) -> support::HttpResponse {
    server.request_with_body(
        "POST",
        path,
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        &body.to_string(),
    )
}

fn post(server: &DevServer, path: &str, cookie: &str) -> support::HttpResponse {
    server.request_with_body(
        "POST",
        path,
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        "{}",
    )
}

fn count(server: &DevServer, sql: &str) -> i64 {
    first_row(server, sql)["count"].as_i64().expect("a count")
}
