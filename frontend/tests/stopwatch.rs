//! ストップウォッチの開始時刻の保存と復元のブラウザのテスト。
//!
//! 対応する要求: `REQ-SET-010`。
//! 開始時刻を端末内に保存し、データベースを開き直した後に、保存した開始時刻から
//! 経過時間を求められることを確かめる。左右別の開始時刻は側ごとに分ける。

use warrior_core::duration;
use warrior_frontend::storage::{StopwatchSide, Storage};
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

mod support;

use support::{database_name, open};

const SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c5701";

#[wasm_bindgen_test]
async fn req_set_010_restores_the_elapsed_time_after_a_reload() {
    let name = database_name("stopwatch");
    let storage = open(&name).await;
    storage
        .save_stopwatch_started_at(SET_ID, StopwatchSide::Whole, 1_000_000)
        .await
        .expect("the start of the whole set must be saved");
    storage
        .save_stopwatch_started_at(SET_ID, StopwatchSide::Left, 2_000_000)
        .await
        .expect("the start of the left side must be saved");
    drop(storage);

    let reopened: Storage = open(&name).await;
    let started = reopened
        .stopwatch_started_at(SET_ID, StopwatchSide::Whole)
        .await
        .expect("the start must be readable")
        .expect("the start of the whole set must remain");
    assert_eq!(started, 1_000_000);
    assert_eq!(duration::elapsed_seconds(started, 1_003_500), 3);

    let left = reopened
        .stopwatch_started_at(SET_ID, StopwatchSide::Left)
        .await
        .expect("the left start must be readable")
        .expect("the start of the left side must remain");
    assert_eq!(left, 2_000_000);
    assert_eq!(
        reopened
            .stopwatch_started_at(SET_ID, StopwatchSide::Right)
            .await
            .expect("the right start must be readable"),
        None
    );

    reopened
        .clear_stopwatch_started_at(SET_ID, StopwatchSide::Left)
        .await
        .expect("the left start must be cleared");
    assert_eq!(
        reopened
            .stopwatch_started_at(SET_ID, StopwatchSide::Left)
            .await
            .expect("the left start must be readable"),
        None
    );
    assert_eq!(
        reopened
            .stopwatch_started_at(SET_ID, StopwatchSide::Whole)
            .await
            .expect("the whole start must be readable"),
        Some(1_000_000)
    );

    reopened
        .clear_account()
        .await
        .expect("the cleanup must work");
}
