//! スーパーセットの組の結合テスト。
//!
//! 対応する要求: `REQ-SET-005`。
//! 組が同じトレーニングの種目だけを受け付け、既に組に属する種目を含む組を拒否し、
//! 並べ替えで組の種目を変えられず、種目の削除で 2 つ未満になった組を解散することを
//! 確かめる。

use warrior_core::base64url;

use crate::helpers::*;
use crate::support::record_unilateral_superset::{
    exercise_delete_operation, exercise_reorder_operation, superset_create_operation,
    superset_delete_operation, superset_reorder_operation,
};
use crate::support::{record_common, DevServer};

const SUPERSET_PORT: u16 = 8839;
const CONCURRENT_PORT: u16 = 8843;

const SECOND_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5912";
const THIRD_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5913";
const FOURTH_ROW_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5916";
const SECOND_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5914";
const THIRD_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5915";
const FOURTH_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5917";

const TWO_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5970";
const TWO_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5971";
const REORDER_EXERCISE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5972";
const REORDER_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5973";
const DELETE_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5974";
const DELETE_SUPERSET_AGAIN_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5975";
const EMPTY_SUPERSET_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5976";
const EMPTY_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5977";
const ONE_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a1";
const ONE_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a2";
const DUPLICATE_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a3";
const DUPLICATE_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a4";
const SECOND_GROUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a5";
const SECOND_GROUP_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a6";
const CHANGING_REORDER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a7";
const DELETE_SECOND_GROUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a8";
const DUPLICATE_SECOND_GROUP_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59a9";
const DUPLICATE_SECOND_GROUP_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59aa";
const CONCURRENT_A_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ab";
const CONCURRENT_A_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ac";
const CONCURRENT_B_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ad";
const CONCURRENT_B_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c59ae";
const FOREIGN_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5978";
const FOREIGN_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5979";
const THREE_MEMBER_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5980";
const THREE_MEMBER_SUPERSET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5981";
const DELETE_SECOND_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5982";
const DELETE_THIRD_OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5983";

#[test]
fn supersets_keep_members_in_order_and_dissolve_when_one_remains() {
    let server = DevServer::start(SUPERSET_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[94u8; 32]);
    let terminal_token = base64url::encode(&[95u8; 32]);
    let other_terminal_token = base64url::encode(&[96u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    seed_superset_exercises(&server);

    create_and_reorder_the_two_member_group(&server, &session_cookie);
    reject_a_reorder_that_changes_the_group(&server, &session_cookie);
    reject_a_duplicate_group(&server, &session_cookie);
    delete_the_two_member_group(&server, &session_cookie);
    reject_the_empty_and_foreign_groups(&server, &session_cookie);
    dissolve_the_three_member_group(&server, &session_cookie);
}

#[test]
fn concurrent_group_creates_conflict_on_the_member_index() {
    let server = DevServer::start(CONCURRENT_PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let token = base64url::encode(&[100u8; 32]);
    let terminal_token = base64url::encode(&[101u8; 32]);
    let other_terminal_token = base64url::encode(&[102u8; 32]);
    seed_base(&server, &token, &terminal_token, &other_terminal_token);
    let session_cookie = cookie(&token, &terminal_token);
    seed_superset_exercises(&server);

    let first = superset_create_operation(
        CONCURRENT_A_OPERATION_ID,
        FINISHED_TRAINING_ID,
        CONCURRENT_A_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let second = superset_create_operation(
        CONCURRENT_B_OPERATION_ID,
        FINISHED_TRAINING_ID,
        CONCURRENT_B_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| post(&server, &session_cookie, &first));
        let second = scope.spawn(|| post(&server, &session_cookie, &second));
        (
            first.join().expect("the first create"),
            second.join().expect("the second create"),
        )
    });
    let mut statuses = vec![first.status, second.status];
    statuses.sort_unstable();
    assert_eq!(statuses, vec![200, 409], "{}", first.body);
    assert_eq!(
        count(&server, "supersets", CONCURRENT_A_SUPERSET_ID)
            + count(&server, "supersets", CONCURRENT_B_SUPERSET_ID),
        1
    );
}

fn seed_superset_exercises(server: &DevServer) {
    for (row_id, exercise_id, name, position) in [
        (SECOND_ROW_ID, SECOND_EXERCISE_ID, "ダンベルプレス", 2),
        (THIRD_ROW_ID, THIRD_EXERCISE_ID, "ダンベルフライ", 3),
        (FOURTH_ROW_ID, FOURTH_EXERCISE_ID, "ダンベルロウ", 4),
    ] {
        record_common::seed_exercise(
            server,
            record_common::ExerciseSeed {
                row_id,
                user_id: USER_ID,
                training_id: FINISHED_TRAINING_ID,
                exercise_id,
                name,
                body_part: "chest",
                muscle: "pectoralis_major_middle",
                record_mode: "weight_and_reps",
                position,
                grip_mode: "none",
                grip_options: &[],
            },
        );
    }
}

fn create_and_reorder_the_two_member_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        TWO_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID, SECOND_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, TWO_MEMBER_SUPERSET_ID),
        vec![FINISHED_ROW_ID.to_owned(), SECOND_ROW_ID.to_owned()]
    );

    let operation = exercise_reorder_operation(
        REORDER_EXERCISE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        &[SECOND_ROW_ID, FINISHED_ROW_ID, THIRD_ROW_ID, FOURTH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);

    let operation = superset_reorder_operation(
        REORDER_SUPERSET_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
        &[SECOND_ROW_ID, FINISHED_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, TWO_MEMBER_SUPERSET_ID),
        vec![SECOND_ROW_ID.to_owned(), FINISHED_ROW_ID.to_owned()]
    );
}

fn reject_a_reorder_that_changes_the_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        SECOND_GROUP_OPERATION_ID,
        FINISHED_TRAINING_ID,
        SECOND_GROUP_SUPERSET_ID,
        &[THIRD_ROW_ID, FOURTH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, SECOND_GROUP_SUPERSET_ID),
        vec![THIRD_ROW_ID.to_owned(), FOURTH_ROW_ID.to_owned()]
    );

    let operation = superset_reorder_operation(
        CHANGING_REORDER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(applied_count(server, CHANGING_REORDER_OPERATION_ID), 0);
    assert_eq!(
        member_ids(server, TWO_MEMBER_SUPERSET_ID),
        vec![SECOND_ROW_ID.to_owned(), FINISHED_ROW_ID.to_owned()]
    );
    assert_eq!(
        member_ids(server, SECOND_GROUP_SUPERSET_ID),
        vec![THIRD_ROW_ID.to_owned(), FOURTH_ROW_ID.to_owned()]
    );

    let operation = superset_create_operation(
        DUPLICATE_SECOND_GROUP_OPERATION_ID,
        FINISHED_TRAINING_ID,
        DUPLICATE_SECOND_GROUP_SUPERSET_ID,
        &[THIRD_ROW_ID, FOURTH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(
        count(server, "supersets", DUPLICATE_SECOND_GROUP_SUPERSET_ID),
        0
    );

    let operation = superset_delete_operation(
        DELETE_SECOND_GROUP_OPERATION_ID,
        FINISHED_TRAINING_ID,
        SECOND_GROUP_SUPERSET_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "supersets", SECOND_GROUP_SUPERSET_ID), 0);
}

fn delete_the_two_member_group(server: &DevServer, cookie: &str) {
    let operation = superset_delete_operation(
        DELETE_SUPERSET_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "supersets", TWO_MEMBER_SUPERSET_ID), 0);
    assert!(member_ids(server, TWO_MEMBER_SUPERSET_ID).is_empty());

    let operation = superset_delete_operation(
        DELETE_SUPERSET_AGAIN_OPERATION_ID,
        FINISHED_TRAINING_ID,
        TWO_MEMBER_SUPERSET_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        applied_count(server, DELETE_SUPERSET_AGAIN_OPERATION_ID),
        1,
        "a delete after the group is gone is recorded to stop the resend"
    );
}

fn reject_the_empty_and_foreign_groups(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        EMPTY_SUPERSET_OPERATION_ID,
        FINISHED_TRAINING_ID,
        EMPTY_SUPERSET_ID,
        &[],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "supersets", EMPTY_SUPERSET_ID), 0);

    let operation = superset_create_operation(
        ONE_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        ONE_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "supersets", ONE_MEMBER_SUPERSET_ID), 0);

    let operation = superset_create_operation(
        FOREIGN_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        FOREIGN_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID, BENCH_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 400, "{}", response.body);
    assert_eq!(count(server, "supersets", FOREIGN_MEMBER_SUPERSET_ID), 0);
}

fn reject_a_duplicate_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        DUPLICATE_OPERATION_ID,
        FINISHED_TRAINING_ID,
        DUPLICATE_SUPERSET_ID,
        &[SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 409, "{}", response.body);
    assert_eq!(count(server, "supersets", DUPLICATE_SUPERSET_ID), 0);
    assert_eq!(applied_count(server, DUPLICATE_OPERATION_ID), 0);
}

fn dissolve_the_three_member_group(server: &DevServer, cookie: &str) {
    let operation = superset_create_operation(
        THREE_MEMBER_OPERATION_ID,
        FINISHED_TRAINING_ID,
        THREE_MEMBER_SUPERSET_ID,
        &[FINISHED_ROW_ID, SECOND_ROW_ID, THIRD_ROW_ID],
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, THREE_MEMBER_SUPERSET_ID),
        vec![
            FINISHED_ROW_ID.to_owned(),
            SECOND_ROW_ID.to_owned(),
            THIRD_ROW_ID.to_owned()
        ]
    );

    let operation = exercise_delete_operation(
        DELETE_SECOND_OPERATION_ID,
        FINISHED_TRAINING_ID,
        SECOND_ROW_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(
        member_ids(server, THREE_MEMBER_SUPERSET_ID),
        vec![FINISHED_ROW_ID.to_owned(), THIRD_ROW_ID.to_owned()]
    );

    let operation = exercise_delete_operation(
        DELETE_THIRD_OPERATION_ID,
        FINISHED_TRAINING_ID,
        THIRD_ROW_ID,
    );
    let response = post(server, cookie, &operation);
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(count(server, "supersets", THREE_MEMBER_SUPERSET_ID), 0);
    assert!(member_ids(server, THREE_MEMBER_SUPERSET_ID).is_empty());
    assert_eq!(
        count_with(
            server,
            &format!(
                "SELECT COUNT(*) AS count FROM superset_members \
                 WHERE superset_id = '{THREE_MEMBER_SUPERSET_ID}'"
            )
        ),
        0
    );
}

fn member_ids(server: &DevServer, superset_id: &str) -> Vec<String> {
    let value = server
        .execute_d1_json(&format!(
            "SELECT training_exercise_id FROM superset_members \
             WHERE superset_id = '{superset_id}' ORDER BY position"
        ))
        .expect("the members must be readable");
    value
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(|rows| rows.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    row.get("training_exercise_id")
                        .and_then(|id| id.as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}
