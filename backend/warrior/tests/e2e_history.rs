//! 種目別の履歴、自己ベスト、推移のグラフの E2E。
//!
//! 対応する要求: `REQ-ANL-001`、`REQ-ANL-002`、`REQ-ANL-003`、`REQ-ANL-004`、
//! `REQ-SET-006`、`REQ-SET-011`。
//! 種目の履歴の並び、セットの内訳、期間と指標の切り替え、同じ日の記録のまとめ、
//! グリップとアタッチメントの絞り込み、自己ベストの達成日、ライトとダークの
//! グラフの色を実ブラウザで確かめる。記録と自己ベストは D1 に置く。

mod support;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;
use warrior_core::jst::{JstDate, UtcInstant};
use warrior_core::set_calc::estimated_one_rep_max;

const T_BAR_ID: &str = "a7c4e2f0-0000-4000-8000-000000000013";
const T_BAR_NAME: &str = "T バーロウ";
const SIDE_PLANK_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005a";
const SIDE_PLANK_NAME: &str = "サイドプランク";
const SERVER_PORT: u16 = 8870;
const PROVIDER_PORT: u16 = 8871;

const LATE_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000201";
const EARLY_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000202";
const DAYS_AGO_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000203";
const TWO_MONTHS_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000204";
const FOUR_MONTHS_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000205";
const EIGHT_MONTHS_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000206";
const TWO_YEARS_TRAINING: &str = "a7c4e2f0-0000-4000-8000-000000000207";
const PLANK_FIRST: &str = "a7c4e2f0-0000-4000-8000-000000000211";
const PLANK_SECOND: &str = "a7c4e2f0-0000-4000-8000-000000000212";

const GRIP_PRONATED_MEDIUM: &str = r#"{"grip":{"style":"pronated","width":"medium"}}"#;
const GRIP_SUPINATED_NARROW: &str = r#"{"grip":{"style":"supinated","width":"narrow"}}"#;
const PRONATED_KEY: &str = "pronated_medium";
const SUPINATED_KEY: &str = "supinated_narrow";

struct Fixture {
    today: JstDate,
    days_ago: JstDate,
    two_months: JstDate,
    four_months: JstDate,
    eight_months: JstDate,
    two_years: JstDate,
}

impl Fixture {
    fn now() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_secs() as i64;
        let today = UtcInstant::from_epoch_seconds(seconds)
            .jst_date()
            .expect("a representable date");
        Self {
            today,
            days_ago: today.add_days(-20).expect("a past day"),
            two_months: today.subtract_months(2).expect("a past month"),
            four_months: today.subtract_months(4).expect("a past month"),
            eight_months: today.subtract_months(8).expect("a past month"),
            two_years: today.subtract_months(24).expect("a past month"),
        }
    }
}

struct WeightSeed<'a> {
    training_id: &'a str,
    date: JstDate,
    finished_at: &'a str,
    grip: &'a str,
    weight_kg: f64,
    reps: i64,
    rpe: Option<f64>,
    momentum_reps: i64,
    assisted_reps: i64,
}

#[tokio::test]
async fn shows_the_history_the_bests_and_the_graph_with_the_period_and_the_metric() {
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

    login(&driver, &server, &provider).await;
    let user_id = current_user_id(&driver).await;
    let fixture = Fixture::now();
    seed_history(&server, &user_id, &fixture);
    seed_bests(&server, &user_id, &fixture);

    open_history(&driver, &server, T_BAR_ID).await;
    check_the_history_and_the_bests(&driver, &fixture).await;
    check_the_periods(&driver).await;
    check_the_metrics(&driver).await;
    check_the_grip_filter(&driver).await;
    check_the_theme_colors(&driver).await;

    open_history(&driver, &server, SIDE_PLANK_ID).await;
    check_the_time_history(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=history", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(driver, server).await;
    browser::wait_for_route(driver, "/").await;
    browser::wait_for_attribute(
        driver,
        "[data-start]",
        "data-active-session-checked",
        "true",
    )
    .await;
}

async fn open_history(driver: &WebDriver, server: &DevServer, exercise_id: &str) {
    driver
        .goto(format!(
            "{}/exercises/{exercise_id}/history",
            server.origin()
        ))
        .await
        .expect("the exercise history must open");
    browser::wait_for_route(driver, "/exercises/:id/history").await;
}

async fn check_the_history_and_the_bests(driver: &WebDriver, fixture: &Fixture) {
    browser::wait_for_attribute(driver, "[data-history-list]", "data-history-trainings", "7").await;
    browser::wait_for_attribute(
        driver,
        "[data-history-bests]",
        "data-history-bests-count",
        "6",
    )
    .await;

    let trainings = training_ids(driver).await;
    assert_eq!(
        trainings,
        vec![
            LATE_TRAINING.to_owned(),
            EARLY_TRAINING.to_owned(),
            DAYS_AGO_TRAINING.to_owned(),
            TWO_MONTHS_TRAINING.to_owned(),
            FOUR_MONTHS_TRAINING.to_owned(),
            EIGHT_MONTHS_TRAINING.to_owned(),
            TWO_YEARS_TRAINING.to_owned(),
        ],
        "the history is ordered by the date and the finished time"
    );

    let late = format!("[data-history-training=\"{LATE_TRAINING}\"] [data-history-set=\"1\"]");
    browser::wait_for_attribute(driver, &late, "data-history-set-text", "100 kg × 5 回").await;
    assert_eq!(
        browser::attribute(driver, &late, "data-history-set-notes").await,
        "通常 / RPE 8.0 / 順手 ミディアム / 反動 1 回 / 補助 1 回"
    );
    let early = format!("[data-history-training=\"{EARLY_TRAINING}\"] [data-history-set=\"1\"]");
    assert_eq!(
        browser::attribute(driver, &early, "data-history-set-text").await,
        "80 kg × 10 回"
    );

    let bests = best_rows(driver).await;
    assert_eq!(bests.len(), 6);
    let max_pronated = bests
        .iter()
        .find(|best| best.kind == "max_weight" && best.equipment == "順手 ミディアム")
        .expect("the pronated max weight best");
    assert!((max_pronated.value - 100.0).abs() < 1e-9);
    assert_eq!(max_pronated.date, fixture.today.to_string());
    let total_pronated = bests
        .iter()
        .find(|best| best.kind == "total_weight" && best.equipment == "順手 ミディアム")
        .expect("the pronated total weight best");
    assert!((total_pronated.value - 450.0).abs() < 1e-9);
    assert_eq!(total_pronated.date, fixture.days_ago.to_string());
    let one_rep_max_pronated = bests
        .iter()
        .find(|best| best.kind == "estimated_one_rep_max" && best.equipment == "順手 ミディアム")
        .expect("the pronated one rep max best");
    assert!(
        (one_rep_max_pronated.value - estimated_one_rep_max(100.0, 3).expect("the formula")).abs()
            < 1e-9
    );
    assert_eq!(one_rep_max_pronated.date, fixture.today.to_string());
}

async fn check_the_periods(driver: &WebDriver) {
    wait_for_graph(driver, "3m", 3).await;
    select_value(driver, "select[data-history-period]", "1m").await;
    wait_for_graph(driver, "1m", 2).await;
    select_value(driver, "select[data-history-period]", "6m").await;
    wait_for_graph(driver, "6m", 4).await;
    select_value(driver, "select[data-history-period]", "1y").await;
    wait_for_graph(driver, "1y", 5).await;
    select_value(driver, "select[data-history-period]", "all").await;
    wait_for_graph(driver, "all", 6).await;
    select_value(driver, "select[data-history-period]", "3m").await;
    wait_for_graph(driver, "3m", 3).await;
}

async fn check_the_metrics(driver: &WebDriver) {
    select_value(driver, "select[data-history-metric]", "total_weight").await;
    wait_for_graph_metric(driver, "total_weight").await;
    let points = graph_points(driver).await;
    assert_eq!(points.len(), 3);
    let today = points.last().expect("the newest point");
    assert!((today.1 - 1_200.0).abs() < 1e-9, "{points:?}");

    select_value(
        driver,
        "select[data-history-metric]",
        "estimated_one_rep_max",
    )
    .await;
    wait_for_graph_metric(driver, "estimated_one_rep_max").await;
    let points = graph_points(driver).await;
    let expected = estimated_one_rep_max(100.0, 3)
        .expect("the formula")
        .max(estimated_one_rep_max(80.0, 10).expect("the formula"));
    assert!((points.last().expect("the newest point").1 - expected).abs() < 1e-9);

    select_value(driver, "select[data-history-metric]", "max_weight").await;
    wait_for_graph_metric(driver, "max_weight").await;
}

async fn check_the_grip_filter(driver: &WebDriver) {
    select_value(driver, "select[data-history-grip-filter]", SUPINATED_KEY).await;
    browser::wait_for_attribute(driver, "[data-history-list]", "data-history-trainings", "1").await;
    browser::wait_for_attribute(
        driver,
        "[data-history-bests]",
        "data-history-bests-count",
        "3",
    )
    .await;
    browser::wait_for_attribute(driver, "[data-history-graph]", "data-graph-points", "1").await;
    assert_eq!(training_ids(driver).await, vec![EARLY_TRAINING.to_owned()]);
    let bests = best_rows(driver).await;
    assert_eq!(bests.len(), 3);
    for best in &bests {
        assert_eq!(best.equipment, "逆手 ナロー", "{best:?}");
    }
    let max = bests
        .iter()
        .find(|best| best.kind == "max_weight")
        .expect("the supinated max weight best");
    assert!((max.value - 80.0).abs() < 1e-9);
    let total = bests
        .iter()
        .find(|best| best.kind == "total_weight")
        .expect("the supinated total weight best");
    assert!((total.value - 800.0).abs() < 1e-9);
    let points = graph_points(driver).await;
    assert_eq!(points.len(), 1);
    assert!((points[0].1 - 80.0).abs() < 1e-9, "{points:?}");
}

async fn check_the_theme_colors(driver: &WebDriver) {
    let light = theme_colors(driver, "light").await;
    let dark = theme_colors(driver, "dark").await;
    assert_ne!(
        light.stroke, dark.stroke,
        "the line color follows the theme"
    );
    assert_ne!(light.stroke, light.background);
    assert_ne!(dark.stroke, dark.background);
}

async fn check_the_time_history(driver: &WebDriver) {
    browser::wait_for_attribute(driver, "[data-history-list]", "data-history-trainings", "2").await;
    browser::wait_for_attribute(
        driver,
        "[data-history-bests]",
        "data-history-bests-count",
        "1",
    )
    .await;
    browser::wait_for_attribute(
        driver,
        "[data-history-best=\"longest_duration\"]",
        "data-history-best-value",
        "50",
    )
    .await;
    let best = browser::attribute(
        driver,
        "[data-history-best=\"longest_duration\"]",
        "data-history-best-date",
    )
    .await;
    assert!(!best.is_empty());
    wait_for_graph_metric(driver, "longest_duration").await;
    let points = graph_points(driver).await;
    assert_eq!(points.len(), 1);
    assert!((points[0].1 - 50.0).abs() < 1e-9, "{points:?}");

    select_value(driver, "select[data-history-metric]", "total_duration").await;
    wait_for_graph_metric(driver, "total_duration").await;
    let points = graph_points(driver).await;
    assert_eq!(points.len(), 1);
    assert!(
        (points[0].1 - 170.0).abs() < 1e-9,
        "the same day adds the left and the right durations: {points:?}"
    );
}

async fn wait_for_graph(driver: &WebDriver, period: &str, points: u32) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let current_period =
            browser::attribute(driver, "[data-graph-period]", "data-graph-period").await;
        let current_points =
            browser::attribute(driver, "[data-history-graph]", "data-graph-points").await;
        if current_period == period && current_points == points.to_string() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the graph did not become the period {period} with {points} points: {current_period}, {current_points}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn wait_for_graph_metric(driver: &WebDriver, metric: &str) {
    browser::wait_for_attribute(driver, "[data-graph-metric]", "data-graph-metric", metric).await;
}

async fn training_ids(driver: &WebDriver) -> Vec<String> {
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-history-training]')) \
           .map(function(element) { return element.getAttribute('data-history-training'); })));",
        Vec::new(),
    )
    .await;
    values
        .as_array()
        .expect("the training ids")
        .iter()
        .map(|value| value.as_str().expect("a training id").to_owned())
        .collect()
}

#[derive(Debug)]
struct BestRow {
    kind: String,
    value: f64,
    date: String,
    equipment: String,
}

async fn best_rows(driver: &WebDriver) -> Vec<BestRow> {
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-history-best]')) \
           .map(function(element) { return [ \
             element.getAttribute('data-history-best'), \
             Number(element.getAttribute('data-history-best-value')), \
             element.getAttribute('data-history-best-date'), \
             element.getAttribute('data-history-best-equipment') || '' ]; })));",
        Vec::new(),
    )
    .await;
    values
        .as_array()
        .expect("the bests")
        .iter()
        .map(|item| BestRow {
            kind: item[0].as_str().expect("a kind").to_owned(),
            value: item[1].as_f64().expect("a value"),
            date: item[2].as_str().expect("a date").to_owned(),
            equipment: item[3].as_str().unwrap_or_default().to_owned(),
        })
        .collect()
}

async fn graph_points(driver: &WebDriver) -> Vec<(String, f64)> {
    let values = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         done(JSON.stringify(Array.from(document.querySelectorAll('[data-graph-point]')) \
           .map(function(element) { return [ \
             element.getAttribute('data-graph-date'), \
             Number(element.getAttribute('data-graph-value')) ]; })));",
        Vec::new(),
    )
    .await;
    values
        .as_array()
        .expect("the graph points")
        .iter()
        .map(|item| {
            (
                item[0].as_str().expect("a date").to_owned(),
                item[1].as_f64().expect("a value"),
            )
        })
        .collect()
}

struct ThemeColors {
    stroke: String,
    background: String,
}

async fn theme_colors(driver: &WebDriver, theme: &str) -> ThemeColors {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         document.documentElement.setAttribute('data-theme', arguments[0]); \
         const line = document.querySelector('[data-graph-line]'); \
         const shell = document.querySelector('[data-route]'); \
         if (!line || !shell) { done(JSON.stringify({ error: 'the graph is missing' })); return; } \
         done(JSON.stringify({ \
           stroke: getComputedStyle(line).stroke, \
           background: getComputedStyle(shell).backgroundColor }));",
        vec![json!(theme)],
    )
    .await;
    ThemeColors {
        stroke: value["stroke"].as_str().unwrap_or_default().to_owned(),
        background: value["background"].as_str().unwrap_or_default().to_owned(),
    }
}

async fn select_value(driver: &WebDriver, selector: &str, value: &str) {
    let select = driver
        .find(By::Css(selector))
        .await
        .unwrap_or_else(|error| panic!("the select {selector} must exist: {error}"));
    let select = SelectElement::new(&select)
        .await
        .unwrap_or_else(|error| panic!("the select {selector} must be a select: {error}"));
    select
        .select_by_value(value)
        .await
        .unwrap_or_else(|error| panic!("the value {value} must be selectable: {error}"));
}

fn seed_history(server: &DevServer, user_id: &str, fixture: &Fixture) {
    let mut seeds = vec![
        WeightSeed {
            training_id: LATE_TRAINING,
            date: fixture.today,
            finished_at: "05:00:00",
            grip: GRIP_PRONATED_MEDIUM,
            weight_kg: 100.0,
            reps: 5,
            rpe: Some(8.0),
            momentum_reps: 1,
            assisted_reps: 1,
        },
        WeightSeed {
            training_id: EARLY_TRAINING,
            date: fixture.today,
            finished_at: "03:00:00",
            grip: GRIP_SUPINATED_NARROW,
            weight_kg: 80.0,
            reps: 10,
            rpe: None,
            momentum_reps: 0,
            assisted_reps: 0,
        },
    ];
    for (training_id, date, weight_kg) in [
        (DAYS_AGO_TRAINING, fixture.days_ago, 90.0),
        (TWO_MONTHS_TRAINING, fixture.two_months, 70.0),
        (FOUR_MONTHS_TRAINING, fixture.four_months, 60.0),
        (EIGHT_MONTHS_TRAINING, fixture.eight_months, 50.0),
        (TWO_YEARS_TRAINING, fixture.two_years, 40.0),
    ] {
        seeds.push(WeightSeed {
            training_id,
            date,
            finished_at: "04:00:00",
            grip: GRIP_PRONATED_MEDIUM,
            weight_kg,
            reps: 5,
            rpe: None,
            momentum_reps: 0,
            assisted_reps: 0,
        });
    }
    for seed in seeds {
        seed_weight_training(server, user_id, &seed);
    }
    seed_time_training(server, user_id, PLANK_FIRST, fixture.today, 30, 40);
    seed_time_training(server, user_id, PLANK_SECOND, fixture.today, 50, 50);
}

fn seed_weight_training(server: &DevServer, user_id: &str, seed: &WeightSeed<'_>) {
    let date = seed.date.to_string();
    let finished_at = format!("{date}T{}Z", seed.finished_at);
    let training_exercise = format!("{}-exercise", seed.training_id);
    let set_id = format!("{}-set", seed.training_id);
    let segment_id = format!("{}-segment", seed.training_id);
    let snapshot = json!({
        "exercise_id": T_BAR_ID,
        "name": T_BAR_NAME,
        "body_part": "back",
        "muscles": [{"muscle": "latissimus_dorsi", "role": "primary"}],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "grip",
        "grip_options": [
            {"grip": {"style": "pronated", "width": "medium"}},
            {"grip": {"style": "supinated", "width": "narrow"}}
        ]
    });
    let rpe = seed
        .rpe
        .map_or_else(|| "NULL".to_owned(), |rpe| format!("{rpe}"));
    for sql in [
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, total_weight_kg, \
              created_at, updated_at) \
             VALUES ('{}', '{user_id}', '{date}', '{date}T02:00:00Z', '', 'empty', \
              '{finished_at}', 0, '{date}T02:00:00Z', '{finished_at}')",
            seed.training_id
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{training_exercise}', '{user_id}', '{}', '{T_BAR_ID}', '{}', 1, \
              '{date}T02:00:00Z', '{finished_at}')",
            seed.training_id,
            sql_text(&snapshot.to_string())
        ),
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, rpe, grip, \
              created_at, updated_at) \
             VALUES ('{set_id}', '{user_id}', '{training_exercise}', 1, 'normal', 1, {rpe}, \
              '{}', '{date}T02:00:00Z', '{finished_at}')",
            sql_text(seed.grip)
        ),
        format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              created_at, updated_at) \
             VALUES ('{segment_id}', '{user_id}', '{set_id}', 1, {}, {}, {}, {}, \
              '{date}T02:00:00Z', '{finished_at}')",
            seed.weight_kg, seed.reps, seed.momentum_reps, seed.assisted_reps
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the weight training must be seeded");
    }
}

fn seed_time_training(
    server: &DevServer,
    user_id: &str,
    training_id: &str,
    date: JstDate,
    left_seconds: i64,
    right_seconds: i64,
) {
    let date = date.to_string();
    let training_exercise = format!("{training_id}-exercise");
    let set_id = format!("{training_id}-set");
    let segment_id = format!("{training_id}-segment");
    let snapshot = json!({
        "exercise_id": SIDE_PLANK_ID,
        "name": SIDE_PLANK_NAME,
        "body_part": "abs",
        "muscles": [],
        "record_mode": "time",
        "laterality": "unilateral",
        "grip_mode": "none",
        "grip_options": []
    });
    for sql in [
        format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, total_weight_kg, \
              created_at, updated_at) \
             VALUES ('{training_id}', '{user_id}', '{date}', '{date}T02:00:00Z', '', 'empty', \
              '{date}T04:00:00Z', 0, '{date}T02:00:00Z', '{date}T04:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{training_exercise}', '{user_id}', '{training_id}', '{SIDE_PLANK_ID}', \
              '{}', 1, '{date}T02:00:00Z', '{date}T04:00:00Z')",
            sql_text(&snapshot.to_string())
        ),
        format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, grip, created_at, \
              updated_at) \
             VALUES ('{set_id}', '{user_id}', '{training_exercise}', 1, 'normal', 1, NULL, \
              '{date}T02:00:00Z', '{date}T04:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments \
             (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
              left_duration_seconds, right_duration_seconds, created_at, updated_at) \
             VALUES ('{segment_id}', '{user_id}', '{set_id}', 1, 0, 0, 0, 0, \
              {left_seconds}, {right_seconds}, '{date}T02:00:00Z', '{date}T04:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the time training must be seeded");
    }
}

fn seed_bests(server: &DevServer, user_id: &str, fixture: &Fixture) {
    let pronated_one_rep_max = estimated_one_rep_max(100.0, 3).expect("the pronated one rep max");
    let supinated_one_rep_max = estimated_one_rep_max(80.0, 10).expect("the supinated one rep max");
    for seed in [
        BestSeed {
            exercise_id: T_BAR_ID,
            grip: PRONATED_KEY,
            attachment: "",
            kind: "max_weight",
            value: 100.0,
            date: fixture.today,
            training_id: LATE_TRAINING,
        },
        BestSeed {
            exercise_id: T_BAR_ID,
            grip: PRONATED_KEY,
            attachment: "",
            kind: "estimated_one_rep_max",
            value: pronated_one_rep_max,
            date: fixture.today,
            training_id: LATE_TRAINING,
        },
        BestSeed {
            exercise_id: T_BAR_ID,
            grip: PRONATED_KEY,
            attachment: "",
            kind: "total_weight",
            value: 450.0,
            date: fixture.days_ago,
            training_id: DAYS_AGO_TRAINING,
        },
        BestSeed {
            exercise_id: T_BAR_ID,
            grip: SUPINATED_KEY,
            attachment: "",
            kind: "max_weight",
            value: 80.0,
            date: fixture.today,
            training_id: EARLY_TRAINING,
        },
        BestSeed {
            exercise_id: T_BAR_ID,
            grip: SUPINATED_KEY,
            attachment: "",
            kind: "estimated_one_rep_max",
            value: supinated_one_rep_max,
            date: fixture.today,
            training_id: EARLY_TRAINING,
        },
        BestSeed {
            exercise_id: T_BAR_ID,
            grip: SUPINATED_KEY,
            attachment: "",
            kind: "total_weight",
            value: 800.0,
            date: fixture.today,
            training_id: EARLY_TRAINING,
        },
        BestSeed {
            exercise_id: SIDE_PLANK_ID,
            grip: "",
            attachment: "",
            kind: "longest_duration",
            value: 50.0,
            date: fixture.today,
            training_id: PLANK_SECOND,
        },
    ] {
        seed_best(server, user_id, &seed);
    }
}

struct BestSeed<'a> {
    exercise_id: &'a str,
    grip: &'a str,
    attachment: &'a str,
    kind: &'a str,
    value: f64,
    date: JstDate,
    training_id: &'a str,
}

fn seed_best(server: &DevServer, user_id: &str, seed: &BestSeed<'_>) {
    server
        .execute_d1_json(&format!(
            "INSERT INTO personal_bests \
             (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id, \
              created_at, updated_at) \
             VALUES ('{user_id}', '{}', '{}', '{}', '{}', {}, \
              '{}', '{}', '{}T03:00:00Z', '{}T03:00:00Z')",
            seed.exercise_id,
            seed.grip,
            seed.attachment,
            seed.kind,
            seed.value,
            seed.date,
            seed.training_id,
            seed.date,
            seed.date
        ))
        .expect("the personal best must be seeded");
}

fn sql_text(text: &str) -> String {
    text.replace('\'', "''")
}
