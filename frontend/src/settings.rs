//! 利用者共通の設定の API、端末内のキャッシュ、送信待ちの列 (REQ-MYP-002、REQ-MYP-003、REQ-MYP-005)。
//!
//! 取得した設定は利用者 ID ごとに端末内へキャッシュし、変更は端末内へ保存してから
//! 送信待ちの列へ積む。送信は入力の完了条件にしない。端末ごとの設定は
//! `storage` が持ち、このモジュールは扱わない。

use std::cell::{Cell, RefCell};
use std::fmt;
use std::future::poll_fn;
use std::task::{Poll, Waker};

use serde::Deserialize;
use warrior_core::nutrition::MacroTarget;
use warrior_core::settings::{DisplayMode, Settings, SettingsError};

use crate::http::{self, HttpError};
use crate::storage;
use crate::theme;

thread_local! {
    static CURRENT_USER: RefCell<Option<String>> = const { RefCell::new(None) };
    static UPDATING: Cell<bool> = const { Cell::new(false) };
    static UPDATE_WAITERS: RefCell<Vec<Waker>> = const { RefCell::new(Vec::new()) };
}

/// 設定の更新の誤り。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    /// ログインしていない。
    Unauthenticated,
    /// 設定の値が範囲外である。
    Invalid(SettingsError),
    /// 端末内へ保存できない。
    Storage,
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthenticated => formatter.write_str("authentication required"),
            Self::Invalid(error) => write!(formatter, "{error}"),
            Self::Storage => formatter.write_str("the settings could not be stored"),
        }
    }
}

impl std::error::Error for UpdateError {}

#[derive(Debug, Deserialize)]
struct CurrentUser {
    id: String,
}

/// ログイン中の利用者の ID を返す。取得できなければ None。
pub async fn current_user_id() -> Option<String> {
    if let Some(user_id) = CURRENT_USER.with(|current| current.borrow().clone()) {
        return Some(user_id);
    }
    let user: CurrentUser = http::request_json("GET", "/api/me").await.ok()?;
    CURRENT_USER.with(|current| {
        *current.borrow_mut() = Some(user.id.clone());
    });
    if let Ok(storage) = storage::current().await {
        let _ = storage.set_settings_owner(&user.id).await;
    }
    Some(user.id)
}

/// セッションに属する端末内の設定の入口を忘れる。端末ごとの設定は消さない。
pub async fn forget_session() {
    CURRENT_USER.with(|current| {
        *current.borrow_mut() = None;
    });
    theme::forget_display_mode();
    if let Ok(storage) = storage::current().await {
        let _ = storage.clear_settings_owner().await;
    }
}

/// 利用者共通の設定を取得する。
pub async fn fetch() -> Result<Settings, HttpError> {
    http::request_json("GET", "/api/settings").await
}

/// 利用者共通の設定を読む。端末内のキャッシュを先に使い、取得できた応答で更新する。
pub async fn load() -> Settings {
    let Some(user_id) = current_user_id().await else {
        let settings = Settings::default();
        apply_theme(settings);
        return settings;
    };
    let storage = storage::current().await.ok();
    let pending = match &storage {
        Some(storage) => storage
            .pending_user_settings(&user_id)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    if pending.is_empty() {
        if let Ok(settings) = fetch().await {
            if settings.validate().is_ok() {
                if let Some(storage) = &storage {
                    let _ = storage.cache_user_settings(&user_id, &settings).await;
                }
                apply_theme(settings);
                return settings;
            }
        }
    }
    let cached = match &storage {
        Some(storage) => storage.cached_user_settings(&user_id).await.ok().flatten(),
        None => None,
    }
    .unwrap_or_default();
    apply_theme(cached);
    cached
}

/// レスト時間の既定値を返す。端末内のキャッシュが無いときは初期値を返す。
pub async fn rest_default_seconds() -> u32 {
    let Some(user_id) = current_user_id().await else {
        return Settings::default().rest_default_seconds;
    };
    let Ok(storage) = storage::current().await else {
        return Settings::default().rest_default_seconds;
    };
    storage
        .cached_user_settings(&user_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .rest_default_seconds
}

/// レスト時間の既定値を更新する。
pub async fn set_rest_default_seconds(seconds: u32) -> Result<Settings, UpdateError> {
    update_with(|settings| {
        settings.rest_default_seconds = seconds;
    })
    .await
}

/// 表示モードを更新する。
pub async fn set_display_mode(mode: DisplayMode) -> Result<Settings, UpdateError> {
    update_with(|settings| {
        settings.display_mode = mode;
    })
    .await
}

/// PFC の配分を更新する。
pub async fn set_macro_target(target: MacroTarget) -> Result<Settings, UpdateError> {
    update_with(|settings| {
        settings.macro_target = target;
    })
    .await
}

/// PFC の配分を計算ルールの既定値に戻す。
pub async fn reset_macro_target() -> Result<Settings, UpdateError> {
    update_with(|settings| {
        settings.macro_target = MacroTarget::default();
    })
    .await
}

async fn update_with(change: impl FnOnce(&mut Settings)) -> Result<Settings, UpdateError> {
    let user_id = current_user_id()
        .await
        .ok_or(UpdateError::Unauthenticated)?;
    let storage = storage::current().await.map_err(|_| UpdateError::Storage)?;
    let _guard = UpdateGuard::acquire().await;
    let mut settings = match storage
        .cached_user_settings(&user_id)
        .await
        .map_err(|_| UpdateError::Storage)?
    {
        Some(settings) => settings,
        None => fetch().await.ok().unwrap_or_default(),
    };
    change(&mut settings);
    settings.validate().map_err(UpdateError::Invalid)?;
    storage
        .save_user_settings(&user_id, &settings)
        .await
        .map_err(|_| UpdateError::Storage)?;
    apply_theme(settings);
    flush_in_background();
    Ok(settings)
}

/// 設定の更新を 1 つずつ適用する。端末内の最新値を読んでから書くため、
/// 複数の入力が重なっても先の変更を後の変更が上書きしない。
struct UpdateGuard;

impl UpdateGuard {
    async fn acquire() -> Self {
        poll_fn(|context| {
            if !UPDATING.with(Cell::get) {
                UPDATING.with(|updating| updating.set(true));
                return Poll::Ready(());
            }
            UPDATE_WAITERS.with(|waiters| {
                let mut waiters = waiters.borrow_mut();
                if !waiters.iter().any(|waker| waker.will_wake(context.waker())) {
                    waiters.push(context.waker().clone());
                }
            });
            Poll::Pending
        })
        .await;
        Self
    }
}

impl Drop for UpdateGuard {
    fn drop(&mut self) {
        UPDATING.with(|updating| updating.set(false));
        let waiters = UPDATE_WAITERS.with(|waiters| std::mem::take(&mut *waiters.borrow_mut()));
        for waker in waiters {
            waker.wake();
        }
    }
}

/// 送信待ちの設定を、呼び出し元を待たせずに送る。
pub fn flush_in_background() {
    wasm_bindgen_futures::spawn_local(async {
        flush().await;
    });
}

/// 送信待ちの設定を古い順に送る。
pub async fn flush() {
    let Some(user_id) = current_user_id().await else {
        return;
    };
    let Ok(storage) = storage::current().await else {
        return;
    };
    loop {
        let pending = match storage.pending_user_settings(&user_id).await {
            Ok(pending) => pending,
            Err(_) => return,
        };
        let Some(entry) = pending.first().cloned() else {
            return;
        };
        let status =
            match http::request_status_with_body("PUT", "/api/settings", &entry.settings).await {
                Ok(status) => status,
                Err(_) => return,
            };
        // 400 は送り直しても通らないため列から外す。401 と通信の失敗は次の機会に送る。
        if (200..300).contains(&status) || status == 400 {
            if storage
                .remove_pending_user_settings(&entry.operation_id)
                .await
                .is_err()
            {
                return;
            }
            continue;
        }
        return;
    }
}

fn apply_theme(settings: Settings) {
    theme::remember_display_mode(settings.display_mode);
    theme::apply_display_mode(settings.display_mode);
}

/// 設定の更新の誤りを、画面に出す文言へ対応させる。
pub fn update_error_key(error: UpdateError) -> crate::text::Key {
    use crate::text::Key;

    match error {
        UpdateError::Invalid(SettingsError::RestDefaultSecondsOutOfRange) => Key::RestTimerInvalid,
        UpdateError::Invalid(SettingsError::MacroTargetOutOfRange) => Key::MacroInvalid,
        UpdateError::Unauthenticated | UpdateError::Storage => Key::SettingsSaveFailed,
    }
}
