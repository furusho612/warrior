//! セットの完了の応答で受け取った自己ベストの端末内への反映。
//!
//! 対応する要求: `REQ-SET-009`。
//! 同じ比較の単位の項目は応答の値で置き換える。同じトレーニングで同じ種類を複数回
//! 更新した場合、最後に受け取った応答の値が残る。

use warrior_core::sync::SessionRecord;
use warrior_core::training_session::PersonalBestItem;
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

use super::{abort, indexeddb, Storage, StorageError, Store};

impl Storage {
    /// セットの完了の応答で受け取った自己ベストを、記録中のトレーニングへ反映する。
    pub async fn merge_personal_bests(
        &self,
        training_id: &str,
        items: &[PersonalBestItem],
    ) -> Result<(), StorageError> {
        if items.is_empty() {
            return Ok(());
        }
        let database = self.database().await?;
        let transaction = indexeddb::Transaction::new(
            &database,
            &[Store::TrainingSessions.name()],
            IdbTransactionMode::Readwrite,
        )?;
        let sessions = transaction.store(Store::TrainingSessions.name())?;
        let Some(value) = indexeddb::get(&sessions, &JsValue::from_str(training_id)).await? else {
            transaction.finish().await?;
            return Ok(());
        };
        let mut session: SessionRecord = indexeddb::from_js(&value)?;
        for item in items {
            match session
                .personal_bests
                .iter()
                .position(|saved| same_personal_best_key(saved, item))
            {
                Some(index) => session.personal_bests[index] = item.clone(),
                None => session.personal_bests.push(item.clone()),
            }
        }
        if let Err(error) = indexeddb::put(&sessions, &indexeddb::to_js(&session)?).await {
            return abort(transaction, error).await;
        }
        transaction.finish().await
    }
}

fn same_personal_best_key(left: &PersonalBestItem, right: &PersonalBestItem) -> bool {
    left.exercise_id == right.exercise_id
        && left.grip == right.grip
        && left.attachment == right.attachment
        && left.kind == right.kind
}
