//! マイページからの記録の CSV の書き出しとダウンロードの E2E。
//!
//! 対応する要求: `REQ-MYP-008`。
//! マイページのデータの区分から書き出しを実行し、ブラウザが保存した CSV の列と行数を
//! 確かめる。

mod support;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use support::browser::{self, Browser};
use support::{current_user_id, DevServer, TestOidc};
use warrior_core::csv::{self, COLUMNS};

const SERVER_PORT: u16 = 8880;
const PROVIDER_PORT: u16 = 8881;
const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c6501";
const SESSION_MEMO: &str = "腰に注意";

#[tokio::test]
async fn req_myp_008_the_export_downloads_the_csv_from_mypage() {
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
    let driver_process = browser::DriverProcess::start(Browser::Chrome, driver_port);
    let download_dir = DownloadDir::new();
    let driver = browser::open_with_download_dir(&driver_process, download_dir.path()).await;

    driver
        .goto(format!("{}/test/user?sub=csv-export", provider.origin()))
        .await
        .expect("the test user must be selected");
    browser::login_with_google(&driver, &server).await;
    let user_id = current_user_id(&driver).await;
    seed_training(&server, &user_id);

    driver
        .goto(format!("{origin}/mypage"))
        .await
        .expect("the mypage must open");
    browser::wait_for_route(&driver, "/mypage").await;
    browser::wait_for_present(&driver, "[data-export-start]").await;
    browser::click(&driver, "[data-export-start]").await;
    browser::wait_for_present(&driver, "[data-export-download]").await;
    browser::click(&driver, "[data-export-download]").await;

    let path = wait_for_csv(download_dir.path());
    let bytes = std::fs::read(&path).expect("the downloaded CSV must be readable");
    assert!(
        bytes.starts_with(b"\xEF\xBB\xBF"),
        "the CSV must have a BOM"
    );
    let text = String::from_utf8(bytes).expect("the CSV is UTF-8");
    let rows = csv::parse_document(&text).expect("the downloaded file is CSV");
    assert_eq!(rows.len(), 4, "the header and three sets");
    assert_eq!(rows[0], COLUMNS);
    assert_eq!(rows[1][1], "ベンチプレス");
    assert_eq!(rows[1][0], "2026-11-12");
    assert_eq!(rows[1][3], "1");
    assert_eq!(rows[1][4], "ウォームアップ");
    assert_eq!(rows[2][3], "2");
    assert_eq!(rows[2][4], "ドロップセット");
    assert_eq!(rows[2][5], "100;80");
    assert_eq!(rows[2][6], "8;6");
    assert_eq!(rows[3][1], "プランク");
    assert_eq!(rows[3][4], "通常");
    assert_eq!(rows[3][7], "60");
    assert_eq!(rows[1][11], SESSION_MEMO);

    browser::assert_no_panic(&driver).await;
    driver.quit().await.expect("the browser must close");
}

fn seed_training(server: &DevServer, user_id: &str) {
    let bench = json!({
        "exercise_id": "master-bench",
        "name": "ベンチプレス",
        "body_part": "chest",
        "muscles": [],
        "record_mode": "weight_and_reps",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    let plank = json!({
        "exercise_id": "master-plank",
        "name": "プランク",
        "body_part": "abs",
        "muscles": [],
        "record_mode": "time",
        "laterality": "bilateral",
        "grip_mode": "none",
        "grip_options": []
    })
    .to_string();
    for sql in [
        format!(
            "INSERT INTO training_sessions (id, user_id, date, started_at, memo, created_at, updated_at) \
             VALUES ('{TRAINING_ID}', '{user_id}', '2026-11-12', '2026-11-12T03:00:00Z', \
             '{SESSION_MEMO}', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z')"
        ),
        format!(
            "INSERT INTO training_exercises (id, user_id, training_id, exercise_id, snapshot, position, memo, created_at, updated_at) \
             VALUES ('{TRAINING_ID}-bench', '{user_id}', '{TRAINING_ID}', 'master-bench', \
             '{bench}', 0, '', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z'), \
             ('{TRAINING_ID}-plank', '{user_id}', '{TRAINING_ID}', 'master-plank', \
             '{plank}', 1, '', '2026-11-12T03:00:00Z', '2026-11-12T04:00:00Z')"
        ),
        format!(
            "INSERT INTO sets (id, user_id, training_exercise_id, position, kind, completed, created_at, updated_at) \
             VALUES ('{TRAINING_ID}-warmup', '{user_id}', '{TRAINING_ID}-bench', 1, 'warmup', 1, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-drop', '{user_id}', '{TRAINING_ID}-bench', 2, 'drop', 1, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-time', '{user_id}', '{TRAINING_ID}-plank', 1, 'normal', 1, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
        format!(
            "INSERT INTO set_segments (id, user_id, set_id, position, weight_kg, reps, momentum_reps, assisted_reps, duration_seconds, created_at, updated_at) \
             VALUES ('{TRAINING_ID}-warmup-1', '{user_id}', '{TRAINING_ID}-warmup', 1, 60.0, 10, 0, 0, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-drop-1', '{user_id}', '{TRAINING_ID}-drop', 1, 100.0, 8, 0, 0, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-drop-2', '{user_id}', '{TRAINING_ID}-drop', 2, 80.0, 6, 0, 0, NULL, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z'), \
             ('{TRAINING_ID}-time-1', '{user_id}', '{TRAINING_ID}-time', 1, 0.0, 0, 0, 0, 60, \
             '2026-11-12T03:00:00Z', '2026-11-12T03:00:00Z')"
        ),
    ] {
        server
            .execute_d1_json(&sql)
            .expect("the training rows must be seeded");
    }
}

fn wait_for_csv(directory: &Path) -> PathBuf {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) == Some("csv") {
                    return path;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "the CSV was not downloaded within 60 seconds"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// テスト専用のダウンロード先。テストの終了時に削除する。
struct DownloadDir(PathBuf);

impl DownloadDir {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("warrior-e2e-csv-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("the download directory must be created");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for DownloadDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
