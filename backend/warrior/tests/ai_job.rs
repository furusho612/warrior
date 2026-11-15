//! AI の生成ジョブの結合テスト。
//!
//! 対応する要求: `REQ-AI-014`、`REQ-AI-018`。
//! トレーニングの終了で日の評価と記録のある種目ごとの提案の行ができ、
//! 記録の無い種目には提案ができないこと、Cron が `pending` を Queue へ送り、
//! ローカルの AI の失敗の経路で状態が `failed` に収束することを確かめる。
//! 状態の取得は所有者だけに許し、別のアカウントには 404 を返す。
//! 実環境の AI に接続しないため、`AI_MODEL` を存在しないモデル名に差し替える。
//! 固定の AI 出力の型の確認は、画面の確認に使うフィクスチャの退行を防ぐための
//! 例示テストであり、要求 ID を持たない。

mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use support::{DevServer, HttpResponse};
use warrior_core::ai::output::AiOutput;
use warrior_core::auth::{hash_session_token, hash_terminal_token};
use warrior_core::base64url;

const ORIGIN: &str = "http://localhost:8787";
const PORT: u16 = 8874;
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5501";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5502";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5510";
const TERMINAL_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5520";
const BENCH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5530";
const PLANK_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5531";
const SQUAT_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5532";
const BENCH_TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5540";
const PLANK_TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5541";
const SQUAT_TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5542";
const BENCH_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5550";
const PLANK_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5551";
const BENCH_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5560";
const PLANK_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5561";
const SESSION_SEED: u8 = 61;
const OTHER_SESSION_SEED: u8 = 62;
const TERMINAL_SEED: u8 = 63;
const DATE: &str = "2026-11-12";
const STARTED_AT: &str = "2026-11-12T03:00:00Z";
const NOW: &str = "2026-11-12T03:00:00Z";
const SENSITIVE_MEMO: &str = "SENSITIVE_MEMO_VALUE";

#[test]
fn finishing_a_training_creates_the_ai_jobs_and_the_local_ai_failure_is_recorded() {
    let server = DevServer::start_with_vars(PORT, &[("AI_MODEL", "invalid-model-for-test")]);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    seed(&server);
    let cookie = session_cookie(SESSION_SEED, TERMINAL_SEED);

    let finished = finish(&server, &cookie);
    assert_eq!(finished.status, 200, "{}", finished.body);

    let jobs = rows(
        &server,
        "SELECT id, kind, status, attempts, training_id, exercise_id, unique_key \
         FROM ai_generation ORDER BY kind, exercise_id",
    );
    assert_eq!(jobs.len(), 3, "one daily evaluation and two proposals");
    assert_eq!(jobs[0]["kind"], "daily_evaluation");
    assert_eq!(jobs[0]["status"], "pending");
    assert_eq!(jobs[0]["attempts"], 0);
    assert_eq!(jobs[0]["training_id"], TRAINING_ID);
    assert_eq!(jobs[0]["exercise_id"], Value::Null);
    assert_eq!(jobs[0]["unique_key"], "daily:2026-11-12");
    assert_eq!(jobs[1]["kind"], "set_proposal");
    assert_eq!(jobs[1]["exercise_id"], BENCH_EXERCISE_ID);
    assert_eq!(
        jobs[1]["unique_key"],
        "set:3f2504e0-4f89-41d3-9a0c-0305e82c5530"
    );
    assert_eq!(jobs[2]["kind"], "set_proposal");
    assert_eq!(jobs[2]["exercise_id"], PLANK_EXERCISE_ID);
    for job in &jobs[1..] {
        assert_eq!(job["status"], "pending");
        assert_eq!(job["attempts"], 0);
        assert_eq!(job["training_id"], TRAINING_ID);
    }
    assert!(
        !jobs
            .iter()
            .any(|job| job["exercise_id"] == SQUAT_EXERCISE_ID),
        "an exercise without a completed set must not have a proposal"
    );

    let daily_id = jobs[0]["id"].as_str().expect("the daily job id").to_owned();
    let status = get(&server, &cookie, &format!("/api/ai/jobs/{daily_id}"));
    assert_eq!(status.status, 200, "{}", status.body);
    assert_eq!(
        serde_json::from_str::<Value>(&status.body).expect("the status response"),
        json!({
            "id": daily_id,
            "kind": "daily_evaluation",
            "status": "pending",
            "attempts": 0
        })
    );

    let other_cookie = session_cookie(OTHER_SESSION_SEED, TERMINAL_SEED);
    assert_eq!(
        get(&server, &other_cookie, &format!("/api/ai/jobs/{daily_id}")).status,
        404,
        "another account must not read the job"
    );
    assert_eq!(
        get(
            &server,
            &cookie,
            "/api/ai/jobs/3f2504e0-4f89-41d3-9a0c-0305e82c5599"
        )
        .status,
        404
    );
    assert_eq!(
        server
            .request("GET", &format!("/api/ai/jobs/{daily_id}"), &[])
            .status,
        401
    );

    let scheduled = server.request("GET", "/cdn-cgi/local/scheduled", &[]);
    assert_eq!(scheduled.status, 200);

    let terminal = wait_for_terminal(&server, &cookie, &daily_id);
    assert_eq!(terminal, "failed", "the local AI binding must fail");
    let final_status = get(&server, &cookie, &format!("/api/ai/jobs/{daily_id}"));
    let final_status: Value =
        serde_json::from_str(&final_status.body).expect("the final status is JSON");
    assert_eq!(final_status["status"], "failed");
    let attempts = final_status["attempts"]
        .as_u64()
        .expect("the attempt count is a number");
    assert!(
        (1..=4).contains(&attempts),
        "the attempt count must be within the limit but was {attempts}"
    );

    let lines = server.wait_for_log("\"event\":\"ai_job\"");
    assert!(
        lines
            .iter()
            .any(|line| line.contains("\"result\":\"failed\"")
                || line.contains("\"result\":\"retry\"")),
        "the job log must include the result: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains(SENSITIVE_MEMO)),
        "the training memo must not be logged"
    );
    assert!(
        !lines.iter().any(|line| line.contains("ベンチプレス")),
        "the AI input must not be logged"
    );

    let key = format!("users/{USER_ID}/ai/{daily_id}/input.json");
    server
        .put_r2_object(&key, b"{\"context\":\"snapshot\"}")
        .expect("the snapshot must be stored");
    assert!(server
        .get_r2_object(&key)
        .expect("the snapshot must be readable")
        .is_some());
    let deleted = server.request_with_body(
        "DELETE",
        "/api/account",
        &[("Origin", ORIGIN), ("Cookie", cookie.as_str())],
        "{\"confirm\":\"delete-account\"}",
    );
    assert_eq!(deleted.status, 204, "{}", deleted.body);
    assert_eq!(
        server
            .get_r2_object(&key)
            .expect("the snapshot must be readable"),
        None,
        "account deletion must remove the AI snapshot"
    );
}

#[test]
fn the_fixed_ai_outputs_match_the_output_type() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ai_outputs");
    let daily_text = std::fs::read_to_string(directory.join("daily_evaluation.json"))
        .expect("the daily evaluation fixture must be readable");
    let daily: AiOutput = serde_json::from_str(&daily_text)
        .expect("the daily evaluation fixture must be an AiOutput");
    assert!(daily.evaluation.is_some());
    assert_eq!(daily.proposals.len(), 1);
    assert_eq!(daily.proposals[0].weight_kg, Some(82.5));

    let proposal_text = std::fs::read_to_string(directory.join("set_proposal.json"))
        .expect("the set proposal fixture must be readable");
    let proposals: AiOutput =
        serde_json::from_str(&proposal_text).expect("the set proposal fixture must be an AiOutput");
    assert_eq!(proposals.proposals.len(), 2);
    assert_eq!(proposals.proposals[1].duration_seconds, Some(75));
}

fn wait_for_terminal(server: &DevServer, cookie: &str, job_id: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut last = String::new();
    loop {
        let response = get(server, cookie, &format!("/api/ai/jobs/{job_id}"));
        if response.status == 200 {
            let value: Value =
                serde_json::from_str(&response.body).expect("the status response is JSON");
            last = value["status"]
                .as_str()
                .expect("the status is a string")
                .to_owned();
            if last == "generated" || last == "failed" {
                return last;
            }
        }
        if Instant::now() >= deadline {
            panic!("the AI job did not reach a terminal state; the last state was {last}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn session_cookie(session_seed: u8, terminal_seed: u8) -> String {
    let session_token = base64url::encode(&[session_seed; 32]);
    let terminal_token = base64url::encode(&[terminal_seed; 32]);
    format!("warrior_session={session_token}; warrior_terminal={terminal_token}")
}

fn finish(server: &DevServer, cookie: &str) -> HttpResponse {
    server.request_with_body(
        "POST",
        &format!("/api/trainings/{TRAINING_ID}/finish"),
        &[("Origin", ORIGIN), ("Cookie", cookie)],
        "",
    )
}

fn get(server: &DevServer, cookie: &str, path: &str) -> HttpResponse {
    server.request("GET", path, &[("Origin", ORIGIN), ("Cookie", cookie)])
}

fn rows(server: &DevServer, sql: &str) -> Vec<Value> {
    let result = server.execute_d1_json(sql).expect("the query must succeed");
    result
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
}

fn seed(server: &DevServer) {
    let session_token = base64url::encode(&[SESSION_SEED; 32]);
    let other_session_token = base64url::encode(&[OTHER_SESSION_SEED; 32]);
    let terminal_token = base64url::encode(&[TERMINAL_SEED; 32]);
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-ai-job', 'ai-job@example.test', 'AI の利用者', \
             '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-ai-job-other', 'ai-job-other@example.test', \
             '別の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5570', '{USER_ID}', '{}', '{NOW}', \
             '2099-01-01T00:00:00Z')",
            hash_session_token(&session_token)
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c5571', '{OTHER_USER_ID}', '{}', '{NOW}', \
             '2099-01-01T00:00:00Z')",
            hash_session_token(&other_session_token)
        ),
        format!(
            "INSERT INTO terminal_tokens (id, user_id, token_hash, created_at, last_seen_at) \
             VALUES ('{TERMINAL_ID}', '{USER_ID}', '{}', '{NOW}', '{NOW}')",
            hash_terminal_token(&terminal_token)
        ),
        format!(
            "INSERT INTO profiles (user_id, height_cm, weight_kg, birth_date, sex, \
             training_experience, goal, training_days_per_week, activity_level, created_at, \
             updated_at) \
             VALUES ('{USER_ID}', 175.0, 70.0, '1990-05-10', 'male', 'intermediate', \
             'hypertrophy', 4, 'normal', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, \
             updated_at) VALUES ('{TRAINING_ID}', '{USER_ID}', '{DATE}', '{STARTED_AT}', \
             '{SENSITIVE_MEMO}', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{BENCH_TRAINING_EXERCISE_ID}', \
             '{USER_ID}', '{TRAINING_ID}', '{BENCH_EXERCISE_ID}', '{}', 1, '{NOW}', '{NOW}')",
            bench_snapshot()
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{PLANK_TRAINING_EXERCISE_ID}', \
             '{USER_ID}', '{TRAINING_ID}', '{PLANK_EXERCISE_ID}', '{}', 2, '{NOW}', '{NOW}')",
            plank_snapshot()
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, \
             position, created_at, updated_at) VALUES ('{SQUAT_TRAINING_EXERCISE_ID}', \
             '{USER_ID}', '{TRAINING_ID}', '{SQUAT_EXERCISE_ID}', '{}', 3, '{NOW}', '{NOW}')",
            squat_snapshot()
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, rpe, \
             grip, created_at, updated_at) VALUES ('{BENCH_SET_ID}', '{USER_ID}', \
             '{BENCH_TRAINING_EXERCISE_ID}', 1, 'normal', 1, 8.0, NULL, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, \
             momentum_reps, assisted_reps, created_at, updated_at) VALUES ('{BENCH_SEGMENT_ID}', \
             '{USER_ID}', '{BENCH_SET_ID}', 1, 80.0, 8, 0, 0, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, rpe, \
             grip, created_at, updated_at) VALUES ('{PLANK_SET_ID}', '{USER_ID}', \
             '{PLANK_TRAINING_EXERCISE_ID}', 1, 'normal', 1, NULL, NULL, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, duration_seconds, \
             momentum_reps, assisted_reps, created_at, updated_at) VALUES ('{PLANK_SEGMENT_ID}', \
             '{USER_ID}', '{PLANK_SET_ID}', 1, 60, 0, 0, '{NOW}', '{NOW}')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the AI job test rows must be seeded");
    }
}

fn bench_snapshot() -> String {
    json!({
        "exercise_id": BENCH_EXERCISE_ID,
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [
            {"muscle": "pectoralis_major_middle", "role": "primary"},
            {"muscle": "triceps_brachii", "role": "secondary"}
        ],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string()
}

fn plank_snapshot() -> String {
    json!({
        "exercise_id": PLANK_EXERCISE_ID,
        "name": "プランク",
        "body_part": "abs",
        "muscles": [
            {"muscle": "rectus_abdominis_upper", "role": "primary"}
        ],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string()
}

fn squat_snapshot() -> String {
    json!({
        "exercise_id": SQUAT_EXERCISE_ID,
        "name": "スクワット",
        "body_part": "legs",
        "muscles": [
            {"muscle": "quadriceps", "role": "primary"}
        ],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string()
}
