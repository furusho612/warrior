//! 種目の追加、削除、並べ替えと引き継ぎの競合の結合テスト。
//!
//! 対応する要求: `REQ-NFR-015`。
//! 編集権の判定と種目のデータ文の間に引き継ぎが入っても、拒否された操作が
//! 一部の行だけを残さないことを確かめる。
mod support;
use support::ownership::*;

#[test]
fn a_takeover_and_an_exercise_operation_never_leave_a_partial_write() {
    let server = start_server_at(8819);
    let (session_token, _, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    for kind in 0..3u32 {
        for round in 0..6u32 {
            let training_id = exercise_uuid("a100", kind * 10 + round);
            let create_id = exercise_uuid("a200", kind * 10 + round);
            let operation_id = exercise_uuid("a300", kind * 10 + round);
            let first_row_id = exercise_uuid("a400", kind * 10 + round);
            let second_row_id = exercise_uuid("a500", kind * 10 + round);
            let set_id = exercise_uuid("a600", kind * 10 + round);
            let segment_id = exercise_uuid("a700", kind * 10 + round);
            let created = post_sync(
                &server,
                &cookie_a,
                &create_operation_for(&training_id, &create_id, ""),
            );
            assert_eq!(created.status, 200, "{}", created.body);

            if kind >= 1 {
                seed_exercise_row(&server, &training_id, &first_row_id, 1);
                seed_exercise_set(&server, &first_row_id, &set_id, &segment_id);
                if kind == 2 {
                    seed_exercise_row(&server, &training_id, &second_row_id, 2);
                }
            }

            let operation = match kind {
                0 => exercise_create_operation(&training_id, &operation_id, &first_row_id, 1),
                1 => exercise_delete_operation(&training_id, &operation_id, &first_row_id),
                _ => exercise_reorder_operation(
                    &training_id,
                    &operation_id,
                    &[second_row_id.clone(), first_row_id.clone()],
                ),
            };
            let (response, takeover) = std::thread::scope(|scope| {
                let operation = scope.spawn(|| post_sync(&server, &cookie_a, &operation));
                let takeover = scope.spawn(|| {
                    post(
                        &server,
                        &cookie_b,
                        &format!("/api/trainings/{training_id}/takeover"),
                    )
                });
                (
                    operation
                        .join()
                        .expect("the exercise operation must finish"),
                    takeover.join().expect("the takeover must finish"),
                )
            });
            assert_eq!(takeover.status, 200, "{}", takeover.body);
            match response.status {
                200 => {
                    assert_eq!(status_of(&response), "applied");
                    assert_eq!(applied_count(&server, &operation_id), 1);
                    match kind {
                        0 => {
                            assert_eq!(row_count(&server, "training_exercises", &first_row_id), 1)
                        }
                        1 => {
                            assert_eq!(row_count(&server, "training_exercises", &first_row_id), 0);
                            assert_eq!(row_count(&server, "sets", &set_id), 0);
                            assert_eq!(row_count(&server, "set_segments", &segment_id), 0);
                        }
                        _ => {
                            assert_eq!(position(&server, &second_row_id), 1);
                            assert_eq!(position(&server, &first_row_id), 2);
                        }
                    }
                }
                403 => {
                    assert_eq!(applied_count(&server, &operation_id), 0);
                    match kind {
                        0 => {
                            assert_eq!(row_count(&server, "training_exercises", &first_row_id), 0)
                        }
                        1 => {
                            assert_eq!(row_count(&server, "training_exercises", &first_row_id), 1);
                            assert_eq!(row_count(&server, "sets", &set_id), 1);
                            assert_eq!(row_count(&server, "set_segments", &segment_id), 1);
                        }
                        _ => {
                            assert_eq!(position(&server, &first_row_id), 1);
                            assert_eq!(position(&server, &second_row_id), 2);
                        }
                    }
                }
                other => {
                    panic!("the exercise operation must be applied or rejected: {other}")
                }
            }
        }
    }
}
