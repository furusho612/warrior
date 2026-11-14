//! 記録中のトレーニングの編集権の判定の性質テスト。
//!
//! 対応する要求: `REQ-NFR-015`。
//! 端末 A と B の書き込みの許可が、終了したかどうかと所有者だけで決まり、
//! 終了の後は両方の端末が書き込めることを確かめる。

use proptest::prelude::*;
use warrior_core::ownership::{decide_write, WriteDecision};

const TERMINAL_A: &str = "terminal-a";
const TERMINAL_B: &str = "terminal-b";

fn terminal() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just(TERMINAL_A), Just(TERMINAL_B)]
}

fn owner() -> impl Strategy<Value = Option<&'static str>> {
    prop_oneof![Just(None), Just(Some(TERMINAL_A)), Just(Some(TERMINAL_B))]
}

fn is_allowed(decision: WriteDecision) -> bool {
    matches!(decision, WriteDecision::Allow | WriteDecision::Claim)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn after_the_finish_every_terminal_may_write(
        owner in owner(),
        terminal in terminal(),
    ) {
        prop_assert!(is_allowed(decide_write(true, owner, terminal)));
    }

    #[test]
    fn a_recording_session_with_an_owner_admits_only_that_terminal(
        owner in prop_oneof![Just(TERMINAL_A), Just(TERMINAL_B)],
        terminal in terminal(),
    ) {
        let decision = decide_write(false, Some(owner), terminal);
        if owner == terminal {
            prop_assert_eq!(decision, WriteDecision::Allow);
        } else {
            prop_assert_eq!(decision, WriteDecision::Forbid);
        }
    }

    #[test]
    fn a_recording_session_without_an_owner_is_claimed_by_any_terminal(
        terminal in terminal(),
    ) {
        prop_assert_eq!(
            decide_write(false, None, terminal),
            WriteDecision::Claim
        );
    }

    #[test]
    fn at_most_one_terminal_may_write_while_recording(
        owner in prop_oneof![Just(TERMINAL_A), Just(TERMINAL_B)],
    ) {
        let allowed = [TERMINAL_A, TERMINAL_B]
            .into_iter()
            .filter(|terminal| is_allowed(decide_write(false, Some(owner), terminal)))
            .collect::<Vec<_>>();
        prop_assert_eq!(allowed, vec![owner]);
    }
}
