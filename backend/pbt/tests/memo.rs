//! メモの保存と読み出しの性質テスト。
//!
//! 対応する要求: `REQ-REC-007`、`REQ-NFR-019`。
//! 任意の文字列のメモが送信待ちの本文の往復で変わらないことと、2000 文字の境界で
//! 受理と拒否が分かれることを確かめる。

use proptest::prelude::*;
use warrior_core::memo::{validate, MemoError, MEMO_MAX_CHARS};
use warrior_core::sync::{SessionMemoPayload, TrainingExerciseMemoPayload};

const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
const TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";

fn memo_text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..=MEMO_MAX_CHARS)
        .prop_map(|characters| characters.into_iter().collect())
}

fn memo_at_the_limit() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), MEMO_MAX_CHARS..=MEMO_MAX_CHARS)
        .prop_map(|characters| characters.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_memo_at_or_below_the_limit_round_trips(memo in memo_text()) {
        prop_assert_eq!(validate(&memo), Ok(()));

        let session = SessionMemoPayload {
            training_id: TRAINING_ID.to_owned(),
            memo: memo.clone(),
        };
        let session_json = serde_json::to_string(&session).expect("the session memo is JSON");
        let session_restored: SessionMemoPayload =
            serde_json::from_str(&session_json).expect("the session memo round trips");
        prop_assert_eq!(session_restored, session);

        let exercise = TrainingExerciseMemoPayload {
            training_id: TRAINING_ID.to_owned(),
            training_exercise_id: TRAINING_EXERCISE_ID.to_owned(),
            memo: memo.clone(),
        };
        let exercise_json = serde_json::to_string(&exercise).expect("the exercise memo is JSON");
        let exercise_restored: TrainingExerciseMemoPayload =
            serde_json::from_str(&exercise_json).expect("the exercise memo round trips");
        prop_assert_eq!(exercise_restored, exercise);
    }

    #[test]
    fn the_limit_accepts_2000_characters_and_rejects_2001(memo in memo_at_the_limit()) {
        prop_assert_eq!(memo.chars().count(), MEMO_MAX_CHARS);
        prop_assert_eq!(validate(&memo), Ok(()));

        let over_the_limit = format!("{memo}x");
        prop_assert_eq!(validate(&over_the_limit), Err(MemoError::TooLong));
    }
}
