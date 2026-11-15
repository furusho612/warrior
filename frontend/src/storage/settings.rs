//! 利用者共通の設定の端末内の保存と送信待ちの列、端末ごとの設定 (REQ-MYP-003、REQ-MYP-005)。
//!
//! 利用者共通の設定は利用者 ID ごとに `meta` ストアへ保存し、変更を送信待ちの列へ積む。
//! 端末ごとの設定 (画面の消灯防止、ブラウザ通知、プッシュ通知) は同じ `meta` ストアへ
//! 保存するが、送信待ちの列には積まない。
//! 表示モードの初期表示用の保存は `crate::theme` が localStorage に持つ。

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use web_sys::IdbObjectStore;

use warrior_core::settings::Settings;

use super::rest_timer::{delete_meta, load_meta, save_meta};
use super::{abort, indexeddb, Storage, StorageError, Store};

/// 利用者共通の設定のキャッシュのキーの接頭辞。
const USER_SETTINGS_KEY_PREFIX: &str = "user_settings:";
/// 送信待ちの設定の列のキー。
const USER_SETTINGS_QUEUE_KEY: &str = "user_settings_queue";
/// 端末内で設定を読む対象の利用者のキー。
const SETTINGS_OWNER_KEY: &str = "settings_owner";
/// 利用者 ID をまだ知らない端末内のテストが使う利用者。
const LOCAL_OWNER: &str = "local";
/// 画面の消灯防止のキー。
const WAKE_LOCK_KEY: &str = "wake_lock_enabled";
/// ブラウザ通知のキー。
const BROWSER_NOTIFICATION_KEY: &str = "browser_notification_enabled";
/// プッシュ通知のキー。
const PUSH_NOTIFICATION_KEY: &str = "push_notification_enabled";
/// 画面の消灯防止の初期値。
const DEFAULT_WAKE_LOCK_ENABLED: bool = true;

/// 利用者共通の設定のキャッシュの 1 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UserSettingsRecord {
    key: String,
    user_id: String,
    settings: Settings,
}

/// 送信待ちの設定の 1 件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingUserSettings {
    /// 端末が作る操作の UUID v4。
    pub operation_id: String,
    /// 設定の利用者。
    pub user_id: String,
    /// 送る設定。
    pub settings: Settings,
}

/// 送信待ちの設定の列の 1 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UserSettingsQueueRecord {
    key: String,
    entries: Vec<PendingUserSettings>,
}

/// 端末ごとの有効かどうかの設定の 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EnabledRecord {
    key: String,
    enabled: bool,
}

/// 端末内で設定を読む対象の利用者の 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OwnerRecord {
    key: String,
    user_id: String,
}

fn user_settings_key(user_id: &str) -> String {
    format!("{USER_SETTINGS_KEY_PREFIX}{user_id}")
}

async fn read_queue(storage: &Storage) -> Result<Vec<PendingUserSettings>, StorageError> {
    Ok(
        load_meta::<UserSettingsQueueRecord>(storage, USER_SETTINGS_QUEUE_KEY)
            .await?
            .map_or_else(Vec::new, |record| record.entries),
    )
}

async fn read_queue_from_store(
    store: &IdbObjectStore,
) -> Result<Vec<PendingUserSettings>, StorageError> {
    let value = indexeddb::get(store, &JsValue::from_str(USER_SETTINGS_QUEUE_KEY)).await?;
    Ok(value
        .as_ref()
        .map(indexeddb::from_js::<UserSettingsQueueRecord>)
        .transpose()?
        .map_or_else(Vec::new, |record| record.entries))
}

async fn save_device_setting(
    storage: &Storage,
    key: &str,
    enabled: bool,
) -> Result<(), StorageError> {
    save_meta(
        storage,
        &EnabledRecord {
            key: key.to_owned(),
            enabled,
        },
    )
    .await
}

async fn device_setting(
    storage: &Storage,
    key: &str,
    default_value: bool,
) -> Result<bool, StorageError> {
    Ok(load_meta::<EnabledRecord>(storage, key)
        .await?
        .map_or(default_value, |record| record.enabled))
}

impl Storage {
    /// 端末内で利用者共通の設定を読む対象の利用者を記録する。
    pub async fn set_settings_owner(&self, user_id: &str) -> Result<(), StorageError> {
        save_meta(
            self,
            &OwnerRecord {
                key: SETTINGS_OWNER_KEY.to_owned(),
                user_id: user_id.to_owned(),
            },
        )
        .await
    }

    /// 端末内で利用者共通の設定を読む対象の利用者を返す。
    pub async fn settings_owner(&self) -> Result<Option<String>, StorageError> {
        Ok(load_meta::<OwnerRecord>(self, SETTINGS_OWNER_KEY)
            .await?
            .map(|record| record.user_id))
    }

    /// 端末内で利用者共通の設定を読む対象の利用者を消す。
    pub async fn clear_settings_owner(&self) -> Result<(), StorageError> {
        delete_meta(self, SETTINGS_OWNER_KEY).await
    }

    async fn active_settings_owner(&self) -> Result<String, StorageError> {
        Ok(self
            .settings_owner()
            .await?
            .unwrap_or_else(|| LOCAL_OWNER.to_owned()))
    }

    /// 利用者共通の設定を端末内へ保存し、送信待ちの列へ積む。
    pub async fn save_user_settings(
        &self,
        user_id: &str,
        settings: &Settings,
    ) -> Result<(), StorageError> {
        settings
            .validate()
            .map_err(|error| StorageError::InvalidOperation(error.to_string()))?;
        let operation_id = crate::ids::new_uuid()?;
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            web_sys::IdbTransactionMode::Readwrite,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        let record = UserSettingsRecord {
            key: user_settings_key(user_id),
            user_id: user_id.to_owned(),
            settings: *settings,
        };
        if let Err(error) = indexeddb::put(&meta, &indexeddb::to_js(&record)?).await {
            return abort(transaction, error).await;
        }
        let mut entries = match read_queue_from_store(&meta).await {
            Ok(entries) => entries,
            Err(error) => return abort(transaction, error).await,
        };
        entries.push(PendingUserSettings {
            operation_id,
            user_id: user_id.to_owned(),
            settings: *settings,
        });
        let queue = UserSettingsQueueRecord {
            key: USER_SETTINGS_QUEUE_KEY.to_owned(),
            entries,
        };
        if let Err(error) = indexeddb::put(&meta, &indexeddb::to_js(&queue)?).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }

    /// 利用者共通の設定を送信待ちの列へ積まずに端末内へ保存する。
    pub async fn cache_user_settings(
        &self,
        user_id: &str,
        settings: &Settings,
    ) -> Result<(), StorageError> {
        settings
            .validate()
            .map_err(|error| StorageError::InvalidOperation(error.to_string()))?;
        save_meta(
            self,
            &UserSettingsRecord {
                key: user_settings_key(user_id),
                user_id: user_id.to_owned(),
                settings: *settings,
            },
        )
        .await
    }

    /// 端末内に保存した利用者共通の設定を返す。
    pub async fn cached_user_settings(
        &self,
        user_id: &str,
    ) -> Result<Option<Settings>, StorageError> {
        Ok(
            load_meta::<UserSettingsRecord>(self, &user_settings_key(user_id))
                .await?
                .map(|record| record.settings),
        )
    }

    /// 端末内で設定を読む対象の利用者のキャッシュを返す。
    pub async fn cached_user_settings_for_active_owner(
        &self,
    ) -> Result<Option<Settings>, StorageError> {
        let owner = self.active_settings_owner().await?;
        self.cached_user_settings(&owner).await
    }

    /// 端末内で設定を読む対象の利用者として、利用者共通の設定を保存する。
    pub async fn save_user_settings_for_active_owner(
        &self,
        settings: &Settings,
    ) -> Result<(), StorageError> {
        let owner = self.active_settings_owner().await?;
        self.save_user_settings(&owner, settings).await
    }

    /// 利用者の送信待ちの設定を古い順に返す。
    pub async fn pending_user_settings(
        &self,
        user_id: &str,
    ) -> Result<Vec<PendingUserSettings>, StorageError> {
        Ok(read_queue(self)
            .await?
            .into_iter()
            .filter(|entry| entry.user_id == user_id)
            .collect())
    }

    /// 送信した設定を送信待ちの列から除く。
    pub async fn remove_pending_user_settings(
        &self,
        operation_id: &str,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            web_sys::IdbTransactionMode::Readwrite,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        let mut entries = match read_queue_from_store(&meta).await {
            Ok(entries) => entries,
            Err(error) => return abort(transaction, error).await,
        };
        entries.retain(|entry| entry.operation_id != operation_id);
        let result = if entries.is_empty() {
            indexeddb::delete_entry(&meta, &JsValue::from_str(USER_SETTINGS_QUEUE_KEY)).await
        } else {
            let queue = UserSettingsQueueRecord {
                key: USER_SETTINGS_QUEUE_KEY.to_owned(),
                entries,
            };
            indexeddb::put(&meta, &indexeddb::to_js(&queue)?).await
        };
        match result {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 画面の消灯防止を有効にするかを保存する。
    pub async fn save_wake_lock_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        save_device_setting(self, WAKE_LOCK_KEY, enabled).await
    }

    /// 画面の消灯防止を有効にするかを返す。保存が無いときは有効とする。
    pub async fn wake_lock_enabled(&self) -> Result<bool, StorageError> {
        device_setting(self, WAKE_LOCK_KEY, DEFAULT_WAKE_LOCK_ENABLED).await
    }

    /// ブラウザ通知を有効にするかを保存する。
    pub async fn save_browser_notification_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), StorageError> {
        save_device_setting(self, BROWSER_NOTIFICATION_KEY, enabled).await
    }

    /// ブラウザ通知を有効にするかを返す。保存が無いときは無効とする。
    pub async fn browser_notification_enabled(&self) -> Result<bool, StorageError> {
        device_setting(self, BROWSER_NOTIFICATION_KEY, false).await
    }

    /// ブラウザ通知を有効にするかを保存する。既存の呼び名を保つ。
    pub async fn save_notification_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        self.save_browser_notification_enabled(enabled).await
    }

    /// ブラウザ通知を有効にするかを返す。既存の呼び名を保つ。
    pub async fn notification_enabled(&self) -> Result<bool, StorageError> {
        self.browser_notification_enabled().await
    }

    /// プッシュ通知を有効にするかを保存する。
    pub async fn save_push_notification_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        save_device_setting(self, PUSH_NOTIFICATION_KEY, enabled).await
    }

    /// プッシュ通知を有効にするかを返す。保存が無いときは無効とする。
    pub async fn push_notification_enabled(&self) -> Result<bool, StorageError> {
        device_setting(self, PUSH_NOTIFICATION_KEY, false).await
    }
}
