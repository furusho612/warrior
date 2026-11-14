//! 記録中のトレーニングの編集権の判定。
//!
//! 書き込みを許可するかは、終了したかどうかと、所有者の端末トークンの ID だけで決まる (ADR-003)。
//! 判定を純粋な関数に置き、D1 の読み書きは `backend/warrior/src/ownership.rs` が担う。

/// 記録中のトレーニングへの書き込みの判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteDecision {
    /// 書き込める。所有者が未設定なので、この端末を所有者にする。
    Claim,
    /// 書き込める。所有者は変わらない。
    Allow,
    /// 書き込めない。ほかの端末が所有者である。
    Forbid,
}

/// 端末からの書き込みを許可するかを決める。
///
/// 終了したトレーニングはどの端末からでも書き込め、後から受理した内容が残る (REQ-NFR-015)。
/// 記録中は所有者だけが書き込み、所有者の無い既存の行は書き込んだ端末が所有者になる。
pub fn decide_write(
    finished: bool,
    owner_terminal_id: Option<&str>,
    terminal_id: &str,
) -> WriteDecision {
    if finished {
        return WriteDecision::Allow;
    }
    match owner_terminal_id {
        None => WriteDecision::Claim,
        Some(owner) if owner == terminal_id => WriteDecision::Allow,
        Some(_) => WriteDecision::Forbid,
    }
}

#[cfg(test)]
mod tests {
    use super::{decide_write, WriteDecision};

    #[test]
    fn the_starting_terminal_keeps_the_editing_right() {
        assert_eq!(
            decide_write(false, Some("terminal-a"), "terminal-a"),
            WriteDecision::Allow
        );
        assert_eq!(
            decide_write(false, Some("terminal-a"), "terminal-b"),
            WriteDecision::Forbid
        );
    }

    #[test]
    fn a_recording_session_without_an_owner_is_claimed_by_the_writer() {
        assert_eq!(
            decide_write(false, None, "terminal-b"),
            WriteDecision::Claim
        );
    }

    #[test]
    fn every_terminal_may_write_after_the_finish() {
        assert_eq!(decide_write(true, None, "terminal-a"), WriteDecision::Allow);
        assert_eq!(decide_write(true, None, "terminal-b"), WriteDecision::Allow);
        assert_eq!(
            decide_write(true, Some("terminal-a"), "terminal-b"),
            WriteDecision::Allow
        );
    }
}
