//! 部位別の分析の E2E。
//!
//! 対応する要求: `REQ-ANL-005`、`REQ-ANL-006`、`REQ-ANL-007`。
//! `/analysis` で週単位と月単位を切り替え、指標をセット数と総重量の間で切り替え、
//! 前の週と前の月との増減を実ブラウザで確かめる。記録は D1 に置く。ウォームアップ、
//! ドロップセット、左右別、時間で記録する種目、部位区分「その他」を含む。

mod support;

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use thirtyfour::components::SelectElement;
use thirtyfour::prelude::*;
use warrior_core::jst::{JstDate, UtcInstant};

const BENCH_PRESS_ID: &str = "a7c4e2f0-0000-4000-8000-000000000003";
const BENCH_PRESS_NAME: &str = "ベンチプレス";
const ONE_HAND_ROW_ID: &str = "a7c4e2f0-0000-4000-8000-000000000018";
const ONE_HAND_ROW_NAME: &str = "ワンハンドダンベルロウ";
const SIDE_PLANK_ID: &str = "a7c4e2f0-0000-4000-8000-00000000005a";
const SIDE_PLANK_NAME: &str = "サイドプランク";
const BURPEE_ID: &str = "a7c4e2f0-0000-4000-8000-000000000066";
const BURPEE_NAME: &str = "バーピー";
const SERVER_PORT: u16 = 8872;
const PROVIDER_PORT: u16 = 8873;

const WEEK_CHEST: &str = "a7c4e2f0-0000-4000-8000-000000000301";
const PREVIOUS_WEEK_CHEST: &str = "a7c4e2f0-0000-4000-8000-000000000302";
const MONTH_BACK: &str = "a7c4e2f0-0000-4000-8000-000000000303";
const PREVIOUS_MONTH_BACK: &str = "a7c4e2f0-0000-4000-8000-000000000304";
const WEEK_ABS: &str = "a7c4e2f0-0000-4000-8000-000000000305";
const WEEK_OTHER: &str = "a7c4e2f0-0000-4000-8000-000000000306";
const WEEK_SNAPSHOT: &str = "a7c4e2f0-0000-4000-8000-000000000307";

#[tokio::test]
async fn shows_the_body_part_analysis_for_the_week_and_the_month() {
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
    seed_analysis(&server, &user_id, &fixture);

    open_analysis(&driver, &server).await;
    wait_for_analysis(&driver, "week", "sets").await;
    check_the_week(&driver).await;
    check_the_metrics(&driver).await;
    check_the_month(&driver).await;
    check_the_theme_colors(&driver).await;

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

struct Fixture {
    week_start: JstDate,
    previous_week_start: JstDate,
    month_start: JstDate,
    previous_month_start: JstDate,
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
        let week_start = today.week().start();
        let month_start = today.month_range().start();
        Self {
            week_start,
            previous_week_start: week_start.add_days(-7).expect("the previous week"),
            month_start,
            previous_month_start: month_start.subtract_months(1).expect("the previous month"),
        }
    }
}

async fn login(driver: &WebDriver, server: &DevServer, provider: &TestOidc) {
    driver
        .goto(format!("{}/test/user?sub=analysis", provider.origin()))
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

async fn open_analysis(driver: &WebDriver, server: &DevServer) {
    driver
        .goto(format!("{}/analysis", server.origin()))
        .await
        .expect("the analysis must open");
    browser::wait_for_route(driver, "/analysis").await;
}

async fn check_the_week(driver: &WebDriver) {
    browser::wait_for_attribute(
        driver,
        "[data-analysis-parts]",
        "data-analysis-part-count",
        "8",
    )
    .await;
    browser::wait_for_text(driver, "前の週との差").await;

    assert_eq!(
        part_value(driver, "chest", "data-body-part-sets").await,
        2.0
    );
    assert_eq!(
        part_value(driver, "chest", "data-body-part-weight").await,
        1_740.0
    );
    assert_eq!(
        part_value(driver, "chest", "data-body-part-previous-sets").await,
        1.0
    );
    assert_eq!(
        part_value(driver, "chest", "data-body-part-previous-weight").await,
        500.0
    );
    assert_eq!(
        part_value(driver, "chest", "data-body-part-sets-change").await,
        1.0
    );
    assert_eq!(
        part_value(driver, "chest", "data-body-part-weight-change").await,
        1_240.0
    );

    assert_eq!(part_value(driver, "abs", "data-body-part-sets").await, 1.0);
    assert_eq!(
        part_value(driver, "abs", "data-body-part-weight").await,
        0.0
    );
    assert_eq!(
        part_value(driver, "abs", "data-body-part-sets-change").await,
        1.0
    );

    assert_eq!(
        part_value(driver, "other", "data-body-part-sets").await,
        1.0
    );
    assert_eq!(
        part_value(driver, "other", "data-body-part-weight").await,
        700.0
    );

    assert_eq!(
        part_value(driver, "shoulders", "data-body-part-sets").await,
        1.0
    );
    assert_eq!(
        part_value(driver, "shoulders", "data-body-part-weight").await,
        500.0
    );
}

async fn check_the_metrics(driver: &WebDriver) {
    browser::wait_for_attribute(
        driver,
        "[data-analysis-bar=\"chest\"]",
        "data-analysis-bar-value",
        "2",
    )
    .await;
    select_value(driver, "select[data-analysis-metric-select]", "weight").await;
    wait_for_analysis(driver, "week", "weight").await;
    browser::wait_for_attribute(
        driver,
        "[data-analysis-bar=\"chest\"]",
        "data-analysis-bar-value",
        "1740",
    )
    .await;
    select_value(driver, "select[data-analysis-metric-select]", "sets").await;
    wait_for_analysis(driver, "week", "sets").await;
    browser::wait_for_attribute(
        driver,
        "[data-analysis-bar=\"chest\"]",
        "data-analysis-bar-value",
        "2",
    )
    .await;
}

async fn check_the_month(driver: &WebDriver) {
    select_value(driver, "select[data-analysis-period-select]", "month").await;
    wait_for_analysis(driver, "month", "sets").await;
    browser::wait_for_text(driver, "前の月との差").await;

    assert_eq!(part_value(driver, "back", "data-body-part-sets").await, 1.0);
    assert_eq!(
        part_value(driver, "back", "data-body-part-weight").await,
        98.0
    );
    assert_eq!(
        part_value(driver, "back", "data-body-part-previous-sets").await,
        3.0
    );
    assert_eq!(
        part_value(driver, "back", "data-body-part-previous-weight").await,
        1_200.0
    );
    assert_eq!(
        part_value(driver, "back", "data-body-part-sets-change").await,
        -2.0
    );
    assert_eq!(
        part_value(driver, "back", "data-body-part-weight-change").await,
        -1_102.0
    );
    assert_eq!(
        browser::attribute(
            driver,
            "[data-body-part=\"back\"]",
            "data-body-part-sets-change"
        )
        .await,
        "-2"
    );

    select_value(driver, "select[data-analysis-period-select]", "week").await;
    wait_for_analysis(driver, "week", "sets").await;
}

async fn check_the_theme_colors(driver: &WebDriver) {
    let light = bar_colors(driver, "light").await;
    let dark = bar_colors(driver, "dark").await;
    assert_ne!(light.fill, light.background, "the bar is visible in light");
    assert_ne!(dark.fill, dark.background, "the bar is visible in dark");
    assert_ne!(light.fill, dark.fill, "the part color follows the theme");
}

struct BarColors {
    fill: String,
    background: String,
}

async fn bar_colors(driver: &WebDriver, theme: &str) -> BarColors {
    let value = browser::eval_json(
        driver,
        "const done = arguments[arguments.length - 1]; \
         document.documentElement.setAttribute('data-theme', arguments[0]); \
         const bar = document.querySelector('[data-analysis-bar=\"chest\"]'); \
         const shell = document.querySelector('[data-route]'); \
         if (!bar || !shell) { done(JSON.stringify({ error: 'the bar is missing' })); return; } \
         done(JSON.stringify({ \
           fill: getComputedStyle(bar).fill, \
           background: getComputedStyle(shell).backgroundColor }));",
        vec![json!(theme)],
    )
    .await;
    BarColors {
        fill: value["fill"].as_str().unwrap_or_default().to_owned(),
        background: value["background"].as_str().unwrap_or_default().to_owned(),
    }
}

async fn wait_for_analysis(driver: &WebDriver, period: &str, metric: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let current_period = browser::attribute(
            driver,
            "[data-analysis-chart]",
            "data-analysis-chart-period",
        )
        .await;
        let current_metric = browser::attribute(
            driver,
            "[data-analysis-chart]",
            "data-analysis-chart-metric",
        )
        .await;
        if current_period == period && current_metric == metric {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the analysis did not become {period} with {metric}: {current_period}, {current_metric}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

async fn part_value(driver: &WebDriver, body_part: &str, attribute: &str) -> f64 {
    browser::attribute(
        driver,
        &format!("[data-body-part=\"{body_part}\"]"),
        attribute,
    )
    .await
    .parse()
    .expect("a numeric body part attribute")
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

struct SetSeed {
    kind: &'static str,
    body_weight_kg: Option<f64>,
    segments: Vec<SegmentSeed>,
}

struct SegmentSeed {
    weight_kg: f64,
    reps: i64,
    left: Option<(f64, i64)>,
    right: Option<(f64, i64)>,
    duration_seconds: Option<i64>,
    left_duration_seconds: Option<i64>,
    right_duration_seconds: Option<i64>,
}

impl SegmentSeed {
    fn weight(weight_kg: f64, reps: i64) -> Self {
        Self {
            weight_kg,
            reps,
            left: None,
            right: None,
            duration_seconds: None,
            left_duration_seconds: None,
            right_duration_seconds: None,
        }
    }

    fn unilateral(
        left_weight_kg: f64,
        left_reps: i64,
        right_weight_kg: f64,
        right_reps: i64,
    ) -> Self {
        Self {
            weight_kg: 0.0,
            reps: 0,
            left: Some((left_weight_kg, left_reps)),
            right: Some((right_weight_kg, right_reps)),
            duration_seconds: None,
            left_duration_seconds: None,
            right_duration_seconds: None,
        }
    }

    fn time(left_seconds: i64, right_seconds: i64) -> Self {
        Self {
            weight_kg: 0.0,
            reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
            left_duration_seconds: Some(left_seconds),
            right_duration_seconds: Some(right_seconds),
        }
    }
}

struct TrainingSeed<'a> {
    training_id: &'a str,
    date: JstDate,
    exercise_id: &'a str,
    name: &'a str,
    body_part: &'a str,
    muscles: serde_json::Value,
    record_mode: &'a str,
    laterality: &'a str,
    sets: Vec<SetSeed>,
}

fn seed_analysis(server: &DevServer, user_id: &str, fixture: &Fixture) {
    let chest_muscles = json!([{"muscle": "pectoralis_major_middle", "role": "primary"}]);
    let back_muscles = json!([{"muscle": "latissimus_dorsi", "role": "primary"}]);
    let abs_muscles = json!([{"muscle": "obliques", "role": "primary"}]);
    let shoulders_muscles = json!([{"muscle": "lateral_deltoid", "role": "primary"}]);
    let other_muscles = json!([]);
    let seeds = vec![
        TrainingSeed {
            training_id: WEEK_CHEST,
            date: fixture.week_start,
            exercise_id: BENCH_PRESS_ID,
            name: BENCH_PRESS_NAME,
            body_part: "chest",
            muscles: chest_muscles.clone(),
            record_mode: "weight_and_reps",
            laterality: "bilateral",
            sets: vec![
                SetSeed {
                    kind: "warmup",
                    body_weight_kg: None,
                    segments: vec![SegmentSeed::weight(60.0, 10)],
                },
                SetSeed {
                    kind: "normal",
                    body_weight_kg: None,
                    segments: vec![SegmentSeed::weight(100.0, 5)],
                },
                SetSeed {
                    kind: "drop",
                    body_weight_kg: None,
                    segments: vec![SegmentSeed::weight(80.0, 8), SegmentSeed::weight(60.0, 10)],
                },
            ],
        },
        TrainingSeed {
            training_id: PREVIOUS_WEEK_CHEST,
            date: fixture.previous_week_start,
            exercise_id: BENCH_PRESS_ID,
            name: BENCH_PRESS_NAME,
            body_part: "chest",
            muscles: chest_muscles,
            record_mode: "weight_and_reps",
            laterality: "bilateral",
            sets: vec![SetSeed {
                kind: "normal",
                body_weight_kg: None,
                segments: vec![SegmentSeed::weight(50.0, 10)],
            }],
        },
        TrainingSeed {
            training_id: MONTH_BACK,
            date: fixture.month_start,
            exercise_id: ONE_HAND_ROW_ID,
            name: ONE_HAND_ROW_NAME,
            body_part: "back",
            muscles: back_muscles.clone(),
            record_mode: "weight_and_reps",
            laterality: "unilateral",
            sets: vec![SetSeed {
                kind: "normal",
                body_weight_kg: None,
                segments: vec![SegmentSeed::unilateral(10.0, 5, 12.0, 4)],
            }],
        },
        TrainingSeed {
            training_id: PREVIOUS_MONTH_BACK,
            date: fixture.previous_month_start,
            exercise_id: ONE_HAND_ROW_ID,
            name: ONE_HAND_ROW_NAME,
            body_part: "back",
            muscles: back_muscles,
            record_mode: "weight_and_reps",
            laterality: "unilateral",
            sets: (0..3)
                .map(|_| SetSeed {
                    kind: "normal",
                    body_weight_kg: None,
                    segments: vec![SegmentSeed::unilateral(40.0, 5, 40.0, 5)],
                })
                .collect(),
        },
        TrainingSeed {
            training_id: WEEK_ABS,
            date: fixture.week_start,
            exercise_id: SIDE_PLANK_ID,
            name: SIDE_PLANK_NAME,
            body_part: "abs",
            muscles: abs_muscles,
            record_mode: "time",
            laterality: "unilateral",
            sets: vec![SetSeed {
                kind: "normal",
                body_weight_kg: None,
                segments: vec![SegmentSeed::time(30, 40)],
            }],
        },
        TrainingSeed {
            training_id: WEEK_OTHER,
            date: fixture.week_start,
            exercise_id: BURPEE_ID,
            name: BURPEE_NAME,
            body_part: "other",
            muscles: other_muscles.clone(),
            record_mode: "bodyweight_and_reps",
            laterality: "bilateral",
            sets: vec![SetSeed {
                kind: "normal",
                body_weight_kg: Some(70.0),
                segments: vec![SegmentSeed::weight(0.0, 10)],
            }],
        },
        TrainingSeed {
            training_id: WEEK_SNAPSHOT,
            date: fixture.week_start,
            exercise_id: BURPEE_ID,
            name: BURPEE_NAME,
            body_part: "shoulders",
            muscles: shoulders_muscles,
            record_mode: "bodyweight_and_reps",
            laterality: "bilateral",
            sets: vec![SetSeed {
                kind: "normal",
                body_weight_kg: Some(50.0),
                segments: vec![SegmentSeed::weight(0.0, 10)],
            }],
        },
    ];
    for seed in seeds {
        seed_training(server, user_id, &seed);
    }
}

struct SeededTraining<'a> {
    user_id: &'a str,
    training_id: &'a str,
    training_exercise: &'a str,
    date: &'a str,
    finished_at: &'a str,
}

fn seed_training(server: &DevServer, user_id: &str, seed: &TrainingSeed<'_>) {
    let date = seed.date.to_string();
    let finished_at = format!("{date}T04:00:00Z");
    let training_exercise = format!("{}-exercise", seed.training_id);
    let snapshot = json!({
        "exercise_id": seed.exercise_id,
        "name": seed.name,
        "body_part": seed.body_part,
        "muscles": seed.muscles,
        "record_mode": seed.record_mode,
        "laterality": seed.laterality,
        "grip_mode": "none",
        "grip_options": []
    });
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_sessions \
             (id, user_id, date, started_at, memo, source_kind, finished_at, total_weight_kg, \
              created_at, updated_at) \
             VALUES ('{}', '{user_id}', '{date}', '{date}T02:00:00Z', '', 'empty', \
              '{finished_at}', 0, '{date}T02:00:00Z', '{finished_at}')",
            seed.training_id
        ))
        .expect("the training must be seeded");
    server
        .execute_d1_json(&format!(
            "INSERT INTO training_exercises \
             (id, user_id, training_id, exercise_id, snapshot, position, created_at, updated_at) \
             VALUES ('{training_exercise}', '{user_id}', '{}', '{}', '{}', 1, \
              '{date}T02:00:00Z', '{finished_at}')",
            seed.training_id,
            seed.exercise_id,
            sql_text(&snapshot.to_string())
        ))
        .expect("the training exercise must be seeded");
    let training = SeededTraining {
        user_id,
        training_id: seed.training_id,
        training_exercise: &training_exercise,
        date: &date,
        finished_at: &finished_at,
    };
    for (set_index, set) in seed.sets.iter().enumerate() {
        seed_set(server, &training, set_index, set);
    }
}

fn seed_set(server: &DevServer, training: &SeededTraining<'_>, set_index: usize, set: &SetSeed) {
    let set_id = format!("{}-set-{}", training.training_id, set_index + 1);
    let body_weight_kg = set
        .body_weight_kg
        .map_or_else(|| "NULL".to_owned(), |weight| format!("{weight}"));
    server
        .execute_d1_json(&format!(
            "INSERT INTO sets \
             (id, user_id, training_exercise_id, position, kind, completed, grip, body_weight_kg, \
              created_at, updated_at) \
             VALUES ('{set_id}', '{}', '{}', {}, '{}', 1, NULL, \
              {body_weight_kg}, '{}T02:00:00Z', '{}')",
            training.user_id,
            training.training_exercise,
            set_index + 1,
            set.kind,
            training.date,
            training.finished_at
        ))
        .expect("the set must be seeded");
    for (segment_index, segment) in set.segments.iter().enumerate() {
        let segment_id = format!("{set_id}-segment-{}", segment_index + 1);
        let (left_weight, left_reps) = segment
            .left
            .map_or(("NULL".to_owned(), 0), |(weight, reps)| {
                (format!("{weight}"), reps)
            });
        let (right_weight, right_reps) = segment
            .right
            .map_or(("NULL".to_owned(), 0), |(weight, reps)| {
                (format!("{weight}"), reps)
            });
        let duration_seconds = segment
            .duration_seconds
            .map_or_else(|| "NULL".to_owned(), |seconds| format!("{seconds}"));
        let left_duration_seconds = segment
            .left_duration_seconds
            .map_or_else(|| "NULL".to_owned(), |seconds| format!("{seconds}"));
        let right_duration_seconds = segment
            .right_duration_seconds
            .map_or_else(|| "NULL".to_owned(), |seconds| format!("{seconds}"));
        server
            .execute_d1_json(&format!(
                "INSERT INTO set_segments \
                 (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, \
                  duration_seconds, left_weight_kg, left_reps, left_duration_seconds, \
                  right_weight_kg, right_reps, right_duration_seconds, created_at, updated_at) \
                 VALUES ('{segment_id}', '{}', '{set_id}', {}, {}, {}, 0, 0, \
                  {duration_seconds}, {left_weight}, {left_reps}, {left_duration_seconds}, \
                  {right_weight}, {right_reps}, {right_duration_seconds}, \
                  '{}T02:00:00Z', '{}')",
                training.user_id,
                segment_index + 1,
                segment.weight_kg,
                segment.reps,
                training.date,
                training.finished_at
            ))
            .expect("the set segment must be seeded");
    }
}

fn sql_text(text: &str) -> String {
    text.replace('\'', "''")
}
