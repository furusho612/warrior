//! 種目ライブラリの一覧、絞り込み、並び、ページング、詳細、お気に入りの結合テスト。
//!
//! 対応する要求: `REQ-LIB-002`、`REQ-LIB-006`、`REQ-LIB-007`、`REQ-LIB-008`。
//! 種目マスターとお気に入りをローカルの D1 に置き、4 つの絞り込み、複数の条件、
//! お気に入りの登録と解除、利用者ごとの分離、空の一覧、カーソルのページング、
//! 詳細の主働筋と補助筋を確かめる。

mod support;

use support::{DevServer, HttpResponse};
use warrior_core::auth::hash_session_token;
use warrior_core::base64url;
use warrior_core::exercise::{BodyPart, Equipment, MuscleRole, MuscleUnit, OtherScope};
use warrior_core::library::{kana_sort_key, ExerciseList, FavoriteState, LibraryExercise};

const PORT: u16 = 8860;
const ORIGIN: &str = "http://localhost:8787";
const PAGE_SIZE: &str = "5";
const USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b01";
const OTHER_USER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4b02";
const BENCH: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const TREADMILL: &str = "a7c4e2f0-0000-4000-8000-000000000063";

#[test]
fn the_library_filters_sorts_pages_and_keeps_favorites_per_user() {
    let server = DevServer::start_with_vars(PORT, &[("LIBRARY_PAGE_SIZE", PAGE_SIZE)]);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[41u8; 32]);
    let other_token = base64url::encode(&[42u8; 32]);
    seed_users(&server, &token, &other_token);
    let cookie = format!("warrior_session={token}");
    let other_cookie = format!("warrior_session={other_token}");

    let all = list(&server, &cookie, "");
    assert_eq!(all.exercises.len(), 106, "the master has 106 exercises");
    assert!(all.next_cursor.is_none(), "the unpaged list returns all");
    assert!(all.exercises.iter().all(|item| !item.favorite));
    assert_order(&all.exercises);

    check_body_part_filter(&server, &cookie);
    check_equipment_filter(&server, &cookie);
    check_query_filter(&server, &cookie);
    check_combined_filters(&server, &cookie);
    check_empty_filter(&server, &cookie);
    check_paging(&server, &cookie, &all);
    check_detail(&server, &cookie);
    check_favorites_and_isolation(&server, &cookie, &other_cookie);
    check_favorite_with_every_filter(&server, &cookie);
    check_rejections(&server, &cookie);
}

fn check_body_part_filter(server: &DevServer, cookie: &str) {
    let chest = list(server, cookie, "?paged=true&body_part=chest");
    assert_eq!(chest.exercises.len(), 5, "the page size is five");
    assert!(chest.next_cursor.is_some(), "the chest has a next page");
    assert!(chest
        .exercises
        .iter()
        .all(|item| item.exercise.body_part == BodyPart::Chest));
    assert_order(&chest.exercises);
    let all_chest = list(server, cookie, "?body_part=chest&paged=false");
    assert_eq!(all_chest.exercises.len(), 18);
    assert!(all_chest
        .exercises
        .iter()
        .all(|item| item.exercise.body_part == BodyPart::Chest));
}

fn check_equipment_filter(server: &DevServer, cookie: &str) {
    let dumbbell = list(server, cookie, "?paged=true&equipment=dumbbell");
    assert!(!dumbbell.exercises.is_empty());
    assert!(dumbbell
        .exercises
        .iter()
        .all(|item| item.exercise.equipment == Equipment::Dumbbell));
    assert_order(&dumbbell.exercises);
}

fn check_query_filter(server: &DevServer, cookie: &str) {
    let query = encode_query("でくらいん");
    let decline = list(server, cookie, &format!("?paged=true&q={query}"));
    assert_eq!(
        decline
            .exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "a7c4e2f0-0000-4000-8000-000000000002",
            "a7c4e2f0-0000-4000-8000-000000000008",
        ],
        "the search matches the kana in order"
    );
    assert_order(&decline.exercises);
    let by_name = list(
        server,
        cookie,
        &format!("?paged=true&q={}", encode_query("デクライン")),
    );
    assert_eq!(
        by_name
            .exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "a7c4e2f0-0000-4000-8000-000000000002",
            "a7c4e2f0-0000-4000-8000-000000000008",
        ],
        "the search matches the name"
    );
}

fn check_combined_filters(server: &DevServer, cookie: &str) {
    let query = encode_query("ベンチ");
    let combined = list(
        server,
        cookie,
        &format!("?paged=true&body_part=chest&equipment=barbell&q={query}"),
    );
    assert_eq!(
        combined
            .exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "a7c4e2f0-0000-4000-8000-000000000001",
            "a7c4e2f0-0000-4000-8000-000000000002",
            BENCH,
        ],
        "every condition must hold"
    );
}

fn check_empty_filter(server: &DevServer, cookie: &str) {
    let query = encode_query("存在しない種目");
    let empty = list(server, cookie, &format!("?paged=true&q={query}"));
    assert!(empty.exercises.is_empty());
    assert!(empty.next_cursor.is_none());
}

fn check_paging(server: &DevServer, cookie: &str, all: &ExerciseList) {
    let mut collected = Vec::new();
    let mut cursor = None;
    loop {
        let path = match cursor.as_deref() {
            Some(cursor) => format!("?paged=true&cursor={}", encode_query(cursor)),
            None => "?paged=true".to_owned(),
        };
        let page = list(server, cookie, &path);
        assert!(!page.exercises.is_empty(), "a page must not be empty");
        if page.next_cursor.is_some() {
            assert_eq!(page.exercises.len(), 5, "the page size is five");
        }
        assert_order(&page.exercises);
        collected.extend(page.exercises);
        let Some(next) = page.next_cursor else {
            break;
        };
        cursor = Some(next);
    }
    assert_eq!(
        collected
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        all.exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        "the pages cover every exercise once in order"
    );
}

fn check_detail(server: &DevServer, cookie: &str) {
    server
        .execute_d1_json(&format!(
            "UPDATE master_exercises SET instructions = '肩甲骨を寄せて押す', \
             cautions = '肩をすくめない', tips = '足で床を押す' WHERE id = '{BENCH}'"
        ))
        .expect("the detail text must be stored");
    let detail = get_detail(server, cookie, BENCH);
    assert_eq!(detail.exercise.name, "ベンチプレス");
    assert_eq!(detail.exercise.body_part, BodyPart::Chest);
    assert_eq!(detail.exercise.equipment, Equipment::Barbell);
    assert_eq!(detail.exercise.instructions, "肩甲骨を寄せて押す");
    assert_eq!(detail.exercise.cautions, "肩をすくめない");
    assert_eq!(detail.exercise.tips, "足で床を押す");
    assert_eq!(
        muscles(&detail, MuscleRole::Primary),
        vec![MuscleUnit::PectoralisMajorMiddle]
    );
    let mut secondary = muscles(&detail, MuscleRole::Secondary);
    secondary.sort_unstable();
    assert_eq!(
        secondary,
        vec![MuscleUnit::AnteriorDeltoid, MuscleUnit::TricepsBrachii]
    );

    let other = get_detail(server, cookie, TREADMILL);
    assert_eq!(other.exercise.body_part, BodyPart::Other);
    assert_eq!(other.exercise.other_scope, Some(OtherScope::FullBody));
    assert!(other.exercise.muscles.is_empty());
}

fn check_favorites_and_isolation(server: &DevServer, cookie: &str, other_cookie: &str) {
    let empty = list(server, cookie, "?paged=true&favorite=true");
    assert!(empty.exercises.is_empty());

    let added = set_favorite(server, cookie, BENCH, true);
    assert_eq!(added.status, 200, "{}", added.body);
    assert_eq!(favorite_state(&added), FavoriteState { favorite: true });
    let only = list(server, cookie, "?paged=true&favorite=true");
    assert_eq!(
        only.exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        vec![BENCH]
    );
    assert!(get_detail(server, cookie, BENCH).favorite);
    assert!(!get_detail(server, other_cookie, BENCH).favorite);

    let other_empty = list(server, other_cookie, "?paged=true&favorite=true");
    assert!(other_empty.exercises.is_empty());
    let other_added = set_favorite(server, other_cookie, BENCH, true);
    assert_eq!(other_added.status, 200, "{}", other_added.body);

    let removed = set_favorite(server, cookie, BENCH, false);
    assert_eq!(removed.status, 200, "{}", removed.body);
    assert_eq!(favorite_state(&removed), FavoriteState { favorite: false });
    assert!(list(server, cookie, "?paged=true&favorite=true")
        .exercises
        .is_empty());
    assert_eq!(
        list(server, other_cookie, "?paged=true&favorite=true")
            .exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        vec![BENCH],
        "another user's favorite must stay"
    );

    set_favorite(server, other_cookie, BENCH, false);
}

fn check_favorite_with_every_filter(server: &DevServer, cookie: &str) {
    let query = encode_query("ベンチ");
    set_favorite(server, cookie, BENCH, true);
    let filtered = list(
        server,
        cookie,
        &format!("?paged=true&body_part=chest&equipment=barbell&q={query}&favorite=true"),
    );
    assert_eq!(
        filtered
            .exercises
            .iter()
            .map(|item| item.exercise.id.as_str())
            .collect::<Vec<_>>(),
        vec![BENCH]
    );
    set_favorite(server, cookie, BENCH, false);
    let removed = list(
        server,
        cookie,
        &format!("?paged=true&body_part=chest&equipment=barbell&q={query}&favorite=true"),
    );
    assert!(removed.exercises.is_empty());
}

fn check_rejections(server: &DevServer, cookie: &str) {
    for path in [
        "/api/exercises?body_part=arms",
        "/api/exercises?equipment=machine&equipment=",
        "/api/exercises?favorite=yes",
        "/api/exercises?paged=yes",
        "/api/exercises?paged=true&cursor=not-a-cursor",
    ] {
        let rejected = server.request("GET", path, &[("Cookie", cookie)]);
        assert_eq!(rejected.status, 400, "{path}: {}", rejected.body);
        assert!(rejected.body.contains("\"code\":\"bad_request\""));
    }
    let without_origin = server.request(
        "PUT",
        &format!("/api/exercises/{BENCH}/favorite"),
        &[("Cookie", cookie)],
    );
    assert_eq!(without_origin.status, 403);

    let unknown = server.request(
        "PUT",
        "/api/exercises/3f2504e0-4f89-41d3-9a0c-0305e82c4b99/favorite",
        &[("Cookie", cookie), ("Origin", ORIGIN)],
    );
    assert_eq!(unknown.status, 404);

    let unauthorized = server.request("GET", "/api/exercises?paged=true", &[]);
    assert_eq!(unauthorized.status, 401);
    let unauthorized_detail = server.request("GET", &format!("/api/exercises/{BENCH}"), &[]);
    assert_eq!(unauthorized_detail.status, 401);
}

fn seed_users(server: &DevServer, token: &str, other_token: &str) {
    let now = "2026-11-12T00:00:00Z";
    for (user_id, sub, email, name) in [
        (
            USER_ID,
            "sub-library",
            "library@example.test",
            "ライブラリの利用者",
        ),
        (
            OTHER_USER_ID,
            "sub-other-library",
            "other-library@example.test",
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
        ("3f2504e0-4f89-41d3-9a0c-0305e82c4b10", USER_ID, token),
        (
            "3f2504e0-4f89-41d3-9a0c-0305e82c4b11",
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

fn list(server: &DevServer, cookie: &str, query: &str) -> ExerciseList {
    let response = server.request(
        "GET",
        &format!("/api/exercises{query}"),
        &[("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the exercise list response")
}

fn get_detail(server: &DevServer, cookie: &str, exercise_id: &str) -> LibraryExercise {
    let response = server.request(
        "GET",
        &format!("/api/exercises/{exercise_id}"),
        &[("Cookie", cookie)],
    );
    assert_eq!(response.status, 200, "{}", response.body);
    serde_json::from_str(&response.body).expect("the exercise detail response")
}

fn set_favorite(
    server: &DevServer,
    cookie: &str,
    exercise_id: &str,
    favorite: bool,
) -> HttpResponse {
    let method = if favorite { "PUT" } else { "DELETE" };
    server.request(
        method,
        &format!("/api/exercises/{exercise_id}/favorite"),
        &[("Cookie", cookie), ("Origin", ORIGIN)],
    )
}

fn favorite_state(response: &HttpResponse) -> FavoriteState {
    serde_json::from_str(&response.body).expect("the favorite response")
}

fn muscles(item: &LibraryExercise, role: MuscleRole) -> Vec<MuscleUnit> {
    item.exercise
        .muscles
        .iter()
        .filter(|assignment| assignment.role == role)
        .map(|assignment| assignment.muscle)
        .collect()
}

fn assert_order(items: &[LibraryExercise]) {
    for pair in items.windows(2) {
        let left = (
            pair[0].exercise.body_part.order(),
            pair[0].exercise.equipment.order(),
            kana_sort_key(&pair[0].exercise.name_kana),
            pair[0].exercise.id.as_str(),
        );
        let right = (
            pair[1].exercise.body_part.order(),
            pair[1].exercise.equipment.order(),
            kana_sort_key(&pair[1].exercise.name_kana),
            pair[1].exercise.id.as_str(),
        );
        assert!(left <= right, "the order must follow the library rules");
    }
}

fn encode_query(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
