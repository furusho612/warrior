//! セットの完了と、過去の記録の変更の後の自己ベストの判定。

use warrior_core::personal_best::BestGroup;
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

/// 操作の適用の後に、影響する組の自己ベストを計算し直す。
///
/// 完了したセットを編集または削除したとき、種目またはトレーニングを削除したとき、
/// 完了を取り消したときは、値が下がるか、行そのものが無くなる。残っている全ての記録から
/// 計算し直す。完了の操作は、この判定の後で行う増加の判定に任せる。
pub(crate) async fn personal_bests_after_operation(
    db: &Db,
    user_id: &str,
    validated: &ValidatedOperation,
    rebuild_groups: &[BestGroup],
) -> CompletionBests {
    if let ValidatedPayload::SetComplete(payload) = &validated.payload {
        if payload.completed {
            return personal_bests_after_completion(db, user_id, validated).await;
        }
    }
    if rebuild_groups.is_empty() {
        return CompletionBests::default();
    }
    match crate::personal_best::rebuild(db, user_id, rebuild_groups).await {
        Ok(_) => CompletionBests::default(),
        Err(error) => {
            // 変更は保存済みである。次に影響する操作が同じ組を計算し直す。
            console_error!("failed to rebuild the personal bests: {error}");
            CompletionBests::default()
        }
    }
}
