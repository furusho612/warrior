//! ドロップセットの選択、段の追加、削除、表示、集計の E2E。
//!
//! 対応する要求: `REQ-SET-003`、`REQ-SET-009`、`REQ-REC-005`。
//! 先頭の行ではドロップセットを選べないことを確かめ、通常のセットの直後に
//! ドロップセットを選んで段を追加する。段の削除、全段削除によるセットの削除、
//! 空の段による完了の解除、`D` の表示、終了サマリーのセット数を確かめる。
//! ドロップセットを含む前回の値をコピーしても、古いセットが D1 に残らないことを確かめる。

mod support;

use std::time::{Duration, Instant};

use support::browser::{self, Browser};
use support::record_browser::{
    add_set, change_kind, complete_selector, current_user_id, element_disabled, login,
    option_disabled, reps_selector, seed_profile, segment_ids, segment_reps, segment_weight,
    select_kind, set_ids, start_training, wait_for_complete_ready, weight_selector,
};
use support::record_common;
use support::{DevServer, TestOidc};
use thirtyfour::WebDriver;

const BENCH_EXERCISE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const SERVER_PORT: u16 = 8836;
const PROVIDER_PORT: u16 = 8837;

#[tokio::test]
async fn adds_and_copies_a_drop_set_and_counts_the_record_sets() {
    let browser = Browser::from_env();
    let origin = format!("http://localhost:{SERVER_PORT}");
    let provider_origin = format!("http://localhost:{PROVIDER_PORT}");
    let server = DevServer::start_with_vars(
        SERVER_PORT,
        &[
            ("ORIGIN", origin.as_str()),
            (
                "GOOGLE_OAUTH_AUTHORIZE_URL",
                &format!("{provider_origin}/authorize"),
            ),
            ("GOOGLE_OAUTH_ISSUER", provider_origin.as_str()),
            (
                "GOOGLE_OAUTH_TOKEN_URL",
                &format!("{provider_origin}/token"),
            ),
            (
                "GOOGLE_OAUTH_USERINFO_URL",
                &format!("{provider_origin}/userinfo"),
            ),
            (
                "GOOGLE_OAUTH_REDIRECT_URI",
                &format!("{origin}/api/auth/google/callback"),
            ),
        ],
    );
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");
    let provider = TestOidc::start(PROVIDER_PORT);
    let driver_port = browser::free_port();
    let driver_process = browser::DriverProcess::start(browser, driver_port);
    let driver = browser::open(browser, &driver_process).await;

    login(&driver, &server, &provider, "record-drop-set").await;
    let user_id = current_user_id(&driver).await;
    seed_profile(&server, &user_id);
    let _training_id = start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;

    let first_set = add_set(&driver).await;
    assert!(
        option_disabled(&driver, &first_set, "drop").await,
        "the first set must not select a drop set"
    );
    browser::set_input(&driver, &weight_selector(&first_set), "100").await;
    browser::set_input(&driver, &reps_selector(&first_set), "5").await;
    wait_for_complete_ready(&driver, &first_set).await;
    browser::click(&driver, &complete_selector(&first_set)).await;
    browser::wait_for_attribute(
        &driver,
        &format!("[data-set-id=\"{first_set}\"]"),
        "data-set-completed",
        "true",
    )
    .await;

    let first_drop = add_set(&driver).await;
    change_kind(&driver, &first_drop, "drop").await;
    assert_eq!(
        browser::attribute(
            &driver,
            &format!("[data-set-id=\"{first_drop}\"]"),
            "data-set-number"
        )
        .await,
        "D"
    );
    assert_eq!(
        browser::attribute(
            &driver,
            &format!("[data-set-id=\"{first_drop}\"]"),
            "data-set-drop"
        )
        .await,
        "true"
    );
    browser::click(
        &driver,
        &format!("[data-set-id=\"{first_drop}\"] [data-delete-segment]"),
    )
    .await;
    browser::wait_for_absent(&driver, &format!("[data-set-id=\"{first_drop}\"]")).await;
    wait_for_set_deleted(&server, &first_drop).await;

    let drop_set = add_set(&driver).await;
    change_kind(&driver, &drop_set, "drop").await;
    browser::wait_for_present(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-add-segment]"),
    )
    .await;
    browser::click(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-add-segment]"),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-display-rows", "3").await;

    select_kind(&driver, &drop_set, "normal").await;
    browser::wait_for_present(&driver, "[data-kind-change-confirm]").await;
    browser::wait_for_text(&driver, "2 段目以降の段が消えます").await;
    browser::click(&driver, "[data-kind-change-cancel]").await;
    browser::wait_for_absent(&driver, "[data-kind-change-confirm]").await;
    assert_eq!(
        browser::attribute(
            &driver,
            &format!("[data-set-id=\"{drop_set}\"]"),
            "data-set-kind"
        )
        .await,
        "drop"
    );

    let extra_segments = segment_ids(&driver, &drop_set).await;
    assert_eq!(extra_segments.len(), 2);
    browser::click(
        &driver,
        &format!(
            "[data-segment-id=\"{}\"] [data-delete-segment]",
            extra_segments[1]
        ),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-display-rows", "2").await;
    browser::click(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"] [data-add-segment]"),
    )
    .await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-display-rows", "3").await;

    let segments = segment_ids(&driver, &drop_set).await;
    assert_eq!(segments.len(), 2);
    browser::set_input(&driver, &segment_weight(&segments[0]), "80").await;
    browser::set_input(&driver, &segment_reps(&segments[0]), "8").await;
    browser::set_input(&driver, &segment_weight(&segments[1]), "60").await;
    browser::set_input(&driver, &segment_reps(&segments[1]), "10").await;
    wait_for_complete_ready(&driver, &drop_set).await;
    browser::click(&driver, &complete_selector(&drop_set)).await;
    browser::wait_for_attribute(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"]"),
        "data-set-completed",
        "true",
    )
    .await;
    wait_for_drop_set(&server, &drop_set).await;

    browser::set_input(&driver, &segment_reps(&segments[1]), "0").await;
    browser::wait_for_attribute(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"]"),
        "data-set-completed",
        "false",
    )
    .await;
    browser::set_input(&driver, &segment_reps(&segments[1]), "10").await;
    wait_for_complete_ready(&driver, &drop_set).await;
    browser::click(&driver, &complete_selector(&drop_set)).await;
    browser::wait_for_attribute(
        &driver,
        &format!("[data-set-id=\"{drop_set}\"]"),
        "data-set-completed",
        "true",
    )
    .await;
    wait_for_drop_set(&server, &drop_set).await;
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;

    assert!(
        element_disabled(
            &driver,
            &format!("[data-set-id=\"{first_set}\"] [data-delete-set]")
        )
        .await,
        "a set before a drop set must not be deletable"
    );
    browser::wait_for_text(&driver, "直後のドロップセットも先に削除してください。").await;
    assert_eq!(
        browser::attribute(&driver, "[data-set-table]", "data-set-rows").await,
        "2"
    );
    assert_eq!(
        browser::attribute(&driver, "[data-set-table]", "data-set-display-rows").await,
        "3"
    );
    finish_and_check_the_summary(&driver).await;

    driver
        .goto(server.origin())
        .await
        .expect("the home must open for the copy check");
    browser::wait_for_route(&driver, "/").await;
    browser::wait_for_attribute(
        &driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
    let copy_training_id = start_training(&driver).await;
    browser::wait_for_attribute(
        &driver,
        "[data-profile-checked]",
        "data-profile-checked",
        "true",
    )
    .await;
    browser::add_exercise(&driver, BENCH_EXERCISE_ID).await;
    browser::select_exercise(&driver, BENCH_EXERCISE_ID).await;

    let current_normal = add_set(&driver).await;
    let current_drop = add_set(&driver).await;
    change_kind(&driver, &current_drop, "drop").await;
    browser::wait_for_present(&driver, "[data-copy-previous]").await;
    browser::click(&driver, "[data-copy-previous]").await;
    browser::wait_for_attribute(&driver, "[data-set-table]", "data-set-rows", "2").await;
    browser::wait_for_absent(&driver, &format!("[data-set-id=\"{current_drop}\"]")).await;
    wait_for_copied_sets(
        &server,
        &copy_training_id,
        &[current_normal, current_drop],
        2,
    )
    .await;

    let copied = set_ids(&driver).await;
    assert_eq!(copied.len(), 2, "the copied sets must replace the old sets");
    for set_id in &copied {
        wait_for_complete_ready(&driver, set_id).await;
        browser::click(&driver, &complete_selector(set_id)).await;
        browser::wait_for_attribute(
            &driver,
            &format!("[data-set-id=\"{set_id}\"]"),
            "data-set-completed",
            "true",
        )
        .await;
    }
    browser::wait_for_attribute(&driver, "[data-sync-status]", "data-sync-status", "saved").await;
    finish_and_check_the_summary(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn wait_for_copied_sets(
    server: &DevServer,
    training_id: &str,
    old_ids: &[String],
    expected: i64,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let total_sql = format!(
        "SELECT COUNT(*) AS count FROM sets s \
         JOIN training_exercises te ON te.id = s.training_exercise_id \
         WHERE te.training_id = '{training_id}' AND te.exercise_id = '{BENCH_EXERCISE_ID}'"
    );
    let old_sql = format!(
        "SELECT COUNT(*) AS count FROM sets WHERE id IN ({})",
        old_ids
            .iter()
            .map(|id| format!("'{id}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    loop {
        let total = record_common::optional_row(server, &total_sql)
            .and_then(|row| row["count"].as_i64())
            .unwrap_or(0);
        let old = record_common::optional_row(server, &old_sql)
            .and_then(|row| row["count"].as_i64())
            .unwrap_or(0);
        if total == expected && old == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the training {training_id} still has {total} sets and {old} old sets"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_drop_set(server: &DevServer, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let set_sql = format!("SELECT kind, completed FROM sets WHERE id = '{set_id}'");
    let segment_sql =
        format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{set_id}'");
    loop {
        let set = record_common::optional_row(server, &set_sql);
        let segments = record_common::optional_row(server, &segment_sql);
        if set
            .as_ref()
            .is_some_and(|row| row["kind"] == "drop" && row["completed"].as_i64() == Some(1))
            && segments
                .as_ref()
                .is_some_and(|row| row["count"].as_i64() == Some(2))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the drop set {set_id} was not saved: {set:?}, {segments:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_set_deleted(server: &DevServer, set_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let set_sql = format!("SELECT COUNT(*) AS count FROM sets WHERE id = '{set_id}'");
    let segment_sql =
        format!("SELECT COUNT(*) AS count FROM set_segments WHERE set_id = '{set_id}'");
    loop {
        let sets = record_common::optional_row(server, &set_sql)
            .and_then(|row| row["count"].as_i64())
            .unwrap_or(0);
        let segments = record_common::optional_row(server, &segment_sql)
            .and_then(|row| row["count"].as_i64())
            .unwrap_or(0);
        if sets == 0 && segments == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the set {set_id} was not deleted: sets {sets}, segments {segments}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn finish_and_check_the_summary(driver: &WebDriver) {
    browser::click(driver, "[data-finish]").await;
    browser::wait_for_route(driver, "/summary/:training_id").await;
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-set-count").await,
        "2"
    );
    assert_eq!(
        browser::attribute(driver, "[data-summary]", "data-total-weight-kg").await,
        "1740"
    );
}
