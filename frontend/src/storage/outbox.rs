//! 送信待ちの列の読み書きと、次の順序の決定。

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use web_sys::IdbObjectStore;

use warrior_core::sync::OutboxEntry;

use super::indexeddb::{self, from_js, get, to_js};
use super::StorageError;

/// トレーニングごとの最後の順序を持つメタのキーの接頭辞。
pub const SEQUENCE_KEY_PREFIX: &str = "outbox_sequence:";

/// メタの 1 行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MetaRecord {
    key: String,
    value: u64,
}

/// トレーニングの最後の順序を持つメタのキー。
pub fn sequence_key(training_id: &str) -> String {
    format!("{SEQUENCE_KEY_PREFIX}{training_id}")
}

/// 送信待ちの列を順序の昇順に並べる。
pub fn sort(entries: &mut [OutboxEntry]) {
    entries.sort_by(|left, right| {
        left.operation
            .sequence
            .cmp(&right.operation.sequence)
            .then_with(|| {
                left.operation
                    .operation_id
                    .cmp(&right.operation.operation_id)
            })
    });
}

/// トレーニングの次の順序を決め、メタを同じトランザクションで更新する。
pub async fn next_sequence(meta: &IdbObjectStore, training_id: &str) -> Result<u64, StorageError> {
    let key = sequence_key(training_id);
    let current = match get(meta, &JsValue::from_str(&key)).await? {
        Some(value) => from_js::<MetaRecord>(&value)?.value,
        None => 0,
    };
    let next = current
        .checked_add(1)
        .ok_or_else(|| StorageError::InvalidOperation("the sequence is exhausted".to_owned()))?;
    let record = MetaRecord { key, value: next };
    indexeddb::put(meta, &to_js(&record)?).await?;
    Ok(next)
}

/// ストアから送信待ちの列を読み、順序の昇順に並べる。
pub async fn read(store: &IdbObjectStore) -> Result<Vec<OutboxEntry>, StorageError> {
    let mut entries = indexeddb::get_all(store)
        .await?
        .iter()
        .map(from_js::<OutboxEntry>)
        .collect::<Result<Vec<_>, _>>()?;
    sort(&mut entries);
    Ok(entries)
}
