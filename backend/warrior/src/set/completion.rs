//! セットの完了の後の自己ベストの判定。

use warrior_core::sync::{ValidatedOperation, ValidatedPayload};
use worker::console_error;

use crate::db::Db;
use crate::personal_best::CompletionBests;

/// セットの完了の適用の後に、自己ベストを判定して返す。
///
/// セットの完了以外の操作と、完了を取り消す操作は、自己ベストを変えない。
pub(crate) async fn personal_bests_after_completion(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
) -> CompletionBests {
    let ValidatedPayload::SetComplete(payload) = &validated.payload else {
        return CompletionBests::default();
    };
    if !payload.completed {
        return CompletionBests::default();
    }
    match crate::personal_best::apply_completion(db, user_id, &validated.operation.training_id)
        .await
    {
        Ok(bests) => bests,
        Err(error) => {
            // セットの完了は保存済みである。サマリーの終了時に同じ判定をやり直す。
            console_error!("failed to update the personal bests: {error}");
            CompletionBests::default()
        }
    }
}
