//! 身体情報と目的の設定の結合テスト。
//!
//! 対応する要求: `REQ-PRF-001`、`REQ-PRF-002`、`REQ-PRF-003`、`REQ-PRF-004`、
//! `REQ-PRF-005`、`REQ-LIB-004`、`REQ-NFR-018`、`REQ-NFR-019`。
//! 未登録は 200 の `null`、更新は最新値の置き換え、入力の範囲の拒否、目的と日数の変更、
//! 自重種目のセットが記録時の体重を保つことを確かめる。
//! 種目とセットは D1 に置き、記録画面は使わない。

mod support;

use serde_json::{json, Value};
use support::{DevServer, HttpResponse};
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::calendar::CalendarDayDetail;
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{Goal, Sex};
use warrior_core::profile::{Profile, TrainingExperience};
use warrior_core::training_record::TrainingRecord;

const ORIGIN: &str = "http://localhost:8787";
const PROFILE_PORT: u16 = 8819;
const BODYWEIGHT_PORT: u16 = 8822;
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a01";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a02";
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a10";
const TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a20";
const BODYWEIGHT_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a21";
const RECORDED_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a30";
const MISSING_WEIGHT_SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a31";
const RECORDED_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a40";
const MISSING_WEIGHT_SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4a41";
const DATE: &str = "2026-11-12";
const NOW: &str = "2026-11-12T00:00:00Z";

#[test]
fn the_profile_round_trips_and_replaces_the_latest_value() {
    let server = DevServer::start(PROFILE_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[31u8; 32]);
    seed_user(&server, &token);
    let cookie = format!("warrior_session={token}");

    assert_eq!(
        server.request("GET", "/api/profile", &[]).status,
        401,
        "the profile requires authentication"
    );
    assert_eq!(get_profile(&server, &cookie).body, "null");

    let without_origin = server.request_with_body(
        "PUT",
        "/api/profile",
        &[("Cookie", cookie.as_str())],
        &full_profile().to_string(),
    );
    assert_eq!(without_origin.status, 403);

    let unauthenticated = server.request_with_body(
        "PUT",
        "/api/profile",
        &[("Origin", ORIGIN)],
        &full_profile().to_string(),
    );
    assert_eq!(unauthenticated.status, 401);
    assert_eq!(put_profile(&server, &cookie, &json!({})).status, 400);

    let created = put_profile(&server, &cookie, &full_profile());
    assert_eq!(created.status, 200, "{}", created.body);
    let created_profile: Profile =
        serde_json::from_str(&created.body).expect("the profile response");
    assert_eq!(
        created_profile,
        serde_json::from_value(full_profile()).expect("the input profile")
    );
    assert_eq!(profile_count(&server, USER_ID), 1);
    assert_eq!(
        serde_json::from_str::<Profile>(&get_profile(&server, &cookie).body)
            .expect("the stored profile"),
        created_profile
    );

    let updated = json!({
        "height_cm": 180.0,
        "weight_kg": 75.0,
        "birth_date": "1995-02-03",
        "sex": "female",
        "training_experience": "advanced",
        "goal": "diet",
        "training_days_per_week": 7,
        "activity_level": "high"
    });
    let updated_response = put_profile(&server, &cookie, &updated);
    assert_eq!(updated_response.status, 200, "{}", updated_response.body);
    assert_eq!(profile_count(&server, USER_ID), 1);
    let stored: Profile =
        serde_json::from_str(&get_profile(&server, &cookie).body).expect("the updated profile");
    assert_eq!(
        stored,
        serde_json::from_value(updated.clone()).expect("the updated input profile")
    );
    assert_eq!(stored.birth_date, Some(JstDate::new(1995, 2, 3).unwrap()));
    assert_eq!(stored.sex, Some(Sex::Female));
    assert_eq!(
        stored.training_experience,
        Some(TrainingExperience::Advanced)
    );

    reject_out_of_range_values(&server, &cookie, &stored);
    accept_the_boundaries(&server, &cookie);
    accept_each_choice(&server, &cookie);
    accept_a_partial_profile(&server, &cookie);
    let other_token = base64url::encode(&[33u8; 32]);
    let other_cookie = format!("warrior_session={other_token}");
    assert_eq!(get_profile(&server, &other_cookie).body, "null");
    assert_eq!(profile_count(&server, OTHER_USER_ID), 0);
    assert_eq!(profile_count(&server, USER_ID), 1);
}

fn reject_out_of_range_values(server: &DevServer, cookie: &str, stored: &Profile) {
    for (field, value) in [
        ("height_cm", json!(99.9)),
        ("height_cm", json!(250.1)),
        ("weight_kg", json!(19.9)),
        ("weight_kg", json!(300.1)),
        ("training_days_per_week", json!(8)),
        ("birth_date", json!("2099-01-01")),
    ] {
        let mut input = serde_json::to_value(stored).expect("serializable profile");
        input[field] = value;
        let response = put_profile(server, cookie, &input);
        assert_eq!(
            response.status, 400,
            "the value {field} must be rejected: {}",
            response.body
        );
        assert_eq!(
            serde_json::from_str::<Profile>(&get_profile(server, cookie).body)
                .expect("the profile must stay"),
            *stored,
            "a rejected update must not change the stored profile"
        );
    }

    let mut input = serde_json::to_value(stored).expect("serializable profile");
    input["height_cm"] = json!(100.0);
    input["weight_kg"] = json!(20.0);
    input["birth_date"] = json!("1000-01-01");
    let response = put_profile(server, cookie, &input);
    assert_eq!(
        response.status, 400,
        "a non-positive basal metabolism must be rejected: {}",
        response.body
    );

    let invalid_choice = json!({
        "height_cm": 175.0,
        "weight_kg": 70.0,
        "birth_date": "1990-05-10",
        "sex": "unknown",
        "training_experience": "intermediate",
        "goal": "hypertrophy",
        "training_days_per_week": 4,
        "activity_level": "normal"
    });
    assert_eq!(put_profile(server, cookie, &invalid_choice).status, 400);

    let mut unknown = serde_json::to_value(stored).expect("serializable profile");
    unknown["unknown"] = json!(1);
    assert_eq!(put_profile(server, cookie, &unknown).status, 400);
}

fn accept_the_boundaries(server: &DevServer, cookie: &str) {
    for (height, weight, days) in [(100.0, 20.0, 0), (250.0, 300.0, 7)] {
        let input = profile_json(ProfileValues {
            height_cm: height,
            weight_kg: weight,
            sex: "other",
            experience: "beginner",
            goal: "body_make",
            days,
            activity: "low",
            ..default_profile_values()
        });
        let response = put_profile(server, cookie, &input);
        assert_eq!(response.status, 200, "{}", response.body);
        let stored: Profile =
            serde_json::from_str(&get_profile(server, cookie).body).expect("the profile");
        assert_eq!(stored.height_cm, Some(height));
        assert_eq!(stored.weight_kg, Some(weight));
        assert_eq!(stored.training_days_per_week, Some(days));
    }
}

fn accept_each_choice(server: &DevServer, cookie: &str) {
    for (sex, sex_value) in [
        (Sex::Male, "male"),
        (Sex::Female, "female"),
        (Sex::Other, "other"),
    ] {
        let input = profile_json(ProfileValues {
            sex: sex_value,
            ..default_profile_values()
        });
        assert_eq!(put_profile(server, cookie, &input).status, 200);
        let stored: Profile =
            serde_json::from_str(&get_profile(server, cookie).body).expect("the profile");
        assert_eq!(stored.sex, Some(sex));
    }

    for (experience, experience_value) in [
        (TrainingExperience::Beginner, "beginner"),
        (TrainingExperience::Intermediate, "intermediate"),
        (TrainingExperience::Advanced, "advanced"),
    ] {
        let input = profile_json(ProfileValues {
            experience: experience_value,
            ..default_profile_values()
        });
        assert_eq!(put_profile(server, cookie, &input).status, 200);
        let stored: Profile =
            serde_json::from_str(&get_profile(server, cookie).body).expect("the profile");
        assert_eq!(stored.training_experience, Some(experience));
    }

    for (goal, goal_value) in [
        (Goal::Hypertrophy, "hypertrophy"),
        (Goal::Diet, "diet"),
        (Goal::BodyMake, "body_make"),
        (Goal::Strength, "strength"),
        (Goal::Performance, "performance"),
    ] {
        let input = profile_json(ProfileValues {
            goal: goal_value,
            ..default_profile_values()
        });
        assert_eq!(put_profile(server, cookie, &input).status, 200);
        let stored: Profile =
            serde_json::from_str(&get_profile(server, cookie).body).expect("the profile");
        assert_eq!(stored.goal, Some(goal));
    }
}

fn accept_a_partial_profile(server: &DevServer, cookie: &str) {
    let input = json!({
        "height_cm": 165.0,
        "weight_kg": 55.0,
        "birth_date": "2000-06-07",
        "sex": "female"
    });
    assert_eq!(put_profile(server, cookie, &input).status, 200);
    let stored: Profile =
        serde_json::from_str(&get_profile(server, cookie).body).expect("the profile");
    assert_eq!(stored.height_cm, Some(165.0));
    assert_eq!(stored.weight_kg, Some(55.0));
    assert_eq!(stored.training_experience, None);
    assert_eq!(stored.goal, None);
    assert_eq!(stored.training_days_per_week, None);
    assert_eq!(stored.activity_level, None);
}

#[test]
fn the_bodyweight_set_keeps_the_recorded_body_weight() {
    let server = DevServer::start(BODYWEIGHT_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[32u8; 32]);
    seed_user(&server, &token);
    let cookie = format!("warrior_session={token}");
    seed_bodyweight_training(&server);

    let day = get_day(&server, &cookie);
    assert_eq!(day.total_weight_kg, 400.0);
    let recorded_set = &day.trainings[0].exercises[0].sets[0];
    assert_eq!(recorded_set.total_weight_kg, 400.0);
    assert_eq!(recorded_set.segments[0].weight_kg, 80.0);
    let missing_weight_set = &day.trainings[0].exercises[0].sets[1];
    assert_eq!(missing_weight_set.total_weight_kg, 0.0);
    assert_eq!(missing_weight_set.segments[0].weight_kg, 0.0);
    let record = get_record(&server, &cookie);
    let recorded = record
        .sets
        .iter()
        .find(|set| set.id == RECORDED_SET_ID)
        .expect("the recorded set");
    assert_eq!(recorded.body_weight_kg, Some(70.0));
    let missing = record
        .sets
        .iter()
        .find(|set| set.id == MISSING_WEIGHT_SET_ID)
        .expect("the set without a body weight");
    assert_eq!(missing.body_weight_kg, None);

    let updated = profile_json(ProfileValues {
        height_cm: 180.0,
        weight_kg: 80.0,
        ..default_profile_values()
    });
    assert_eq!(put_profile(&server, &cookie, &updated).status, 200);

    let day_after_update = get_day(&server, &cookie);
    assert_eq!(
        day_after_update.total_weight_kg, 400.0,
        "updating the profile must not change the recorded set"
    );
    let recorded_set_after_update = &day_after_update.trainings[0].exercises[0].sets[0];
    assert_eq!(recorded_set_after_update.segments[0].weight_kg, 80.0);
    assert_eq!(
        stored_added_weight(&server, RECORDED_SEGMENT_ID),
        10.0,
        "the segment must keep the added weight, not the computed total"
    );
    let stored_body_weight = set_body_weight(&server, RECORDED_SET_ID);
    assert_eq!(stored_body_weight, Some(70.0));
    let record_after_update = get_record(&server, &cookie);
    let recorded_after_update = record_after_update
        .sets
        .iter()
        .find(|set| set.id == RECORDED_SET_ID)
        .expect("the recorded set after the update");
    assert_eq!(recorded_after_update.body_weight_kg, Some(70.0));
}

fn seed_user(server: &DevServer, token: &str) {
    for sql in [
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{USER_ID}', 'sub-profile', 'profile@example.test', '身体情報の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO users (id, google_sub, email, display_name, created_at, updated_at) \
             VALUES ('{OTHER_USER_ID}', 'sub-other-profile', 'other-profile@example.test', '別の利用者', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4a03', '{USER_ID}', '{}', '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(token)
        ),
        format!(
            "INSERT INTO auth_sessions (id, user_id, token_hash, created_at, expires_at) \
             VALUES ('3f2504e0-4f89-41d3-9a0c-0305e82c4a04', '{OTHER_USER_ID}', '{}', '{NOW}', '2099-01-01T00:00:00Z')",
            hash_session_token(&base64url::encode(&[33u8; 32]))
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the profile fixture must be seeded");
    }
}

fn seed_bodyweight_training(server: &DevServer) {
    let snapshot = json!({
        "exercise_id": BODYWEIGHT_EXERCISE_ID,
        "name": "プッシュアップ",
        "body_part": "chest",
        "muscles": [],
        "record_mode": "bodyweight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string()
    .replace('\'', "''");
    for sql in [
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{TRAINING_ID}', '{USER_ID}', '{DATE}', '{NOW}', '', '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{TRAINING_EXERCISE_ID}', '{USER_ID}', '{TRAINING_ID}', '{BODYWEIGHT_EXERCISE_ID}', '{snapshot}', 1, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, body_weight_kg, created_at, updated_at) \
             VALUES ('{RECORDED_SET_ID}', '{USER_ID}', '{TRAINING_EXERCISE_ID}', 1, 'normal', 1, 70.0, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{MISSING_WEIGHT_SET_ID}', '{USER_ID}', '{TRAINING_EXERCISE_ID}', 2, 'normal', 1, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, created_at, updated_at) \
             VALUES ('{RECORDED_SEGMENT_ID}', '{USER_ID}', '{RECORDED_SET_ID}', 1, 10.0, 5, '{NOW}', '{NOW}')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, created_at, updated_at) \
             VALUES ('{MISSING_WEIGHT_SEGMENT_ID}', '{USER_ID}', '{MISSING_WEIGHT_SET_ID}', 1, 20.0, 5, '{NOW}', '{NOW}')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the bodyweight fixture must be seeded");
    }
}

fn full_profile() -> Value {
    profile_json(default_profile_values())
}

struct ProfileValues<'a> {
    height_cm: f64,
    weight_kg: f64,
    birth_date: &'a str,
    sex: &'a str,
    experience: &'a str,
    goal: &'a str,
    days: u32,
    activity: &'a str,
}

fn default_profile_values() -> ProfileValues<'static> {
    ProfileValues {
        height_cm: 175.0,
        weight_kg: 70.0,
        birth_date: "1990-05-10",
        sex: "male",
        experience: "intermediate",
        goal: "hypertrophy",
        days: 4,
        activity: "normal",
    }
}

fn profile_json(values: ProfileValues<'_>) -> Value {
    json!({
        "height_cm": values.height_cm,
        "weight_kg": values.weight_kg,
        "birth_date": values.birth_date,
        "sex": values.sex,
        "training_experience": values.experience,
        "goal": values.goal,
        "training_days_per_week": values.days,
        "activity_level": values.activity
    })
}

fn get_profile(server: &DevServer, cookie: &str) -> HttpResponse {
    server.request("GET", "/api/profile", &[("Cookie", cookie)])
}

fn put_profile(server: &DevServer, cookie: &str, body: &Value) -> HttpResponse {
    server.request_with_body(
        "PUT",
        "/api/profile",
        &[("Cookie", cookie), ("Origin", ORIGIN)],
        &body.to_string(),
    )
}

fn profile_count(server: &DevServer, user_id: &str) -> i64 {
    count(
        server,
        &format!("SELECT COUNT(*) AS count FROM profiles WHERE user_id = '{user_id}'"),
    )
}

fn count(server: &DevServer, sql: &str) -> i64 {
    server
        .execute_d1_json(sql)
        .expect("the count must be readable")
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("count"))
        .and_then(Value::as_i64)
        .expect("a count")
}

fn get_day(server: &DevServer, cookie: &str) -> CalendarDayDetail {
    let response = server.request(
        "GET",
        &format!("/api/calendar/{DATE}"),
        &[("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the day response")
}

fn get_record(server: &DevServer, cookie: &str) -> TrainingRecord {
    let response = server.request(
        "GET",
        &format!("/api/trainings/{TRAINING_ID}"),
        &[("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the training record")
}

fn stored_added_weight(server: &DevServer, segment_id: &str) -> f64 {
    let value = server
        .execute_d1_json(&format!(
            "SELECT weight_kg FROM set_segments WHERE id = '{segment_id}'"
        ))
        .expect("the segment weight must be readable");
    value
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("weight_kg"))
        .and_then(Value::as_f64)
        .expect("the stored segment weight")
}

fn set_body_weight(server: &DevServer, set_id: &str) -> Option<f64> {
    let value = server
        .execute_d1_json(&format!(
            "SELECT body_weight_kg FROM sets WHERE id = '{set_id}'"
        ))
        .expect("the set body weight must be readable");
    value
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.get(0))
        .and_then(|row| row.get("body_weight_kg"))
        .and_then(Value::as_f64)
}
