//! 端末の送信待ちの列と、同期の操作の型と検証。
//!
//! 操作は端末が生成した UUID v4 を持ち、同じ操作 ID の再送でサーバーの状態を変えない (ADR-002)。
//! 同じトレーニングの操作は順序を保ち、依存する操作は親が解決するまで送らない。
//! サーバーの適用は `backend/warrior/src/sync.rs` が担う。

use serde::{Deserialize, Serialize};

use crate::datetime;
use crate::exercise::ExerciseSnapshot;
use crate::ids::is_valid_uuid_v4;
use crate::jst::JstDate;
use crate::training_session::{PersonalBestItem, SessionSource};

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
    /// トレーニングへの種目の追加。
    TrainingExerciseCreate,
    /// トレーニングからの種目の削除。
    TrainingExerciseDelete,
    /// トレーニングの種目の並べ替え。
    TrainingExerciseReorder,
    /// セットの追加。
    SetCreate,
    /// セットの更新。
    SetUpdate,
    /// セットの削除。
    SetDelete,
    /// セットの並べ替え。
    SetReorder,
    /// セットの完了の状態の変更。
    SetComplete,
}

impl OperationKind {
    /// API と D1 の `kind` に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TrainingSessionCreate => "training_session_create",
            Self::TrainingSessionUpdate => "training_session_update",
            Self::TrainingExerciseCreate => "training_exercise_create",
            Self::TrainingExerciseDelete => "training_exercise_delete",
            Self::TrainingExerciseReorder => "training_exercise_reorder",
            Self::SetCreate => "set_create",
            Self::SetUpdate => "set_update",
            Self::SetDelete => "set_delete",
            Self::SetReorder => "set_reorder",
            Self::SetComplete => "set_complete",
        }
    }

    /// `kind` の文字列を種別に戻す。
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "training_session_create" => Some(Self::TrainingSessionCreate),
            "training_session_update" => Some(Self::TrainingSessionUpdate),
            "training_exercise_create" => Some(Self::TrainingExerciseCreate),
            "training_exercise_delete" => Some(Self::TrainingExerciseDelete),
            "training_exercise_reorder" => Some(Self::TrainingExerciseReorder),
            "set_create" => Some(Self::SetCreate),
            "set_update" => Some(Self::SetUpdate),
            "set_delete" => Some(Self::SetDelete),
            "set_reorder" => Some(Self::SetReorder),
            "set_complete" => Some(Self::SetComplete),
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
    /// 開始元の種別。省略した場合は空の状態からの開始とする。
    #[serde(default)]
    pub source_kind: SessionSource,
    /// 開始元のルーティンの UUID v4。
    #[serde(default)]
    pub routine_id: Option<String>,
    /// 開始元のプログラムの日の UUID v4。
    #[serde(default)]
    pub program_day_id: Option<String>,
    /// 開始元の独自のプログラムの日の UUID v4。
    #[serde(default)]
    pub custom_program_day_id: Option<String>,
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

/// トレーニングへの種目の追加の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingExerciseCreatePayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 追加する種目の行の UUID v4。
    pub training_exercise_id: String,
    /// 種目の ID。
    pub exercise_id: String,
    /// 記録した時点の種目の内容。
    pub snapshot: ExerciseSnapshot,
    /// トレーニングの中での並び順。1 以上。
    pub position: i64,
}

/// トレーニングからの種目の削除の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingExerciseDeletePayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 削除する種目の行の UUID v4。
    pub training_exercise_id: String,
}

/// トレーニングの種目の並べ替えの本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingExerciseReorderPayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 新しい順序の種目の行の UUID v4。先頭から 1、2 の位置になる。
    pub ordered_ids: Vec<String>,
}

/// セットの 1 つの段の本文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetSegmentPayload {
    /// 段の UUID v4。
    pub segment_id: String,
    /// セットの中での並び順。1 以上。
    pub position: i64,
    /// 重量 (kg)。自重種目はセットごとの加重である。
    pub weight_kg: f64,
    /// 回数。
    pub reps: u32,
    /// 反動を使った回数。
    pub momentum_reps: u32,
    /// 補助を受けた回数。
    pub assisted_reps: u32,
    /// 時間で記録する場合の秒数。
    pub duration_seconds: Option<u32>,
    /// 左の値。左右別の種目だけが持つ。
    pub left: Option<crate::set_calc::SideValue>,
    /// 右の値。左右別の種目だけが持つ。
    pub right: Option<crate::set_calc::SideValue>,
}

/// セットの追加の本文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetCreatePayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// セットを追加する種目の行の UUID v4。
    pub training_exercise_id: String,
    /// 追加するセットの UUID v4。
    pub set_id: String,
    /// 種目の中での並び順。1 以上。
    pub position: i64,
    /// セット種別。
    pub kind: crate::set_calc::SetKind,
    /// RPE。
    pub rpe: Option<f64>,
    /// グリップ。選ばない種目は None。
    pub grip: Option<crate::exercise::GripSelection>,
    /// 記録した時点の体重 (kg)。身体情報が無いときは None。
    pub body_weight_kg: Option<f64>,
    /// 段の本文。
    pub segments: Vec<SetSegmentPayload>,
}

/// セットの更新の本文。段は全置換である。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetUpdatePayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 更新するセットの UUID v4。
    pub set_id: String,
    /// 種目の中での並び順。1 以上。
    pub position: i64,
    /// セット種別。
    pub kind: crate::set_calc::SetKind,
    /// 完了したか。値の無いセットは完了にできない。
    pub completed: bool,
    /// RPE。
    pub rpe: Option<f64>,
    /// グリップ。選ばない種目は None。
    pub grip: Option<crate::exercise::GripSelection>,
    /// 記録した時点の体重 (kg)。身体情報が無いときは None。
    pub body_weight_kg: Option<f64>,
    /// 段の本文。段は全置換である。
    pub segments: Vec<SetSegmentPayload>,
}

/// セットの削除の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetDeletePayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 削除するセットの UUID v4。
    pub set_id: String,
}

/// セットの並べ替えの本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetReorderPayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 並べ替える種目の行の UUID v4。
    pub training_exercise_id: String,
    /// 新しい順序のセットの UUID v4。その種目の全セットと一致する。
    pub ordered_ids: Vec<String>,
}

/// セットの完了の状態の変更の本文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetCompletePayload {
    /// 対象のトレーニングの UUID v4。操作の `training_id` と一致する。
    pub training_id: String,
    /// 完了の状態を変えるセットの UUID v4。
    pub set_id: String,
    /// 完了したか。
    pub completed: bool,
}

/// 端末とサーバーが持つトレーニングの行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 開始元の種別。
    #[serde(default)]
    pub source_kind: SessionSource,
    /// 開始元のルーティンの UUID v4。
    #[serde(default)]
    pub routine_id: Option<String>,
    /// 開始元のプログラムの日の UUID v4。
    #[serde(default)]
    pub program_day_id: Option<String>,
    /// 開始元の独自のプログラムの日の UUID v4。
    #[serde(default)]
    pub custom_program_day_id: Option<String>,
    /// 終了した時刻の ISO 8601 の UTC の文字列。
    #[serde(default)]
    pub finished_at: Option<String>,
    /// 開始時刻から終了時刻までの秒数。
    #[serde(default)]
    pub duration_seconds: Option<i64>,
    /// 総重量 (kg)。
    #[serde(default)]
    pub total_weight_kg: Option<f64>,
    /// セットを持つ種目の数。
    #[serde(default)]
    pub exercise_count: Option<u32>,
    /// トレーニングのセット数。
    #[serde(default)]
    pub set_count: Option<u32>,
    /// 終了時に更新した自己ベスト。
    ///
    /// サーバーの `training_sessions.personal_bests` はセッションのベスト全部を持つため、
    /// 端末のこの値とは意味が異なる。端末は応答で受け取った更新分だけを保存する。
    #[serde(default)]
    pub personal_bests: Vec<PersonalBestItem>,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// 検証済みの操作の本文。
#[derive(Debug, Clone, PartialEq)]
pub enum ValidatedPayload {
    /// 作成の本文。
    Create(SessionCreatePayload),
    /// 更新の本文。
    Update(SessionUpdatePayload),
    /// 種目の追加の本文。
    ExerciseCreate(TrainingExerciseCreatePayload),
    /// 種目の削除の本文。
    ExerciseDelete(TrainingExerciseDeletePayload),
    /// 種目の並べ替えの本文。
    ExerciseReorder(TrainingExerciseReorderPayload),
    /// セットの追加の本文。
    SetCreate(SetCreatePayload),
    /// セットの更新の本文。
    SetUpdate(SetUpdatePayload),
    /// セットの削除の本文。
    SetDelete(SetDeletePayload),
    /// セットの並べ替えの本文。
    SetReorder(SetReorderPayload),
    /// セットの完了の状態の変更の本文。
    SetComplete(SetCompletePayload),
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
    /// 空の開始元にルーティン ID またはプログラムの日 ID がある。
    SourceIdUnexpected,
    /// 種目の行または種目の ID が UUID v4 でない。
    InvalidExerciseId,
    /// 種目のスナップショットが不正である、または種目の ID と一致しない。
    InvalidSnapshot,
    /// 種目の並び順が 1 未満である。
    InvalidPosition,
    /// 並べ替えの ID が空、重複、または UUID v4 でない。
    InvalidOrderedIds,
    /// セットの ID が UUID v4 でない。
    InvalidSetId,
    /// 段の ID が UUID v4 でない、または重複している。
    InvalidSegmentId,
    /// 重量が負、または有限でない。
    InvalidWeight,
    /// 回数が 0 である、または反動と補助の回数が回数を超えている。
    InvalidReps,
    /// RPE が 6 から 10 の 0.5 刻みでない。
    InvalidRpe,
    /// 時間が 1 秒から 86400 秒の範囲外である。
    InvalidDuration,
    /// 記録した時点の体重が正の有限の値でない。
    InvalidBodyWeight,
    /// 段が空である。
    EmptySegments,
    /// 通常とウォームアップのセットが 1 つでない段を持つ。
    InvalidSegmentCount,
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
            Self::SourceIdUnexpected => "source_id_unexpected",
            Self::InvalidExerciseId => "invalid_exercise_id",
            Self::InvalidSnapshot => "invalid_snapshot",
            Self::InvalidPosition => "invalid_position",
            Self::InvalidOrderedIds => "invalid_ordered_ids",
            Self::InvalidSetId => "invalid_set_id",
            Self::InvalidSegmentId => "invalid_segment_id",
            Self::InvalidWeight => "invalid_weight",
            Self::InvalidReps => "invalid_reps",
            Self::InvalidRpe => "invalid_rpe",
            Self::InvalidDuration => "invalid_duration",
            Self::InvalidBodyWeight => "invalid_body_weight",
            Self::EmptySegments => "empty_segments",
            Self::InvalidSegmentCount => "invalid_segment_count",
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
            validate_source_ids(&payload)?;
            validate_session_fields(&payload.date, &payload.started_at, &payload.memo)?;
            ValidatedPayload::Create(payload)
        }
        OperationKind::TrainingSessionUpdate => {
            let payload: SessionUpdatePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            validate_session_fields(&payload.date, &payload.started_at, &payload.memo)?;
            ValidatedPayload::Update(payload)
        }
        OperationKind::TrainingExerciseCreate => {
            let payload: TrainingExerciseCreatePayload =
                serde_json::from_value(operation.payload.clone())
                    .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_exercise::validate_create(operation, payload)?
        }
        OperationKind::TrainingExerciseDelete => {
            let payload: TrainingExerciseDeletePayload =
                serde_json::from_value(operation.payload.clone())
                    .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_exercise::validate_delete(operation, payload)?
        }
        OperationKind::TrainingExerciseReorder => {
            let payload: TrainingExerciseReorderPayload =
                serde_json::from_value(operation.payload.clone())
                    .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_exercise::validate_reorder(operation, payload)?
        }
        OperationKind::SetCreate => {
            let payload: SetCreatePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_set::validate_create(operation, payload)?
        }
        OperationKind::SetUpdate => {
            let payload: SetUpdatePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_set::validate_update(operation, payload)?
        }
        OperationKind::SetDelete => {
            let payload: SetDeletePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_set::validate_delete(operation, payload)?
        }
        OperationKind::SetReorder => {
            let payload: SetReorderPayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_set::validate_reorder(operation, payload)?
        }
        OperationKind::SetComplete => {
            let payload: SetCompletePayload = serde_json::from_value(operation.payload.clone())
                .map_err(|_| SyncError::InvalidPayload)?;
            crate::sync_set::validate_complete(operation, payload)?
        }
    };

    Ok(ValidatedOperation {
        operation: operation.clone(),
        payload,
    })
}

fn validate_source_ids(payload: &SessionCreatePayload) -> Result<(), SyncError> {
    if payload.source_kind == SessionSource::Empty
        && (payload.routine_id.is_some()
            || payload.program_day_id.is_some()
            || payload.custom_program_day_id.is_some())
    {
        return Err(SyncError::SourceIdUnexpected);
    }
    Ok(())
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
mod tests;
