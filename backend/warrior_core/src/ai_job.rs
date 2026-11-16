//! AI 生成ジョブの状態、再試行、lease、一意キー (ADR-021)。
//!
//! 状態は `pending`、`enqueued`、`generated`、`failed` の 4 つである。
//! 一時的な失敗は `pending` に戻して Queue の `delaySeconds` で再投入し、
//! 再試行の上限を超えたら `failed` にする。恒久的な失敗は再試行しない。
//! D1 への書き込みは Worker が担い、このモジュールは判断だけを持つ。

use serde::{Deserialize, Serialize};

/// 生成の試行回数の上限。初回を含めて 4 回とする。
pub const MAX_ATTEMPTS: u32 = 4;

/// `enqueued` の lease の長さ (秒)。
pub const LEASE_SECONDS: i64 = 60;

/// 一時的な失敗の後の再試行の間隔 (秒)。
pub const RETRY_DELAYS_SECONDS: [u32; 3] = [1, 5, 30];

/// 生成の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// 日の評価。
    DailyEvaluation,
    /// 次回セットの提案。
    SetProposal,
    /// 週の評価。
    WeeklyEvaluation,
    /// 月の評価。
    MonthlyEvaluation,
}

impl JobKind {
    /// 4 種類。
    pub const ALL: [Self; 4] = [
        Self::DailyEvaluation,
        Self::SetProposal,
        Self::WeeklyEvaluation,
        Self::MonthlyEvaluation,
    ];

    /// D1 と API に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DailyEvaluation => "daily_evaluation",
            Self::SetProposal => "set_proposal",
            Self::WeeklyEvaluation => "weekly_evaluation",
            Self::MonthlyEvaluation => "monthly_evaluation",
        }
    }

    /// D1 の値を種別に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }
}

/// 生成の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// 投入を待つ。
    Pending,
    /// Queue に投入し、消費を待つ。
    Enqueued,
    /// 生成に成功した。
    Generated,
    /// 生成に失敗した。
    Failed,
}

impl JobStatus {
    /// D1 と API に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Enqueued => "enqueued",
            Self::Generated => "generated",
            Self::Failed => "failed",
        }
    }

    /// D1 の値を状態に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "enqueued" => Some(Self::Enqueued),
            "generated" => Some(Self::Generated),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// これ以上処理しない状態か。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Generated | Self::Failed)
    }
}

/// 失敗の分類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// 再試行する失敗。
    Temporary,
    /// 再試行しない失敗。
    Permanent,
}

/// 失敗を分類する。
///
/// 再試行するのは、タイムアウト、HTTP の 429 と 5xx に限る。
/// 無料枠の超過は HTTP の状態にかかわらず恒久的な失敗とする。
/// 一覧に無い失敗は、再試行の対象を広げないため恒久的な失敗とする。
pub fn classify_failure(status: Option<u16>, code: Option<&str>, timed_out: bool) -> FailureKind {
    if is_free_tier_exceeded(code) {
        return FailureKind::Permanent;
    }
    if timed_out {
        return FailureKind::Temporary;
    }
    if let Some(kind) = classify_http_status(status) {
        return kind;
    }
    if let Some(kind) = code.and_then(parse_http_status) {
        return kind;
    }
    FailureKind::Permanent
}

/// 無料枠の超過を表す Workers AI のエラーコード。
const FREE_TIER_EXCEEDED_CODES: &[&str] = &["3040", "E_DAILY_LIMIT_EXCEEDED"];

/// 無料枠の超過を表す Workers AI のエラーコードか。
pub fn is_free_tier_exceeded(code: Option<&str>) -> bool {
    code.is_some_and(|code| {
        FREE_TIER_EXCEEDED_CODES
            .iter()
            .any(|known| code.trim().eq_ignore_ascii_case(known))
    })
}

fn classify_http_status(status: Option<u16>) -> Option<FailureKind> {
    let status = status?;
    match status {
        429 | 500..=599 => Some(FailureKind::Temporary),
        400 | 401 | 403 | 422 => Some(FailureKind::Permanent),
        _ => None,
    }
}

fn parse_http_status(code: &str) -> Option<FailureKind> {
    classify_http_status(code.trim().parse::<u16>().ok())
}

/// 生成の状態と、そこまでの試行回数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobState {
    /// 状態。
    pub status: JobStatus,
    /// 試行回数。
    pub attempts: u32,
}

impl JobState {
    /// 状態と試行回数から作る。
    pub fn new(status: JobStatus, attempts: u32) -> Self {
        Self { status, attempts }
    }

    /// 作成直後の状態。
    pub fn initial() -> Self {
        Self::new(JobStatus::Pending, 0)
    }
}

/// 1 回の処理の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// 出力を保存できた。
    Succeeded,
    /// 再試行する失敗。
    TemporaryFailure,
    /// 再試行しない失敗。
    PermanentFailure,
}

/// 1 回の処理の後の遷移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobTransition {
    /// 再試行する。
    Retry {
        /// 遷移後の状態。状態は `pending` になる。
        state: JobState,
        /// Queue に指定する遅延 (秒)。
        delay_seconds: u32,
    },
    /// 処理を終えた。
    Terminal(JobState),
}

impl JobTransition {
    /// 遷移後の状態。
    pub fn state(self) -> JobState {
        match self {
            Self::Retry { state, .. } => state,
            Self::Terminal(state) => state,
        }
    }
}

/// 処理の結果を状態へ反映する。
///
/// 成功したら `generated`、恒久的な失敗なら `failed` にする。
/// 一時的な失敗は、再試行の上限までは `pending` に戻し、上限を超えたら `failed` にする。
/// `generated` と `failed` は同じメッセージを再度処理しても変えない。
pub fn advance(state: JobState, outcome: AttemptOutcome) -> JobTransition {
    if state.status.is_terminal() {
        return JobTransition::Terminal(state);
    }
    let attempts = state.attempts.saturating_add(1);
    match outcome {
        AttemptOutcome::Succeeded => {
            JobTransition::Terminal(JobState::new(JobStatus::Generated, attempts))
        }
        AttemptOutcome::PermanentFailure => {
            JobTransition::Terminal(JobState::new(JobStatus::Failed, attempts))
        }
        AttemptOutcome::TemporaryFailure => match retry_delay_seconds(attempts) {
            Some(delay_seconds) => JobTransition::Retry {
                state: JobState::new(JobStatus::Pending, attempts),
                delay_seconds,
            },
            None => JobTransition::Terminal(JobState::new(JobStatus::Failed, attempts)),
        },
    }
}

/// 試行回数の後の再試行の間隔。上限を超えたら None。
pub fn retry_delay_seconds(attempts: u32) -> Option<u32> {
    match attempts {
        1 => Some(RETRY_DELAYS_SECONDS[0]),
        2 => Some(RETRY_DELAYS_SECONDS[1]),
        3 => Some(RETRY_DELAYS_SECONDS[2]),
        _ => None,
    }
}

/// `enqueued` の lease の期限。
pub fn lease_deadline(now_epoch_seconds: i64) -> i64 {
    now_epoch_seconds.saturating_add(LEASE_SECONDS)
}

/// 再試行を始める時刻。
pub fn next_attempt_at(now_epoch_seconds: i64, delay_seconds: u32) -> i64 {
    now_epoch_seconds.saturating_add(i64::from(delay_seconds))
}

/// メッセージを処理してよい状態か。
///
/// `enqueued` は lease の期限内だけ処理する。`pending` は再試行の時刻を過ぎたときだけ
/// 処理する。Queue の遅延で届いた再試行のメッセージは、この経路で `pending` を処理する。
pub fn is_due(
    status: JobStatus,
    lease_expires_at: Option<i64>,
    next_attempt_at: Option<i64>,
    now_epoch_seconds: i64,
) -> bool {
    match status {
        JobStatus::Pending => next_attempt_at.is_none_or(|deadline| now_epoch_seconds >= deadline),
        JobStatus::Enqueued => {
            lease_expires_at.is_some_and(|deadline| now_epoch_seconds <= deadline)
        }
        JobStatus::Generated | JobStatus::Failed => false,
    }
}

/// `enqueued` の lease が期限を過ぎたか。
pub fn lease_expired(
    status: JobStatus,
    lease_expires_at: Option<i64>,
    now_epoch_seconds: i64,
) -> bool {
    status == JobStatus::Enqueued
        && lease_expires_at.is_none_or(|deadline| now_epoch_seconds > deadline)
}

/// 日の評価の一意キー。同じ日の評価は 1 行だけを持つ。
pub fn daily_evaluation_key(date: &str) -> String {
    format!("daily:{date}")
}

/// 次回セットの提案の一意キー。種目ごとに 1 行だけを持つ。
pub fn set_proposal_key(exercise_id: &str) -> String {
    format!("set:{exercise_id}")
}

/// 週の評価の一意キー。
pub fn weekly_evaluation_key(start: &str, end: &str) -> String {
    format!("week:{start}:{end}")
}

/// 月の評価の一意キー。
pub fn monthly_evaluation_key(start: &str, end: &str) -> String {
    format!("month:{start}:{end}")
}

#[cfg(test)]
mod tests {
    //! 状態、再試行、lease、一意キーの例示テスト。
    //!
    //! 対応する要求: `REQ-AI-014`、`REQ-AI-018`。状態と再試行は生成の内部条件であり、
    //! 受入基準の外側も含む。

    use super::{
        advance, classify_failure, daily_evaluation_key, is_due, lease_deadline, lease_expired,
        monthly_evaluation_key, next_attempt_at, retry_delay_seconds, set_proposal_key,
        weekly_evaluation_key, AttemptOutcome, FailureKind, JobState, JobStatus, JobTransition,
        LEASE_SECONDS, MAX_ATTEMPTS, RETRY_DELAYS_SECONDS,
    };

    #[test]
    fn a_successful_attempt_becomes_generated() {
        let transition = advance(
            JobState::new(JobStatus::Enqueued, 0),
            AttemptOutcome::Succeeded,
        );
        assert_eq!(
            transition,
            JobTransition::Terminal(JobState::new(JobStatus::Generated, 1))
        );
    }

    #[test]
    fn a_permanent_failure_becomes_failed_without_retrying() {
        let transition = advance(
            JobState::new(JobStatus::Enqueued, 0),
            AttemptOutcome::PermanentFailure,
        );
        assert_eq!(
            transition,
            JobTransition::Terminal(JobState::new(JobStatus::Failed, 1))
        );
    }

    #[test]
    fn temporary_failures_use_the_fixed_delays_and_stop_at_the_limit() {
        let mut state = JobState::new(JobStatus::Enqueued, 0);
        for (attempt, delay) in RETRY_DELAYS_SECONDS.into_iter().enumerate() {
            let transition = advance(state, AttemptOutcome::TemporaryFailure);
            assert_eq!(
                transition,
                JobTransition::Retry {
                    state: JobState::new(JobStatus::Pending, attempt as u32 + 1),
                    delay_seconds: delay,
                }
            );
            state = transition.state();
        }
        let last = advance(
            JobState::new(JobStatus::Enqueued, MAX_ATTEMPTS - 1),
            AttemptOutcome::TemporaryFailure,
        );
        assert_eq!(
            last,
            JobTransition::Terminal(JobState::new(JobStatus::Failed, MAX_ATTEMPTS))
        );
    }

    #[test]
    fn retry_delays_are_one_five_and_thirty_seconds() {
        assert_eq!(retry_delay_seconds(1), Some(1));
        assert_eq!(retry_delay_seconds(2), Some(5));
        assert_eq!(retry_delay_seconds(3), Some(30));
        assert_eq!(retry_delay_seconds(4), None);
        assert_eq!(retry_delay_seconds(0), None);
    }

    #[test]
    fn terminal_states_do_not_change_when_a_message_is_reprocessed() {
        for status in [JobStatus::Generated, JobStatus::Failed] {
            for outcome in [
                AttemptOutcome::Succeeded,
                AttemptOutcome::TemporaryFailure,
                AttemptOutcome::PermanentFailure,
            ] {
                let state = JobState::new(status, 2);
                assert_eq!(
                    advance(state, outcome),
                    JobTransition::Terminal(state),
                    "{status:?} with {outcome:?}"
                );
            }
        }
    }

    #[test]
    fn timeouts_429_and_5xx_are_temporary() {
        assert_eq!(classify_failure(None, None, true), FailureKind::Temporary);
        for status in [429, 500, 503, 599] {
            assert_eq!(
                classify_failure(Some(status), None, false),
                FailureKind::Temporary,
                "{status}"
            );
        }
    }

    #[test]
    fn permanent_statuses_are_400_401_403_and_422() {
        for status in [400, 401, 403, 422] {
            assert_eq!(
                classify_failure(Some(status), None, false),
                FailureKind::Permanent,
                "{status}"
            );
        }
    }

    #[test]
    fn the_free_tier_exceeded_code_is_permanent_even_with_429() {
        assert_eq!(
            classify_failure(Some(429), Some("3040"), false),
            FailureKind::Permanent
        );
        assert_eq!(
            classify_failure(None, Some("3040"), false),
            FailureKind::Permanent
        );
        assert_eq!(
            classify_failure(Some(429), Some("429"), false),
            FailureKind::Temporary
        );
    }

    #[test]
    fn an_unknown_failure_is_permanent() {
        assert_eq!(
            classify_failure(Some(404), Some("3001"), false),
            FailureKind::Permanent
        );
        assert_eq!(classify_failure(None, None, false), FailureKind::Permanent);
    }

    #[test]
    fn a_lease_is_valid_through_its_deadline_and_expired_after_it() {
        let now = 1_700_000_000;
        let deadline = lease_deadline(now);
        assert_eq!(deadline, now + LEASE_SECONDS);
        assert!(is_due(JobStatus::Enqueued, Some(deadline), None, deadline));
        assert!(!is_due(
            JobStatus::Enqueued,
            Some(deadline),
            None,
            deadline + 1
        ));
        assert!(lease_expired(
            JobStatus::Enqueued,
            Some(deadline),
            deadline + 1
        ));
        assert!(!lease_expired(
            JobStatus::Enqueued,
            Some(deadline),
            deadline
        ));
    }

    #[test]
    fn a_pending_row_is_processed_after_its_next_attempt_time() {
        let now = 1_700_000_000;
        let due = next_attempt_at(now, 30);
        assert_eq!(due, now + 30);
        assert!(!is_due(JobStatus::Pending, None, Some(due), due - 1));
        assert!(is_due(JobStatus::Pending, None, Some(due), due));
        assert!(is_due(JobStatus::Pending, None, None, now));
        assert!(!is_due(JobStatus::Generated, None, None, now));
        assert!(!is_due(JobStatus::Failed, None, None, now));
    }

    #[test]
    fn unique_keys_separate_the_kinds_and_targets() {
        assert_eq!(daily_evaluation_key("2026-11-12"), "daily:2026-11-12");
        assert_eq!(set_proposal_key("bench-press"), "set:bench-press");
        assert_eq!(
            weekly_evaluation_key("2026-11-09", "2026-11-15"),
            "week:2026-11-09:2026-11-15"
        );
        assert_eq!(
            monthly_evaluation_key("2026-11-01", "2026-11-30"),
            "month:2026-11-01:2026-11-30"
        );
        assert_ne!(
            daily_evaluation_key("2026-11-12"),
            set_proposal_key("bench-press")
        );
    }
}
