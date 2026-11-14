//! 身体情報の体重のキャッシュ。同期のメタとは別のストアに置く。

use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use web_sys::IdbObjectStore;

use super::indexeddb::{self, from_js, get, to_js};
use super::StorageError;

/// キャッシュの行のキー。
const BODY_WEIGHT_KEY: &str = "profile_body_weight";

/// 身体情報の体重の 1 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BodyWeightRecord {
    key: String,
    body_weight_kg: f64,
}

/// 身体情報の体重をキャッシュする。None のときはキャッシュを消す。
pub async fn save(store: &IdbObjectStore, body_weight_kg: Option<f64>) -> Result<(), StorageError> {
    match body_weight_kg {
        Some(body_weight_kg) => {
            let record = BodyWeightRecord {
                key: BODY_WEIGHT_KEY.to_owned(),
                body_weight_kg,
            };
            indexeddb::put(store, &to_js(&record)?).await
        }
        None => indexeddb::delete_entry(store, &JsValue::from_str(BODY_WEIGHT_KEY)).await,
    }
}

/// キャッシュした身体情報の体重を返す。
pub async fn cached(store: &IdbObjectStore) -> Result<Option<f64>, StorageError> {
    match get(store, &JsValue::from_str(BODY_WEIGHT_KEY)).await? {
        Some(value) => Ok(Some(from_js::<BodyWeightRecord>(&value)?.body_weight_kg)),
        None => Ok(None),
    }
}
