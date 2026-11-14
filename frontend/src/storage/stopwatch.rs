//! 時間で記録するセットのストップウォッチの開始時刻 (REQ-SET-010)。
//!
//! 開始時刻を `meta` ストアに保存する。再読み込みの後は保存した開始時刻と現在時刻の
//! 差から経過時間を求める。左右別の種目は、左右それぞれの開始時刻を持つ。

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use web_sys::{IdbObjectStore, IdbTransactionMode};

use super::indexeddb::{self, from_js, get, to_js};
use super::{abort, Storage, StorageError, Store};

/// ストップウォッチを計る対象の側。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopwatchSide {
    /// 左右の区別がない種目。
    Whole,
    /// 左。
    Left,
    /// 右。
    Right,
}

impl StopwatchSide {
    fn suffix(self) -> &'static str {
        match self {
            Self::Whole => "",
            Self::Left => ":left",
            Self::Right => ":right",
        }
    }
}

/// ストップウォッチの開始時刻を持つメタの 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StartedAtRecord {
    key: String,
    started_at_ms: i64,
}

/// セットと側の開始時刻を持つメタのキー。
pub(super) fn key(set_id: &str, side: StopwatchSide) -> String {
    format!("stopwatch_started_at:{set_id}{}", side.suffix())
}

/// 開始時刻を保存する。
pub(super) async fn save(
    meta: &IdbObjectStore,
    set_id: &str,
    side: StopwatchSide,
    started_at_ms: i64,
) -> Result<(), StorageError> {
    let record = StartedAtRecord {
        key: key(set_id, side),
        started_at_ms,
    };
    indexeddb::put(meta, &to_js(&record)?).await
}

/// 保存した開始時刻を返す。無ければ None。
pub(super) async fn load(
    meta: &IdbObjectStore,
    set_id: &str,
    side: StopwatchSide,
) -> Result<Option<i64>, StorageError> {
    match get(meta, &JsValue::from_str(&key(set_id, side))).await? {
        Some(value) => Ok(Some(from_js::<StartedAtRecord>(&value)?.started_at_ms)),
        None => Ok(None),
    }
}

/// 保存した開始時刻を消す。
pub(super) async fn clear(
    meta: &IdbObjectStore,
    set_id: &str,
    side: StopwatchSide,
) -> Result<(), StorageError> {
    indexeddb::delete_entry(meta, &JsValue::from_str(&key(set_id, side))).await
}

impl Storage {
    /// ストップウォッチの開始時刻を端末内へ保存する。
    pub async fn save_stopwatch_started_at(
        &self,
        set_id: &str,
        side: StopwatchSide,
        started_at_ms: i64,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        match save(&meta, set_id, side, started_at_ms).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }

    /// 保存したストップウォッチの開始時刻を返す。無ければ None。
    pub async fn stopwatch_started_at(
        &self,
        set_id: &str,
        side: StopwatchSide,
    ) -> Result<Option<i64>, StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            IdbTransactionMode::Readonly,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        let started_at = load(&meta, set_id, side).await?;
        transaction.finish().await?;
        Ok(started_at)
    }

    /// 保存したストップウォッチの開始時刻を消す。
    pub async fn clear_stopwatch_started_at(
        &self,
        set_id: &str,
        side: StopwatchSide,
    ) -> Result<(), StorageError> {
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::Meta.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let meta = transaction.store(Store::Meta.name())?;
        match clear(&meta, set_id, side).await {
            Ok(()) => transaction.finish().await,
            Err(error) => abort(transaction, error).await,
        }
    }
}
