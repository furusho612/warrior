//! AI 生成ジョブの状態の性質テスト。
//!
//! 対応する要求: `REQ-AI-014`、`REQ-AI-018`。
//! 同じ生成の ID のメッセージを何度処理しても、生成または失敗の状態に収束することを
//! 確かめる。再試行の上限と、終端の状態が変わらないことも固定する。

use proptest::prelude::*;
use warrior_core::ai_job::{advance, AttemptOutcome, JobState, JobStatus, MAX_ATTEMPTS};

fn outcome() -> impl Strategy<Value = AttemptOutcome> {
    prop_oneof![
        Just(AttemptOutcome::Succeeded),
        Just(AttemptOutcome::TemporaryFailure),
        Just(AttemptOutcome::PermanentFailure),
    ]
}

proptest! {
    #[test]
    fn processing_the_same_job_reaches_generated_or_failed(
        outcomes in prop::collection::vec(outcome(), MAX_ATTEMPTS as usize..20)
    ) {
        let mut state = JobState::new(JobStatus::Enqueued, 0);
        for outcome in outcomes {
            state = advance(state, outcome).state();
            prop_assert!(
                state.attempts <= MAX_ATTEMPTS,
                "the attempts must stay within the limit"
            );
        }
        prop_assert!(
            matches!(state.status, JobStatus::Generated | JobStatus::Failed),
            "the final state must be generated or failed"
        );
    }

    #[test]
    fn terminal_states_do_not_change(
        outcomes in prop::collection::vec(outcome(), 0..20),
        terminal in prop_oneof![
            Just(JobStatus::Generated),
            Just(JobStatus::Failed),
        ],
        attempts in 0u32..=MAX_ATTEMPTS
    ) {
        let terminal_state = JobState::new(terminal, attempts);
        let mut state = terminal_state;
        for outcome in outcomes {
            state = advance(state, outcome).state();
            prop_assert_eq!(state, terminal_state);
        }
        prop_assert_eq!(state, terminal_state);
    }
}
