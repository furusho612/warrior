//! サーバーに保存済みのトレーニングの全体。
//!
//! 同じアカウントでログインしたほかの端末の閲覧と、記録の引き継ぎの応答に使う
//! (REQ-NFR-006、REQ-NFR-015)。端末はこの形で IndexedDB のセッション木を置き換える。
//! 行の項目は D1 の列に合わせ、`user_id` は応答に含めない。

use serde::{Deserialize, Serialize};

use crate::sync::SessionRecord;

/// 保存済みのトレーニングの全体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingRecord {
    /// トレーニングの行。
    pub session: SessionRecord,
    /// トレーニングに追加した種目。
    pub exercises: Vec<TrainingExerciseRecord>,
    /// 種目に記録したセット。
    pub sets: Vec<SetRecordRow>,
    /// ドロップセットの段。
    pub segments: Vec<SetSegmentRecord>,
    /// スーパーセットの組。組に含める種目は `members` が持つ。
    pub supersets: Vec<SupersetRecord>,
}

/// トレーニングに追加した種目の行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingExerciseRecord {
    /// 種目の行の ID。
    pub id: String,
    /// 対象のトレーニングの ID。
    pub training_id: String,
    /// 種目の ID。
    pub exercise_id: String,
    /// 記録した時点の種目のスナップショット。JSON の文字列である。
    pub snapshot: String,
    /// 並び順。
    pub position: i64,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// セットの行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetRecordRow {
    /// セットの ID。
    pub id: String,
    /// 対象の種目の行の ID。
    pub training_exercise_id: String,
    /// 並び順。
    pub position: i64,
    /// セット種別。`normal`、`warmup`、`drop` のいずれかである。
    pub kind: String,
    /// 完了したか。0 か 1 である。
    pub completed: i64,
    /// RPE。
    pub rpe: Option<f64>,
    /// グリップ。JSON の文字列である。
    pub grip: Option<String>,
    /// 自重種目のセットを記録した時点の体重 (kg)。身体情報が無いときは None。
    #[serde(default)]
    pub body_weight_kg: Option<f64>,
    /// 記録した時刻。
    pub recorded_at: Option<String>,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// ドロップセットの段の行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetSegmentRecord {
    /// 段の ID。
    pub id: String,
    /// 対象のセットの ID。
    pub set_id: String,
    /// 並び順。
    pub position: i64,
    /// 重量 (kg)。自重種目はセットごとの加重を表す。
    pub weight_kg: Option<f64>,
    /// 回数。
    pub reps: Option<i64>,
    /// 反動の回数。
    pub momentum_reps: i64,
    /// 補助の回数。
    pub assisted_reps: i64,
    /// 時間 (秒)。
    pub duration_seconds: Option<i64>,
    /// 左の重量 (kg)。
    pub left_weight_kg: Option<f64>,
    /// 左の回数。
    pub left_reps: Option<i64>,
    /// 左の時間 (秒)。
    pub left_duration_seconds: Option<i64>,
    /// 右の重量 (kg)。
    pub right_weight_kg: Option<f64>,
    /// 右の回数。
    pub right_reps: Option<i64>,
    /// 右の時間 (秒)。
    pub right_duration_seconds: Option<i64>,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// スーパーセットの組の行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupersetRecord {
    /// 組の ID。
    pub id: String,
    /// 対象のトレーニングの ID。
    pub training_id: String,
    /// 並び順。
    pub position: i64,
    /// 組に含める種目。
    pub members: Vec<SupersetMemberRecord>,
    /// 作成した時刻。
    pub created_at: String,
    /// 更新した時刻。
    pub updated_at: String,
}

/// スーパーセットに含める種目の行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersetMemberRecord {
    /// 対象の組の ID。
    pub superset_id: String,
    /// 対象の種目の行の ID。
    pub training_exercise_id: String,
    /// 組の中での並び順。
    pub position: i64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::TrainingRecord;

    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";

    fn record_value() -> serde_json::Value {
        json!({
            "session": {
                "id": TRAINING_ID,
                "date": "2026-11-12",
                "started_at": "2026-11-12T03:00:00Z",
                "memo": "メモ",
                "created_at": "2026-11-12T03:00:00Z",
                "updated_at": "2026-11-12T03:10:00Z"
            },
            "exercises": [{
                "id": "exercise-row",
                "training_id": TRAINING_ID,
                "exercise_id": "bench-press",
                "snapshot": "{}",
                "position": 1,
                "created_at": "2026-11-12T03:00:00Z",
                "updated_at": "2026-11-12T03:00:00Z"
            }],
            "sets": [{
                "id": "set-1",
                "training_exercise_id": "exercise-row",
                "position": 1,
                "kind": "normal",
                "completed": 1,
                "rpe": 8.0,
                "grip": null,
                "recorded_at": null,
                "created_at": "2026-11-12T03:00:00Z",
                "updated_at": "2026-11-12T03:00:00Z"
            }],
            "segments": [{
                "id": "segment-1",
                "set_id": "set-1",
                "position": 1,
                "weight_kg": 100.0,
                "reps": 5,
                "momentum_reps": 0,
                "assisted_reps": 0,
                "duration_seconds": null,
                "left_weight_kg": null,
                "left_reps": null,
                "left_duration_seconds": null,
                "right_weight_kg": null,
                "right_reps": null,
                "right_duration_seconds": null,
                "created_at": "2026-11-12T03:00:00Z",
                "updated_at": "2026-11-12T03:00:00Z"
            }],
            "supersets": [{
                "id": "superset-1",
                "training_id": TRAINING_ID,
                "position": 1,
                "members": [{"superset_id": "superset-1", "training_exercise_id": "exercise-row", "position": 1}],
                "created_at": "2026-11-12T03:00:00Z",
                "updated_at": "2026-11-12T03:00:00Z"
            }]
        })
    }

    #[test]
    fn a_training_record_round_trips_through_json() {
        let record: TrainingRecord =
            serde_json::from_value(record_value()).expect("the record must deserialize");
        assert_eq!(record.session.id, TRAINING_ID);
        assert_eq!(record.session.memo, "メモ");
        assert_eq!(record.exercises.len(), 1);
        assert_eq!(record.exercises[0].position, 1);
        assert_eq!(record.sets[0].completed, 1);
        assert_eq!(record.segments[0].weight_kg, Some(100.0));
        assert_eq!(
            record.supersets[0].members[0].training_exercise_id,
            "exercise-row"
        );
        let restored = serde_json::to_value(&record).expect("the record must serialize");
        let reparsed: TrainingRecord =
            serde_json::from_value(restored).expect("the record must round trip");
        assert_eq!(reparsed, record);
    }
}
