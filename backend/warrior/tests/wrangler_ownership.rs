//! 記録中のトレーニングの編集権と引き継ぎの結合テスト。
//!
//! 対応する要求: `REQ-NFR-015`、`REQ-NFR-006`。
//! 同じ利用者の 2 つの端末トークンを使い、記録中は所有者だけが書き込めること、
//! 引き継ぎで所有者が移ること、終了の後はどの端末からでも書けて後勝ちになること、
//! 保存済みの記録をほかの端末が読めること、別の利用者の記録が 404 になることを確かめる。
//! 判定と書き込みの間に所有者が移っても、0 行の書き込みを成功として扱わないことを
//! 引き継ぎとの競合で確かめる。

mod support;

use serde_json::{json, Value};
use support::ownership::*;
use warrior_core::training_session::TrainingSessionSummary;


#[test]
fn only_the_owner_edits_a_recording_training_and_the_handover_moves_the_right() {
    let server = start_server();
    let (session_token, other_session_token, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    create_as_a(&server, &cookie_a);
    check_stored_hashes(&server, &token_a, &token_b);
    seed_sets(&server);

    rejects_the_other_terminal(&server, &cookie_b, "3f2504e0-4f89-41d3-9a0c-0305e82c4640");
    rejects_a_create_from_the_other_terminal(&server, &cookie_b);
    check_record(&server, &cookie_b);
    takeover_as_b(&server, &cookie_b);
    rejects_the_original_terminal(&server, &cookie_a, "3f2504e0-4f89-41d3-9a0c-0305e82c4641");
    reject_the_finish_of_the_other_terminal(&server, &cookie_a);
    finish_as_b(&server, &cookie_b);
    reject_the_takeover_of_the_finished_training(&server, &cookie_b);
    check_the_rejected_takeover_did_not_change_d1(&server);

    edit_after_the_finish(&server, &cookie_a, &cookie_b);
    claim_a_recording_session_without_an_owner(&server, &cookie_a, &cookie_b);
    reject_other_users(&server, &cookie_a);
    reject_a_missing_training(&server, &cookie_a);
    reject_a_reused_terminal_of_a_switched_account(&server, &other_session_token, &token_a);
}

/// 記録中の更新と引き継ぎが重なっても、受理した応答と D1 の内容が食い違わない。
///
/// 判定と batch の間で所有者が移ると、更新の SQL は 0 行になる。このとき操作 ID を
/// 残すと、同じ操作の再送が適用済みに見える。200 の更新はメモと操作 ID を残し、
/// 403 の更新はどちらも残さないことを、どの実行順序でも確かめる。
#[test]
fn a_takeover_and_a_recording_update_never_leave_an_unapplied_operation() {
    let server = start_server_at(RACE_UPDATE_PORT);
    let (session_token, _, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    for round in 0..6u8 {
        let training_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c47{round:02x}");
        let create_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c48{round:02x}");
        let update_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c49{round:02x}");
        let created = post_sync(
            &server,
            &cookie_a,
            &create_operation_for(&training_id, &create_id, ""),
        );
        assert_eq!(created.status, 200, "{}", created.body);

        let (update, takeover) = std::thread::scope(|scope| {
            let update = scope.spawn(|| {
                post_sync(
                    &server,
                    &cookie_a,
                    &update_operation_for(&training_id, &update_id, 2, "race-a"),
                )
            });
            let takeover = scope.spawn(|| {
                post(
                    &server,
                    &cookie_b,
                    &format!("/api/trainings/{training_id}/takeover"),
                )
            });
            (
                update.join().expect("the update must finish"),
                takeover.join().expect("the takeover must finish"),
            )
        });

        assert_eq!(takeover.status, 200, "{}", takeover.body);
        let stored = first_row(
            &server,
            &format!(
                "SELECT memo, owner_terminal_id FROM training_sessions WHERE id = '{training_id}'"
            ),
        );
        assert_eq!(
            stored["owner_terminal_id"],
            json!(TERMINAL_B_ID),
            "the takeover must own the training"
        );
        match update.status {
            200 => {
                assert_eq!(status_of(&update), "applied");
                assert_eq!(
                    stored["memo"],
                    json!("race-a"),
                    "an applied update must leave its memo"
                );
                assert_eq!(applied_count(&server, &update_id), 1);
            }
            403 => {
                assert_eq!(
                    serde_json::from_str::<Value>(&update.body).expect("an error body")["error"]
                        ["code"],
                    json!("forbidden")
                );
                assert_eq!(
                    stored["memo"],
                    json!(""),
                    "a rejected update must not leave its memo"
                );
                assert_eq!(
                    applied_count(&server, &update_id),
                    0,
                    "a rejected update must not leave the operation id"
                );
            }
            other => panic!(
                "the update must be applied or rejected: {other} {}",
                update.body
            ),
        }
        let resent = post_sync(
            &server,
            &cookie_a,
            &update_operation_for(&training_id, &update_id, 2, "race-a"),
        );
        if update.status == 200 {
            assert_eq!(resent.status, 200, "{}", resent.body);
            assert_eq!(status_of(&resent), "already_applied");
            assert_eq!(applied_count(&server, &update_id), 1);
        } else {
            assert_eq!(resent.status, 403, "{}", resent.body);
            assert_eq!(applied_count(&server, &update_id), 0);
        }
    }
}

/// 引き継ぎと終了が重なっても、両方が成功せず、状態と応答が一致する。
///
/// 終了が先に通れば保存済みのサマリーを返し、引き継ぎは 409 になる。引き継ぎが先に
/// 通れば終了は 403 になり、記録中のまま残る。
#[test]
fn a_takeover_and_a_finish_never_both_succeed() {
    let server = start_server_at(RACE_FINISH_PORT);
    let (session_token, _, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    for round in 0..6u8 {
        let training_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c4a{round:02x}");
        let create_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c4b{round:02x}");
        let created = post_sync(
            &server,
            &cookie_a,
            &create_operation_for(&training_id, &create_id, ""),
        );
        assert_eq!(created.status, 200, "{}", created.body);

        let (finish, takeover) = std::thread::scope(|scope| {
            let finish = scope.spawn(|| {
                post(
                    &server,
                    &cookie_a,
                    &format!("/api/trainings/{training_id}/finish"),
                )
            });
            let takeover = scope.spawn(|| {
                post(
                    &server,
                    &cookie_b,
                    &format!("/api/trainings/{training_id}/takeover"),
                )
            });
            (
                finish.join().expect("the finish must finish"),
                takeover.join().expect("the takeover must finish"),
            )
        });

        let stored = first_row(
            &server,
            &format!(
                "SELECT finished_at, owner_terminal_id FROM training_sessions WHERE id = '{training_id}'"
            ),
        );
        assert_eq!(
            applied_training_count(&server, &training_id),
            1,
            "the finish and the takeover must not add operations"
        );
        match finish.status {
            200 => {
                assert_eq!(takeover.status, 409, "{}", takeover.body);
                assert!(
                    stored["finished_at"].as_str().is_some(),
                    "a successful finish must store the finish time"
                );
                assert_eq!(
                    stored["owner_terminal_id"],
                    Value::Null,
                    "a successful finish must release the editing right"
                );
                let summary: TrainingSessionSummary =
                    serde_json::from_str(&finish.body).expect("the finish response is a summary");
                assert_eq!(summary.set_count, 0);
            }
            403 => {
                assert_eq!(
                    serde_json::from_str::<Value>(&finish.body).expect("an error body")["error"]
                        ["code"],
                    json!("forbidden")
                );
                assert_eq!(
                    takeover.status, 200,
                    "the takeover must win when the finish is rejected: {}",
                    takeover.body
                );
                assert_eq!(
                    stored["finished_at"],
                    Value::Null,
                    "a rejected finish must not finish the training"
                );
                assert_eq!(stored["owner_terminal_id"], json!(TERMINAL_B_ID));
            }
            other => panic!(
                "the finish must be saved or rejected: {other} {}",
                finish.body
            ),
        }
    }
}

/// 拒否された操作は操作 ID を残さず、同じ ID の再送も拒否される。
///
/// 終了後の更新は受理され、同じ ID の再送は適用済みとして成功する。
#[test]
fn a_rejected_operation_is_not_recorded_and_the_same_id_stays_rejected() {
    let server = start_server_at(RESEND_PORT);
    let (session_token, _, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    let training_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4c01";
    let create_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4c02";
    let rejected_update_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4c03";
    let rejected_create_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4c04";
    let applied_update_id = "3f2504e0-4f89-41d3-9a0c-0305e82c4c05";
    let created = post_sync(
        &server,
        &cookie_a,
        &create_operation_for(training_id, create_id, ""),
    );
    assert_eq!(created.status, 200, "{}", created.body);
    let takeover = post(
        &server,
        &cookie_b,
        &format!("/api/trainings/{training_id}/takeover"),
    );
    assert_eq!(takeover.status, 200, "{}", takeover.body);

    let rejected = update_operation_for(training_id, rejected_update_id, 2, "rejected");
    for _ in 0..2 {
        let response = post_sync(&server, &cookie_a, &rejected);
        assert_eq!(response.status, 403, "{}", response.body);
    }
    assert_eq!(applied_count(&server, rejected_update_id), 0);
    assert_eq!(d1_memo(&server, training_id), "");

    let rejected_create = create_operation_for(training_id, rejected_create_id, "rejected");
    for _ in 0..2 {
        let response = post_sync(&server, &cookie_a, &rejected_create);
        assert_eq!(response.status, 403, "{}", response.body);
    }
    assert_eq!(applied_count(&server, rejected_create_id), 0);
    assert_eq!(d1_memo(&server, training_id), "");

    let finished = post(
        &server,
        &cookie_b,
        &format!("/api/trainings/{training_id}/finish"),
    );
    assert_eq!(finished.status, 200, "{}", finished.body);

    let applied = update_operation_for(training_id, applied_update_id, 3, "applied");
    let first = post_sync(&server, &cookie_a, &applied);
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(status_of(&first), "applied");
    assert_eq!(applied_count(&server, applied_update_id), 1);
    assert_eq!(d1_memo(&server, training_id), "applied");

    let again = post_sync(&server, &cookie_a, &applied);
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(status_of(&again), "already_applied");
    assert_eq!(applied_count(&server, applied_update_id), 1);
    assert_eq!(d1_memo(&server, training_id), "applied");
}

/// 記録中の作成と引き継ぎが重なっても、0 行の作成は操作 ID を残さない。
#[test]
fn a_takeover_and_a_second_create_never_leave_an_unapplied_operation() {
    let server = start_server_at(RACE_CREATE_PORT);
    let (session_token, _, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    for round in 0..6u8 {
        let training_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c4d{round:02x}");
        let base_create_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c4e{round:02x}");
        let second_create_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c4f{round:02x}");
        let created = post_sync(
            &server,
            &cookie_a,
            &create_operation_for(&training_id, &base_create_id, ""),
        );
        assert_eq!(created.status, 200, "{}", created.body);
        let second_create = create_operation_for(&training_id, &second_create_id, "race-create");

        let (create, takeover) = std::thread::scope(|scope| {
            let create = scope.spawn(|| post_sync(&server, &cookie_a, &second_create));
            let takeover = scope.spawn(|| {
                post(
                    &server,
                    &cookie_b,
                    &format!("/api/trainings/{training_id}/takeover"),
                )
            });
            (
                create.join().expect("the create must finish"),
                takeover.join().expect("the takeover must finish"),
            )
        });

        assert_eq!(takeover.status, 200, "{}", takeover.body);
        let resent = post_sync(&server, &cookie_a, &second_create);
        match create.status {
            200 => {
                assert_eq!(status_of(&create), "applied");
                assert_eq!(applied_count(&server, &second_create_id), 1);
                assert_eq!(d1_memo(&server, &training_id), "race-create");
                assert_eq!(resent.status, 200, "{}", resent.body);
                assert_eq!(status_of(&resent), "already_applied");
                assert_eq!(applied_count(&server, &second_create_id), 1);
            }
            403 => {
                assert_eq!(
                    serde_json::from_str::<Value>(&create.body).expect("an error body")["error"]
                        ["code"],
                    json!("forbidden")
                );
                assert_eq!(applied_count(&server, &second_create_id), 0);
                assert_eq!(d1_memo(&server, &training_id), "");
                assert_eq!(resent.status, 403, "{}", resent.body);
                assert_eq!(applied_count(&server, &second_create_id), 0);
            }
            other => panic!(
                "the create must be applied or rejected: {other} {}",
                create.body
            ),
        }
        assert_eq!(
            first_row(
                &server,
                &format!(
                    "SELECT owner_terminal_id FROM training_sessions WHERE id = '{training_id}'"
                )
            )["owner_terminal_id"],
            json!(TERMINAL_B_ID)
        );
    }
}

/// 所有者の無い記録の claim が終了と競合しても、終了後の更新として適用される。
#[test]
fn a_claim_that_loses_to_the_finish_is_reapplied_after_the_finish() {
    let server = start_server_at(RACE_CLAIM_PORT);
    let (session_token, _, token_a, token_b) = seed(&server);
    let cookie_a = terminal_cookie(&session_token, &token_a);
    let cookie_b = terminal_cookie(&session_token, &token_b);

    for round in 0..6u8 {
        let training_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c50{round:02x}");
        let update_id = format!("3f2504e0-4f89-41d3-9a0c-0305e82c51{round:02x}");
        seed_a_recording_session_without_an_owner(&server, &training_id);
        let update = update_operation_for(&training_id, &update_id, 2, "claim-a");

        let (update_response, finish_response) = std::thread::scope(|scope| {
            let update = scope.spawn(|| post_sync(&server, &cookie_a, &update));
            let finish = scope.spawn(|| {
                post(
                    &server,
                    &cookie_b,
                    &format!("/api/trainings/{training_id}/finish"),
                )
            });
            (
                update.join().expect("the update must finish"),
                finish.join().expect("the finish must finish"),
            )
        });

        assert_eq!(
            update_response.status, 200,
            "the update must apply before or after the finish: {}",
            update_response.body
        );
        assert_eq!(applied_count(&server, &update_id), 1);
        let stored = first_row(
            &server,
            &format!(
                "SELECT memo, finished_at, owner_terminal_id FROM training_sessions WHERE id = '{training_id}'"
            ),
        );
        assert_eq!(stored["memo"], json!("claim-a"));
        match finish_response.status {
            200 => {
                assert!(
                    stored["finished_at"].as_str().is_some(),
                    "a successful finish must store the finish time"
                );
                assert_eq!(stored["owner_terminal_id"], Value::Null);
            }
            403 => {
                assert_eq!(
                    serde_json::from_str::<Value>(&finish_response.body).expect("an error body")
                        ["error"]["code"],
                    json!("forbidden")
                );
                assert_eq!(
                    stored["finished_at"],
                    Value::Null,
                    "a rejected finish must not finish the training"
                );
                assert_eq!(stored["owner_terminal_id"], json!(TERMINAL_A_ID));
            }
            other => panic!(
                "the finish must be saved or rejected: {other} {}",
                finish_response.body
            ),
        }
    }
}
