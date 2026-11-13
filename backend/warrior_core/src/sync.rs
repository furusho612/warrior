//! 端末の送信待ちの列と、同期の操作の型と検証。
//!
//! 操作は端末が生成した UUID v4 を持ち、同じ操作 ID の再送でサーバーの状態を変えない (ADR-002)。
//! 同じトレーニングの操作は順序を保ち、依存する操作は親が解決するまで送らない。
//! サーバーの適用は `backend/warrior/src/sync.rs` が担う。

use serde::{Deserialize, Serialize};

use crate::datetime;
use crate::ids::is_valid_uuid_v4;
use crate::jst::JstDate;

/// メモの最大の文字数 (PRD の REQ-NFR-019)。
pub const MEMO_MAX_CHARS: usize = 2000;

/// 同期の操作の種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    /// トレーニングの作成。
    TrainingSessionCreate,
    /// トレーニングの更新。
    TrainingSessionUpdate,
}

impl OperationKind {
    /// API と D1 の `kind` に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TrainingSessionCreate => "training_session_create",
            Self::TrainingSessionUpdate => "training_session_update",
        }
    }

    /// `kind` の文字列を種別に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "training_session_create" => Some(Self::TrainingSessionCreate),
            "training_session_update" => Some(Self::TrainingSessionUpdate),
            _ => None,
        }
    }
}

/// 端末が積む 1 つの操作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// 操作の UUID v4。
    pub operation_id: String,
    /// 操作が対象とするトレーニングの UUID v4。
    pub training_id: String,
    /// 同じトレーニングの中での順序。1 以上。
    pub sequence: u64,
    /// 操作の種別。
    pub kind: OperationKind,
    /// 依存する親の操作 ID。依存しないときは None。
    pub parent_operation_id: Option<String>,
    /// 操作の本文。
    pub payload: serde_json::Value,
}

/// トレーニングの作成の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCreatePayload {
    /// トレーニングの UUID v4。
    pub id: String,
    /// 日本時間の `YYYY-MM-DD`。
    pub date: String,
    /// 開始した時刻の ISO 8601 の UTC の文字列。
    pub started_at: String,
    /// トレーニング全体のメモ。
    pub memo: String,
}

/// トレーニングの更新の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionUpdatePayload {
    /// 日本時間の `YYYY-MM-DD`。
    pub date: String,
    /// 開始した時刻の ISO 8601 の UTC の文字列。
    pub started_at: String,
    /// トレーニング全体のメモ。
    pub memo: String,
}

/// 端末とサーバーが持つトレーニングの行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// トレーニングの UUID v4。
    pub id: String,
    /// 日本時間の `YYYY-MM-DD`。
    pub date: String,
    /// 開始した時刻の ISO 8601 の UTC の文字列。
    pub started_at: Option<String>,
    /// トレーニング全体のメモ。
    pub memo: String,
    /// サーバーが受理したメモ。
    #[serde(default)]
    pub confirmed_memo: String,
    /// サーバーが受理した操作の順序。
    #[serde(default)]
    pub confirmed_sequence: u64,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// 検証済みの操作の本文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidatedPayload {
    /// 作成の本文。
    Create(SessionCreatePayload),
    /// 更新の本文。
    Update(SessionUpdatePayload),
}

/// 検証済みの操作。
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedOperation {
    /// 検証した操作。
    pub operation: Operation,
    /// 種別に対応する本文。
    pub payload: ValidatedPayload,
}

/// 操作の検証の誤り。すべて API の 400 に対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncError {
    /// 操作 ID が UUID v4 でない。
    InvalidOperationId,
    /// トレーニング ID が UUID v4 でない。
    InvalidTrainingId,
    /// 順序が 1 未満である。
    InvalidSequence,
    /// 親の操作 ID が UUID v4 でない、または自分自身を指す。
    InvalidParent,
    /// 本文が種別の形に一致しない。
    InvalidPayload,
    /// 作成の本文の ID がトレーニング ID と一致しない。
    PayloadIdMismatch,
    /// 日付が日本時間の `YYYY-MM-DD` として妥当でない。
    InvalidDate,
    /// 開始時刻が ISO 8601 の UTC として妥当でない。
    InvalidStartedAt,
    /// 日付が開始時刻の日本時間の日付と一致しない。
    DateMismatch,
    /// メモが 2000 文字を超える。
    MemoTooLong,
}

impl SyncError {
    /// ログと画面に載せる識別子。
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidOperationId => "invalid_operation_id",
            Self::InvalidTrainingId => "invalid_training_id",
            Self::InvalidSequence => "invalid_sequence",
            Self::InvalidParent => "invalid_parent",
            Self::InvalidPayload => "invalid_payload",
            Self::PayloadIdMismatch => "payload_id_mismatch",
            Self::InvalidDate => "invalid_date",
            Self::InvalidStartedAt => "invalid_started_at",
            Self::DateMismatch => "date_mismatch",
            Self::MemoTooLong => "memo_too_long",
        }
    }
}

/// 操作の ID、順序、本文を検証する。
pub fn validate_operation(operation: &Operation) -> Result<ValidatedOperation, SyncError> {
    if !is_valid_uuid_v4(&operation.operation_id) {
        return Err(SyncError::InvalidOperationId);
    }
    if !is_valid_uuid_v4(&operation.training_id) {
        return Err(SyncError::InvalidTrainingId);
    }
    if operation.sequence < 1 {
        return Err(SyncError::InvalidSequence);
    }
    if let Some(parent) = &operation.parent_operation_id {
        if !is_valid_uuid_v4(parent) || parent == &operation.operation_id {
            return Err(SyncError::InvalidParent);
        }
    }

    let payload = match operation.kind {
        OperationKind::TrainingSessionCreate => {
            let payload: SessionCreatePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            if payload.id != operation.training_id {
                return Err(SyncError::PayloadIdMismatch);
            }
            validate_session_fields(&payload.date, &payload.started_at, &payload.memo)?;
            ValidatedPayload::Create(payload)
        }
        OperationKind::TrainingSessionUpdate => {
            let payload: SessionUpdatePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            validate_session_fields(&payload.date, &payload.started_at, &payload.memo)?;
            ValidatedPayload::Update(payload)
        }
    };

    Ok(ValidatedOperation {
        operation: operation.clone(),
        payload,
    })
}

fn validate_session_fields(date: &str, started_at: &str, memo: &str) -> Result<(), SyncError> {
    let date = JstDate::parse(date).ok_or(SyncError::InvalidDate)?;
    if datetime::parse_utc(started_at).is_none() {
        return Err(SyncError::InvalidStartedAt);
    }
    let japan_date =
        datetime::japan_date_from_utc(started_at).ok_or(SyncError::InvalidStartedAt)?;
    if date.to_string() != japan_date {
        return Err(SyncError::DateMismatch);
    }
    if memo.chars().count() > MEMO_MAX_CHARS {
        return Err(SyncError::MemoTooLong);
    }
    Ok(())
}

/// 送信待ちの列の 1 行の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxState {
    /// 送信できる。
    Pending,
    /// 送信中である。
    Sending,
    /// 親の操作が解決するまで送れない。
    Blocked,
    /// 恒久的なエラーで失敗した。
    Failed,
}

/// 送信待ちの列の 1 行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboxEntry {
    /// 送る操作。
    pub operation: Operation,
    /// 状態。
    pub state: OutboxState,
    /// 一時的な失敗の回数。
    pub attempts: u32,
    /// 送信を試みたことがあるか。
    #[serde(default)]
    pub attempted: bool,
}

/// 次に送る操作を選ぶ。
///
/// 同じトレーニングでより小さい順序の Pending または Sending があれば送らない。
/// 親が列に残っている間は送らず、親が成功して列から消えたら送る。
/// Failed の操作は、同じトレーニングの後続の操作を止めない。
pub fn next_sendable(entries: &[OutboxEntry]) -> Option<&OutboxEntry> {
    let mut best: Option<&OutboxEntry> = None;
    for entry in entries {
        if entry.state != OutboxState::Pending {
            continue;
        }
        if entries.iter().any(|other| {
            other.operation.training_id == entry.operation.training_id
                && other.operation.sequence < entry.operation.sequence
                && matches!(other.state, OutboxState::Pending | OutboxState::Sending)
        }) {
            continue;
        }
        if let Some(parent) = &entry.operation.parent_operation_id {
            if entries
                .iter()
                .any(|other| &other.operation.operation_id == parent)
            {
                continue;
            }
        }
        if best.is_none_or(|current| entry.operation.sequence < current.operation.sequence) {
            best = Some(entry);
        }
    }
    best
}

/// 操作を送信中にし、直接の子を Blocked にする。
pub fn mark_sending(entries: &mut [OutboxEntry], operation_id: &str) -> bool {
    let Some(index) = position(entries, operation_id) else {
        return false;
    };
    if entries[index].state != OutboxState::Pending {
        return false;
    }
    entries[index].state = OutboxState::Sending;
    entries[index].attempted = true;
    let parent_id = operation_id.to_owned();
    for entry in entries.iter_mut() {
        if entry.operation.parent_operation_id.as_deref() == Some(parent_id.as_str())
            && entry.state == OutboxState::Pending
        {
            entry.state = OutboxState::Blocked;
        }
    }
    true
}

/// 操作を成功として列から削除し、直接の子を Pending に戻す。
pub fn mark_success(entries: &mut Vec<OutboxEntry>, operation_id: &str) -> bool {
    let Some(index) = position(entries, operation_id) else {
        return false;
    };
    entries.remove(index);
    for entry in entries.iter_mut() {
        if entry.operation.parent_operation_id.as_deref() == Some(operation_id)
            && entry.state == OutboxState::Blocked
        {
            entry.state = OutboxState::Pending;
        }
    }
    true
}

/// 送信中のまま残った操作を Pending に戻す。試行の印は残す。
pub fn recover_interrupted(entries: &mut [OutboxEntry]) -> bool {
    let mut changed = false;
    for entry in entries.iter_mut() {
        if entry.state == OutboxState::Sending {
            entry.state = OutboxState::Pending;
            changed = true;
        }
    }
    changed
}

/// 同じトレーニングの最も新しい、Pending または Blocked の更新を返す。
/// その更新が送信を試みたものであれば、合流先にしない。
pub fn pending_update<'a>(
    entries: &'a [OutboxEntry],
    training_id: &str,
) -> Option<&'a OutboxEntry> {
    entries
        .iter()
        .filter(|entry| {
            entry.operation.training_id == training_id
                && entry.operation.kind == OperationKind::TrainingSessionUpdate
                && matches!(entry.state, OutboxState::Pending | OutboxState::Blocked)
        })
        .max_by_key(|entry| entry.operation.sequence)
        .filter(|entry| !entry.attempted)
}

/// 同じトレーニングの未解決の作成操作を返す。
pub fn unresolved_create<'a>(
    entries: &'a [OutboxEntry],
    training_id: &str,
) -> Option<&'a OutboxEntry> {
    entries
        .iter()
        .filter(|entry| {
            entry.operation.training_id == training_id
                && entry.operation.kind == OperationKind::TrainingSessionCreate
                && matches!(
                    entry.state,
                    OutboxState::Pending
                        | OutboxState::Sending
                        | OutboxState::Blocked
                        | OutboxState::Failed
                )
        })
        .max_by_key(|entry| entry.operation.sequence)
}

/// 一時的な失敗として、Sending を Pending に戻し、試行回数を増やす。
pub fn mark_transient_failure(entries: &mut [OutboxEntry], operation_id: &str) -> bool {
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.operation.operation_id == operation_id)
    else {
        return false;
    };
    if entry.state != OutboxState::Sending {
        return false;
    }
    entry.state = OutboxState::Pending;
    entry.attempts = entry.attempts.saturating_add(1);
    true
}

/// 恒久的な失敗として、Sending を Failed にする。
pub fn mark_permanent_failure(entries: &mut [OutboxEntry], operation_id: &str) -> bool {
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.operation.operation_id == operation_id)
    else {
        return false;
    };
    if entry.state != OutboxState::Sending {
        return false;
    }
    entry.state = OutboxState::Failed;
    true
}

/// 操作とその子孫を列から削除し、削除した操作 ID を返す。
pub fn discard(entries: &mut Vec<OutboxEntry>, operation_id: &str) -> Vec<String> {
    let mut removed = vec![operation_id.to_owned()];
    let mut cursor = 0;
    while cursor < removed.len() {
        let parent = removed[cursor].clone();
        for entry in entries.iter() {
            let child_id = &entry.operation.operation_id;
            if entry.operation.parent_operation_id.as_deref() == Some(parent.as_str())
                && !removed.contains(child_id)
            {
                removed.push(child_id.clone());
            }
        }
        cursor += 1;
    }
    entries.retain(|entry| !removed.contains(&entry.operation.operation_id));
    removed
}

/// Failed の操作を Pending に戻し、試行回数を 0 にする。
pub fn retry(entries: &mut [OutboxEntry], operation_id: &str) -> bool {
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.operation.operation_id == operation_id)
    else {
        return false;
    };
    if entry.state != OutboxState::Failed {
        return false;
    }
    entry.state = OutboxState::Pending;
    entry.attempts = 0;
    true
}

/// 未保存の操作の件数。Pending、Sending、Blocked を数える。
pub fn unsaved_count(entries: &[OutboxEntry]) -> usize {
    entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.state,
                OutboxState::Pending | OutboxState::Sending | OutboxState::Blocked
            )
        })
        .count()
}

/// 失敗した操作の件数。
pub fn failed_count(entries: &[OutboxEntry]) -> usize {
    entries
        .iter()
        .filter(|entry| entry.state == OutboxState::Failed)
        .count()
}

fn position(entries: &[OutboxEntry], operation_id: &str) -> Option<usize> {
    entries
        .iter()
        .position(|entry| entry.operation.operation_id == operation_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        discard, mark_permanent_failure, mark_sending, mark_success, mark_transient_failure,
        next_sendable, pending_update, recover_interrupted, retry, unresolved_create,
        unsaved_count, validate_operation, Operation, OperationKind, OutboxEntry, OutboxState,
        SyncError, MEMO_MAX_CHARS,
    };

    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
    const UPDATE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";
    const PARENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3304";
    const CHILD_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3305";

    fn create_operation() -> Operation {
        Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 1,
            kind: OperationKind::TrainingSessionCreate,
            parent_operation_id: None,
            payload: json!({
                "id": TRAINING_ID,
                "date": "2026-11-12",
                "started_at": "2026-11-12T03:00:00Z",
                "memo": ""
            }),
        }
    }

    fn update_operation(operation_id: &str, sequence: u64) -> Operation {
        Operation {
            operation_id: operation_id.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence,
            kind: OperationKind::TrainingSessionUpdate,
            parent_operation_id: None,
            payload: json!({
                "date": "2026-11-12",
                "started_at": "2026-11-12T03:00:00Z",
                "memo": "更新"
            }),
        }
    }

    fn entry(operation: Operation, state: OutboxState) -> OutboxEntry {
        OutboxEntry {
            operation,
            state,
            attempts: 0,
            attempted: false,
        }
    }

    #[test]
    fn validates_a_create_operation() {
        let validated = validate_operation(&create_operation()).expect("valid create");
        assert_eq!(validated.operation.operation_id, OPERATION_ID);
    }

    #[test]
    fn rejects_a_create_with_a_different_payload_id() {
        let mut operation = create_operation();
        operation.payload["id"] = json!(UPDATE_ID);
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::PayloadIdMismatch)
        );
    }

    #[test]
    fn rejects_a_date_that_does_not_match_the_started_at() {
        let mut operation = create_operation();
        operation.payload["date"] = json!("2026-11-13");
        assert_eq!(validate_operation(&operation), Err(SyncError::DateMismatch));
    }

    #[test]
    fn rejects_a_memo_over_the_limit() {
        let mut operation = create_operation();
        operation.payload["memo"] = json!("あ".repeat(MEMO_MAX_CHARS + 1));
        assert_eq!(validate_operation(&operation), Err(SyncError::MemoTooLong));
    }

    #[test]
    fn rejects_a_sequence_below_one() {
        let mut operation = create_operation();
        operation.sequence = 0;
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::InvalidSequence)
        );
    }

    #[test]
    fn rejects_an_operation_id_that_is_not_a_uuid_v4() {
        let mut operation = create_operation();
        operation.operation_id = "not-a-uuid".to_owned();
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::InvalidOperationId)
        );
    }

    #[test]
    fn rejects_a_training_id_that_is_not_a_uuid_v4() {
        let mut operation = create_operation();
        operation.training_id = "not-a-uuid".to_owned();
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::InvalidTrainingId)
        );
    }

    #[test]
    fn rejects_an_operation_that_names_itself_as_the_parent() {
        let mut operation = create_operation();
        operation.parent_operation_id = Some(OPERATION_ID.to_owned());
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::InvalidParent)
        );
    }

    #[test]
    fn rejects_a_started_at_that_is_not_an_iso8601_utc_time() {
        let mut operation = create_operation();
        operation.payload["started_at"] = json!("2026-11-12 03:00:00");
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::InvalidStartedAt)
        );
    }

    #[test]
    fn accepts_a_memo_at_the_limit() {
        let mut operation = create_operation();
        operation.payload["memo"] = json!("あ".repeat(MEMO_MAX_CHARS));
        assert!(validate_operation(&operation).is_ok());
    }

    #[test]
    fn rejects_an_unknown_payload_field() {
        let mut operation = create_operation();
        operation.payload["unexpected"] = json!(true);
        assert_eq!(
            validate_operation(&operation),
            Err(SyncError::InvalidPayload)
        );
    }

    #[test]
    fn sends_the_oldest_pending_operation_of_a_training() {
        let entries = vec![
            entry(update_operation(UPDATE_ID, 2), OutboxState::Pending),
            entry(create_operation(), OutboxState::Pending),
        ];
        assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some(OPERATION_ID)
        );
    }

    #[test]
    fn blocks_a_child_until_the_parent_leaves_the_outbox() {
        let mut child = update_operation(CHILD_ID, 2);
        child.parent_operation_id = Some(OPERATION_ID.to_owned());
        let mut entries = vec![
            entry(create_operation(), OutboxState::Pending),
            entry(child, OutboxState::Pending),
        ];
        assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some(OPERATION_ID)
        );

        mark_sending(&mut entries, OPERATION_ID);
        assert_eq!(entries[1].state, OutboxState::Blocked);
        assert!(mark_success(&mut entries, OPERATION_ID));
        assert_eq!(entries[0].state, OutboxState::Pending);
        assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some(CHILD_ID)
        );
    }

    #[test]
    fn a_failed_operation_does_not_stop_later_operations() {
        let entries = vec![
            entry(create_operation(), OutboxState::Failed),
            entry(update_operation(UPDATE_ID, 2), OutboxState::Pending),
        ];
        assert_eq!(
            next_sendable(&entries).map(|entry| entry.operation.operation_id.as_str()),
            Some(UPDATE_ID)
        );
    }

    #[test]
    fn discarding_a_parent_removes_descendants() {
        let mut child = update_operation(CHILD_ID, 2);
        child.parent_operation_id = Some(OPERATION_ID.to_owned());
        let mut grandchild = update_operation(PARENT_ID, 3);
        grandchild.parent_operation_id = Some(CHILD_ID.to_owned());
        let mut entries = vec![
            entry(create_operation(), OutboxState::Failed),
            entry(child, OutboxState::Blocked),
            entry(grandchild, OutboxState::Blocked),
        ];

        let removed = discard(&mut entries, OPERATION_ID);
        assert_eq!(removed.len(), 3);
        assert!(entries.is_empty());
    }

    #[test]
    fn pending_update_uses_only_the_latest_pending_or_blocked_update() {
        let older = entry(update_operation(UPDATE_ID, 2), OutboxState::Pending);
        let mut attempted = entry(update_operation(PARENT_ID, 3), OutboxState::Pending);
        attempted.attempted = true;
        let newer = entry(update_operation(CHILD_ID, 4), OutboxState::Blocked);
        let entries = vec![older.clone(), attempted.clone(), newer];
        assert_eq!(
            pending_update(&entries, TRAINING_ID)
                .map(|entry| entry.operation.operation_id.as_str()),
            Some(CHILD_ID)
        );

        let entries = vec![older, attempted];
        assert_eq!(pending_update(&entries, TRAINING_ID), None);
    }

    #[test]
    fn an_unresolved_create_is_found_even_after_it_fails() {
        let mut create = entry(create_operation(), OutboxState::Failed);
        create.attempted = true;
        let entries = vec![create];
        assert_eq!(
            unresolved_create(&entries, TRAINING_ID)
                .map(|entry| entry.operation.operation_id.as_str()),
            Some(OPERATION_ID)
        );
    }

    #[test]
    fn recovering_an_interrupted_send_keeps_the_attempt_mark() {
        let mut entries = vec![entry(create_operation(), OutboxState::Pending)];
        assert!(mark_sending(&mut entries, OPERATION_ID));
        assert!(recover_interrupted(&mut entries));
        assert_eq!(entries[0].state, OutboxState::Pending);
        assert!(entries[0].attempted);
        assert!(!recover_interrupted(&mut entries));
    }

    #[test]
    fn a_transient_failure_returns_to_pending_and_counts_an_attempt() {
        let mut entries = vec![entry(create_operation(), OutboxState::Pending)];
        assert!(mark_sending(&mut entries, OPERATION_ID));
        assert!(mark_transient_failure(&mut entries, OPERATION_ID));
        assert_eq!(entries[0].state, OutboxState::Pending);
        assert_eq!(entries[0].attempts, 1);
    }

    #[test]
    fn a_permanent_failure_can_be_retried_or_discarded() {
        let mut entries = vec![entry(create_operation(), OutboxState::Pending)];
        assert!(mark_sending(&mut entries, OPERATION_ID));
        assert!(mark_permanent_failure(&mut entries, OPERATION_ID));
        assert_eq!(entries[0].state, OutboxState::Failed);
        assert!(retry(&mut entries, OPERATION_ID));
        assert_eq!(entries[0].state, OutboxState::Pending);
        assert_eq!(entries[0].attempts, 0);
    }

    #[test]
    fn unsaved_counts_exclude_failed_operations() {
        let entries = vec![
            entry(create_operation(), OutboxState::Pending),
            entry(update_operation(UPDATE_ID, 2), OutboxState::Failed),
        ];
        assert_eq!(unsaved_count(&entries), 1);
        assert_eq!(super::failed_count(&entries), 1);
    }
}
