//! レストタイマーの終了予定時刻と、種目ごととセットごとのレスト時間の端末内の保存 (REQ-TMR-001、REQ-TMR-006)。
//!
//! 終了予定時刻とレスト時間は `meta` ストアに保存する。再読み込みの後は、保存した
//! 終了予定時刻からタイマーを続ける。レスト時間は全体の既定値、種目ごとの値、
//! セットごとの値を持ち、セットの値が種目の値より優先する。
//! 全体の既定値は利用者共通の設定として `storage::settings` が持ち、ここでは
//! 種目ごととセットごとの値だけを扱う。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

use warrior_core::rest_timer::is_valid_seconds;

use crate::rest_push::RestReservation;

use super::{abort, indexeddb, Storage, StorageError, Store};

/// 秒数を保存するメタの 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SecondsRecord {
    key: String,
    seconds: u32,
}

/// 終了予定時刻を保存するメタの 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct EndRecord {
    key: String,
    end_ms: i64,
}

/// プッシュ通知の予約を保存するメタの 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReservationRecord {
    key: String,
    reservation: RestReservation,
}

fn timer_key(training_id: &str) -> String {
    format!("rest_timer:{training_id}")
}

fn reservation_key(training_id: &str) -> String {
    format!("rest_reservation:{training_id}")
}

fn exercise_seconds_key(training_exercise_id: &str) -> String {
    format!("rest_seconds:exercise:{training_exercise_id}")
}

fn set_seconds_key(set_id: &str) -> String {
    format!("rest_seconds:set:{set_id}")
}

pub(super) async fn load_meta<T: DeserializeOwned>(
    storage: &Storage,
    key: &str,
) -> Result<Option<T>, StorageError> {
    let database = storage.database().await?;
    let transaction = indexeddb::Transaction::new(
        &database,
        &[Store::Meta.name()],
        web_sys::IdbTransactionMode::Readonly,
    )?;
    let meta = transaction.store(Store::Meta.name())?;
    let value = indexeddb::get(&meta, &JsValue::from_str(key)).await?;
    let record = value.as_ref().map(indexeddb::from_js::<T>).transpose()?;
    transaction.finish().await?;
    Ok(record)
}

pub(super) async fn save_meta<T: Serialize>(
    storage: &Storage,
    value: &T,
) -> Result<(), StorageError> {
    let database = storage.database().await?;
    let transaction = indexeddb::Transaction::new(
        &database,
        &[Store::Meta.name()],
        web_sys::IdbTransactionMode::Readwrite,
    )?;
    let meta = transaction.store(Store::Meta.name())?;
    match indexeddb::put(&meta, &indexeddb::to_js(value)?).await {
        Ok(()) => transaction.finish().await,
        Err(error) => abort(transaction, error).await,
    }
}

pub(super) async fn delete_meta(storage: &Storage, key: &str) -> Result<(), StorageError> {
    let database = storage.database().await?;
    let transaction = indexeddb::Transaction::new(
        &database,
        &[Store::Meta.name()],
        web_sys::IdbTransactionMode::Readwrite,
    )?;
    let meta = transaction.store(Store::Meta.name())?;
    match indexeddb::delete_entry(&meta, &JsValue::from_str(key)).await {
        Ok(()) => transaction.finish().await,
        Err(error) => abort(transaction, error).await,
    }
}

async fn save_seconds(
    storage: &Storage,
    key: String,
    seconds: Option<u32>,
) -> Result<(), StorageError> {
    match seconds {
        Some(seconds) if !is_valid_seconds(seconds) => Err(StorageError::InvalidOperation(
            "the rest seconds are out of range".to_owned(),
        )),
        Some(seconds) => save_meta(storage, &SecondsRecord { key, seconds }).await,
        None => delete_meta(storage, &key).await,
    }
}

async fn load_seconds(storage: &Storage, key: &str) -> Result<Option<u32>, StorageError> {
    Ok(load_meta::<SecondsRecord>(storage, key)
        .await?
        .map(|record| record.seconds)
        .filter(|seconds| is_valid_seconds(*seconds)))
}

impl Storage {
    /// 終了予定時刻を端末内へ保存する。
    pub async fn save_rest_timer(
        &self,
        training_id: &str,
        end_ms: i64,
    ) -> Result<(), StorageError> {
        save_meta(
            self,
            &EndRecord {
                key: timer_key(training_id),
                end_ms,
            },
        )
        .await
    }

    /// 保存した終了予定時刻を返す。無ければ None。
    pub async fn rest_timer_end(&self, training_id: &str) -> Result<Option<i64>, StorageError> {
        Ok(load_meta::<EndRecord>(self, &timer_key(training_id))
            .await?
            .map(|record| record.end_ms))
    }

    /// 保存した終了予定時刻を消す。
    pub async fn clear_rest_timer(&self, training_id: &str) -> Result<(), StorageError> {
        delete_meta(self, &timer_key(training_id)).await?;
        delete_meta(self, &reservation_key(training_id)).await
    }

    /// プッシュ通知の予約を端末内へ保存する。
    pub async fn save_rest_reservation(
        &self,
        training_id: &str,
        reservation: &RestReservation,
    ) -> Result<(), StorageError> {
        save_meta(
            self,
            &ReservationRecord {
                key: reservation_key(training_id),
                reservation: reservation.clone(),
            },
        )
        .await
    }

    /// 保存したプッシュ通知の予約を返す。無ければ None。
    pub async fn rest_reservation(
        &self,
        training_id: &str,
    ) -> Result<Option<RestReservation>, StorageError> {
        Ok(
            load_meta::<ReservationRecord>(self, &reservation_key(training_id))
                .await?
                .map(|record| record.reservation),
        )
    }

    /// レスト時間の既定値を保存する。None のときは初期値に戻す。
    pub async fn save_rest_default_seconds(
        &self,
        seconds: Option<u32>,
    ) -> Result<(), StorageError> {
        let settings = self
            .cached_user_settings_for_active_owner()
            .await?
            .unwrap_or_default();
        let settings = warrior_core::settings::Settings {
            rest_default_seconds: seconds.unwrap_or(warrior_core::rest_timer::DEFAULT_SECONDS),
            ..settings
        };
        self.save_user_settings_for_active_owner(&settings).await
    }

    /// レスト時間の既定値を返す。保存が無いときは 90 秒を返す。
    pub async fn rest_default_seconds(&self) -> Result<u32, StorageError> {
        Ok(self
            .cached_user_settings_for_active_owner()
            .await?
            .map_or(warrior_core::rest_timer::DEFAULT_SECONDS, |settings| {
                settings.rest_default_seconds
            }))
    }

    /// 種目ごとのレスト時間を保存する。None のときは指定を消す。
    pub async fn save_rest_exercise_seconds(
        &self,
        training_exercise_id: &str,
        seconds: Option<u32>,
    ) -> Result<(), StorageError> {
        save_seconds(self, exercise_seconds_key(training_exercise_id), seconds).await
    }

    /// 種目ごとのレスト時間を返す。指定が無ければ None。
    pub async fn rest_exercise_seconds(
        &self,
        training_exercise_id: &str,
    ) -> Result<Option<u32>, StorageError> {
        load_seconds(self, &exercise_seconds_key(training_exercise_id)).await
    }

    /// セットごとのレスト時間を保存する。None のときは指定を消す。
    pub async fn save_rest_set_seconds(
        &self,
        set_id: &str,
        seconds: Option<u32>,
    ) -> Result<(), StorageError> {
        save_seconds(self, set_seconds_key(set_id), seconds).await
    }

    /// セットごとのレスト時間を返す。指定が無ければ None。
    pub async fn rest_set_seconds(&self, set_id: &str) -> Result<Option<u32>, StorageError> {
        load_seconds(self, &set_seconds_key(set_id)).await
    }
}
