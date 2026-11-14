//! セットの追加、更新、削除、並べ替え、完了の操作の本文の検証。
//!
//! 対応する要求: `REQ-SET-001`、`REQ-SET-002`、`REQ-SET-006`、`REQ-SET-007`、
//! `REQ-LIB-004`、`REQ-PRF-005`。
//! `warrior_core::sync` の操作の検証から呼ぶ。本文を操作のトレーニングの ID、セットの
//! ID、段、セット種別、RPE、記録した時点の体重について確かめ、`SetRecord` への変換を
//! 提供する。段の入力は warrior_core::set_calc の計算規則と同じ形にそろえる。

use crate::exercise::GripSelection;
use crate::ids::is_valid_uuid_v4;
use crate::profile::bodyweight_set_weight;
use crate::set_calc::{
    validate_rpe, BestKey, RecordMode, SetRecord, SetSegment, SetValidationError, SideValue,
};
use crate::sync::{
    Operation, SetCompletePayload, SetCreatePayload, SetDeletePayload, SetReorderPayload,
    SetSegmentPayload, SetUpdatePayload, SyncError, ValidatedPayload,
};

/// 秒数の上限。1 日とする。
const DURATION_MAX_SECONDS: u32 = 86_400;

/// セットの追加の本文を検証する。
pub(crate) fn validate_create(
    operation: &Operation,
    payload: SetCreatePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    validate_set_fields(
        &payload.set_id,
        Some(&payload.training_exercise_id),
        payload.position,
        payload.rpe,
        payload.body_weight_kg,
        &payload.segments,
    )?;
    Ok(ValidatedPayload::SetCreate(payload))
}

/// セットの更新の本文を検証する。
pub(crate) fn validate_update(
    operation: &Operation,
    payload: SetUpdatePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    validate_set_fields(
        &payload.set_id,
        None,
        payload.position,
        payload.rpe,
        payload.body_weight_kg,
        &payload.segments,
    )?;
    Ok(ValidatedPayload::SetUpdate(payload))
}

/// セットの削除の本文を検証する。
pub(crate) fn validate_delete(
    operation: &Operation,
    payload: SetDeletePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.set_id) {
        return Err(SyncError::InvalidSetId);
    }
    Ok(ValidatedPayload::SetDelete(payload))
}

/// セットの並べ替えの本文を検証する。
pub(crate) fn validate_reorder(
    operation: &Operation,
    payload: SetReorderPayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.training_exercise_id) {
        return Err(SyncError::InvalidExerciseId);
    }
    if payload.ordered_ids.is_empty() {
        return Err(SyncError::InvalidOrderedIds);
    }
    let mut seen = Vec::new();
    for id in &payload.ordered_ids {
        if !is_valid_uuid_v4(id) || seen.contains(id) {
            return Err(SyncError::InvalidOrderedIds);
        }
        seen.push(id.clone());
    }
    Ok(ValidatedPayload::SetReorder(payload))
}

/// セットの完了の状態の変更の本文を検証する。
pub(crate) fn validate_complete(
    operation: &Operation,
    payload: SetCompletePayload,
) -> Result<ValidatedPayload, SyncError> {
    if payload.training_id != operation.training_id {
        return Err(SyncError::PayloadIdMismatch);
    }
    if !is_valid_uuid_v4(&payload.set_id) {
        return Err(SyncError::InvalidSetId);
    }
    Ok(ValidatedPayload::SetComplete(payload))
}

fn validate_set_fields(
    set_id: &str,
    training_exercise_id: Option<&str>,
    position: i64,
    rpe: Option<f64>,
    body_weight_kg: Option<f64>,
    segments: &[SetSegmentPayload],
) -> Result<(), SyncError> {
    if !is_valid_uuid_v4(set_id) {
        return Err(SyncError::InvalidSetId);
    }
    if let Some(training_exercise_id) = training_exercise_id {
        if !is_valid_uuid_v4(training_exercise_id) {
            return Err(SyncError::InvalidExerciseId);
        }
    }
    if position < 1 {
        return Err(SyncError::InvalidPosition);
    }
    if validate_rpe(rpe).is_err() {
        return Err(SyncError::InvalidRpe);
    }
    if body_weight_kg.is_some_and(|weight| !weight.is_finite() || weight <= 0.0) {
        return Err(SyncError::InvalidBodyWeight);
    }
    if segments.is_empty() {
        return Err(SyncError::EmptySegments);
    }
    let mut seen = Vec::new();
    for segment in segments {
        if !is_valid_uuid_v4(&segment.segment_id) || seen.contains(&segment.segment_id) {
            return Err(SyncError::InvalidSegmentId);
        }
        seen.push(segment.segment_id.clone());
        validate_segment(segment)?;
    }
    Ok(())
}

fn validate_segment(segment: &SetSegmentPayload) -> Result<(), SyncError> {
    if segment.position < 1 {
        return Err(SyncError::InvalidPosition);
    }
    if !segment.weight_kg.is_finite() || segment.weight_kg < 0.0 {
        return Err(SyncError::InvalidWeight);
    }
    if let Some(duration) = segment.duration_seconds {
        if !(1..=DURATION_MAX_SECONDS).contains(&duration) {
            return Err(SyncError::InvalidDuration);
        }
    }
    let has_sides = segment.left.is_some() || segment.right.is_some();
    if !has_sides
        && u64::from(segment.momentum_reps) + u64::from(segment.assisted_reps)
            > u64::from(segment.reps)
    {
        return Err(SyncError::InvalidReps);
    }
    for side in [segment.left.as_ref(), segment.right.as_ref()]
        .into_iter()
        .flatten()
    {
        validate_side(side, segment.momentum_reps, segment.assisted_reps)?;
    }
    Ok(())
}

fn validate_side(
    side: &SideValue,
    momentum_reps: u32,
    assisted_reps: u32,
) -> Result<(), SyncError> {
    if !side.weight_kg.is_finite() || side.weight_kg < 0.0 {
        return Err(SyncError::InvalidWeight);
    }
    if let Some(duration) = side.duration_seconds {
        if !(1..=DURATION_MAX_SECONDS).contains(&duration) {
            return Err(SyncError::InvalidDuration);
        }
    }
    if u64::from(momentum_reps) + u64::from(assisted_reps) > u64::from(side.reps) {
        return Err(SyncError::InvalidReps);
    }
    Ok(())
}

impl SetSegmentPayload {
    /// crate::set_calc の段の記録に変換する。自重種目は記録した時点の体重に加重を足す。
    pub fn to_segment(&self, mode: RecordMode, body_weight_kg: Option<f64>) -> SetSegment {
        SetSegment {
            weight_kg: weight_at_record(self.weight_kg, mode, body_weight_kg),
            reps: self.reps,
            momentum_reps: self.momentum_reps,
            assisted_reps: self.assisted_reps,
            left: self.left.clone().map(|side| SideValue {
                weight_kg: weight_at_record(side.weight_kg, mode, body_weight_kg),
                ..side
            }),
            right: self.right.clone().map(|side| SideValue {
                weight_kg: weight_at_record(side.weight_kg, mode, body_weight_kg),
                ..side
            }),
            duration_seconds: self.duration_seconds,
        }
    }

    /// 段が完了に必要な値を持つか。
    ///
    /// 時間で記録する段は時間、それ以外の段は回数が要る。左右別の段は左右の
    /// 両方に値があることを求める。
    pub fn is_complete(&self, mode: RecordMode) -> bool {
        match mode {
            RecordMode::Time => match (self.left.as_ref(), self.right.as_ref()) {
                (Some(left), Some(right)) => {
                    left.duration_seconds.is_some_and(|duration| duration > 0)
                        && right.duration_seconds.is_some_and(|duration| duration > 0)
                }
                (None, None) => self.duration_seconds.is_some_and(|duration| duration > 0),
                (Some(_), None) | (None, Some(_)) => false,
            },
            RecordMode::WeightAndReps | RecordMode::BodyweightAndReps => {
                match (self.left.as_ref(), self.right.as_ref()) {
                    (Some(left), Some(right)) => left.reps > 0 && right.reps > 0,
                    (None, None) => self.reps > 0,
                    (Some(_), None) | (None, Some(_)) => false,
                }
            }
        }
    }
}

impl SetCreatePayload {
    /// crate::set_calc のセットの記録に変換する。
    pub fn to_record(
        &self,
        exercise_id: &str,
        mode: RecordMode,
    ) -> Result<SetRecord, SetValidationError> {
        to_record(
            exercise_id,
            self.kind,
            self.rpe,
            self.grip,
            self.body_weight_kg,
            &self.segments,
            mode,
        )
    }
}

impl SetUpdatePayload {
    /// crate::set_calc のセットの記録に変換する。
    pub fn to_record(
        &self,
        exercise_id: &str,
        mode: RecordMode,
    ) -> Result<SetRecord, SetValidationError> {
        to_record(
            exercise_id,
            self.kind,
            self.rpe,
            self.grip,
            self.body_weight_kg,
            &self.segments,
            mode,
        )
    }
}

fn to_record(
    exercise_id: &str,
    kind: crate::set_calc::SetKind,
    rpe: Option<f64>,
    grip: Option<GripSelection>,
    body_weight_kg: Option<f64>,
    segments: &[SetSegmentPayload],
    mode: RecordMode,
) -> Result<SetRecord, SetValidationError> {
    let (grip, attachment) = match grip {
        None => (None, None),
        Some(selection) => selection.comparison_key(),
    };
    let key = BestKey {
        exercise_id: exercise_id.to_owned(),
        grip,
        attachment,
    };
    let segments = segments
        .iter()
        .map(|segment| segment.to_segment(mode, body_weight_kg))
        .collect();
    SetRecord::new(key, kind, segments, rpe, None, mode)
}

fn weight_at_record(added_weight_kg: f64, mode: RecordMode, body_weight_kg: Option<f64>) -> f64 {
    match mode {
        RecordMode::BodyweightAndReps => {
            bodyweight_set_weight(body_weight_kg, added_weight_kg).unwrap_or(0.0)
        }
        RecordMode::WeightAndReps | RecordMode::Time => added_weight_kg,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        validate_complete, validate_create, validate_delete, validate_reorder, validate_update,
    };
    use crate::exercise::GripSelection;
    use crate::set_calc::{RecordMode, SetKind};
    use crate::sync::{
        Operation, OperationKind, SetCompletePayload, SetCreatePayload, SetDeletePayload,
        SetReorderPayload, SetSegmentPayload, SetUpdatePayload, SyncError, ValidatedPayload,
    };

    const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const OPERATION_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3302";
    const TRAINING_EXERCISE_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3303";
    const SET_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3304";
    const SEGMENT_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3305";
    const OTHER_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3306";

    fn operation(kind: OperationKind, payload: Value) -> Operation {
        Operation {
            operation_id: OPERATION_ID.to_owned(),
            training_id: TRAINING_ID.to_owned(),
            sequence: 2,
            kind,
            parent_operation_id: None,
            payload,
        }
    }

    fn segment() -> SetSegmentPayload {
        SetSegmentPayload {
            segment_id: SEGMENT_ID.to_owned(),
            position: 1,
            weight_kg: 100.0,
            reps: 5,
            momentum_reps: 0,
            assisted_reps: 0,
            duration_seconds: None,
            left: None,
            right: None,
        }
    }

    fn create_payload() -> SetCreatePayload {
        SetCreatePayload {
            training_id: TRAINING_ID.to_owned(),
            training_exercise_id: TRAINING_EXERCISE_ID.to_owned(),
            set_id: SET_ID.to_owned(),
            position: 1,
            kind: SetKind::Normal,
            rpe: None,
            grip: None,
            body_weight_kg: Some(70.0),
            segments: vec![segment()],
        }
    }

    fn create_operation() -> Operation {
        operation(
            OperationKind::SetCreate,
            serde_json::to_value(create_payload()).expect("a create payload"),
        )
    }

    fn create_from(operation: &Operation) -> SetCreatePayload {
        serde_json::from_value(operation.payload.clone()).expect("a create payload")
    }

    fn update_payload() -> SetUpdatePayload {
        SetUpdatePayload {
            training_id: TRAINING_ID.to_owned(),
            set_id: SET_ID.to_owned(),
            position: 1,
            kind: SetKind::Warmup,
            completed: false,
            rpe: Some(8.0),
            grip: None,
            body_weight_kg: Some(70.0),
            segments: vec![segment()],
        }
    }

    fn update_operation() -> Operation {
        operation(
            OperationKind::SetUpdate,
            serde_json::to_value(update_payload()).expect("an update payload"),
        )
    }

    fn update_from(operation: &Operation) -> SetUpdatePayload {
        serde_json::from_value(operation.payload.clone()).expect("an update payload")
    }

    #[test]
    fn a_valid_create_passes_and_keeps_the_segments() {
        let operation = create_operation();
        let validated =
            validate_create(&operation, create_from(&operation)).expect("a valid set create");
        let ValidatedPayload::SetCreate(payload) = validated else {
            panic!("the payload must be a set create");
        };
        assert_eq!(payload.set_id, SET_ID);
        assert_eq!(payload.training_exercise_id, TRAINING_EXERCISE_ID);
        assert_eq!(payload.kind, SetKind::Normal);
        assert_eq!(payload.body_weight_kg, Some(70.0));
        assert_eq!(payload.segments, vec![segment()]);
    }

    #[test]
    fn a_create_with_a_different_training_is_rejected() {
        let mut operation = create_operation();
        operation.payload["training_id"] = json!(OTHER_ID);
        assert_eq!(
            validate_create(&operation, create_from(&operation)),
            Err(SyncError::PayloadIdMismatch)
        );
    }

    #[test]
    fn a_set_id_and_a_training_exercise_id_must_be_uuid_v4() {
        let mut operation = create_operation();
        operation.payload["set_id"] = json!("set-1");
        assert_eq!(
            validate_create(&operation, create_from(&operation)),
            Err(SyncError::InvalidSetId)
        );

        let mut operation = create_operation();
        operation.payload["training_exercise_id"] = json!("exercise-1");
        assert_eq!(
            validate_create(&operation, create_from(&operation)),
            Err(SyncError::InvalidExerciseId)
        );
    }

    #[test]
    fn a_segment_id_must_be_a_uuid_v4_and_unique() {
        let mut operation = create_operation();
        operation.payload["segments"][0]["segment_id"] = json!("segment-1");
        assert_eq!(
            validate_create(&operation, create_from(&operation)),
            Err(SyncError::InvalidSegmentId)
        );

        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.segments.push(segment());
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::InvalidSegmentId)
        );
    }

    #[test]
    fn a_position_below_one_is_rejected_for_the_set_and_the_segment() {
        let mut operation = create_operation();
        operation.payload["position"] = json!(0);
        assert_eq!(
            validate_create(&operation, create_from(&operation)),
            Err(SyncError::InvalidPosition)
        );

        let mut operation = create_operation();
        operation.payload["segments"][0]["position"] = json!(0);
        assert_eq!(
            validate_create(&operation, create_from(&operation)),
            Err(SyncError::InvalidPosition)
        );
    }

    #[test]
    fn an_empty_segment_list_is_rejected() {
        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.segments.clear();
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::EmptySegments)
        );
    }

    #[test]
    fn an_invalid_weight_rpe_and_duration_are_rejected() {
        for weight in [-1.0, f64::NAN, f64::INFINITY] {
            let operation = create_operation();
            let mut payload = create_from(&operation);
            payload.segments[0].weight_kg = weight;
            assert_eq!(
                validate_create(&operation, payload),
                Err(SyncError::InvalidWeight)
            );
        }

        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.rpe = Some(7.25);
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::InvalidRpe)
        );

        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.segments[0].duration_seconds = Some(0);
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::InvalidDuration)
        );
    }

    #[test]
    fn invalid_body_weight_is_rejected() {
        for weight in [Some(0.0), Some(-1.0), Some(f64::NAN)] {
            let operation = create_operation();
            let mut payload = create_from(&operation);
            payload.body_weight_kg = weight;
            assert_eq!(
                validate_create(&operation, payload),
                Err(SyncError::InvalidBodyWeight)
            );
        }
        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.body_weight_kg = None;
        let validated = validate_create(&operation, payload).expect("a valid create");
        let ValidatedPayload::SetCreate(payload) = validated else {
            panic!("the payload must be a set create");
        };
        assert_eq!(payload.body_weight_kg, None);
    }

    #[test]
    fn assisted_reps_cannot_exceed_the_reps() {
        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.segments[0].momentum_reps = 3;
        payload.segments[0].assisted_reps = 3;
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::InvalidReps)
        );
    }

    #[test]
    fn sides_validate_their_own_reps_and_durations() {
        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.segments[0].left = Some(crate::set_calc::SideValue {
            weight_kg: 10.0,
            reps: 2,
            duration_seconds: None,
        });
        payload.segments[0].momentum_reps = 2;
        payload.segments[0].assisted_reps = 2;
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::InvalidReps)
        );

        let operation = create_operation();
        let mut payload = create_from(&operation);
        payload.segments[0].right = Some(crate::set_calc::SideValue {
            weight_kg: 10.0,
            reps: 5,
            duration_seconds: Some(0),
        });
        assert_eq!(
            validate_create(&operation, payload),
            Err(SyncError::InvalidDuration)
        );
    }

    #[test]
    fn a_valid_update_passes_with_all_segments_replaced() {
        let operation = update_operation();
        let validated =
            validate_update(&operation, update_from(&operation)).expect("a valid set update");
        let ValidatedPayload::SetUpdate(payload) = validated else {
            panic!("the payload must be a set update");
        };
        assert_eq!(payload.set_id, SET_ID);
        assert_eq!(payload.kind, SetKind::Warmup);
        assert_eq!(payload.rpe, Some(8.0));
    }

    #[test]
    fn a_valid_delete_and_complete_pass() {
        let delete = operation(
            OperationKind::SetDelete,
            json!({"training_id": TRAINING_ID, "set_id": SET_ID}),
        );
        let payload: SetDeletePayload =
            serde_json::from_value(delete.payload.clone()).expect("a delete payload");
        let validated = validate_delete(&delete, payload).expect("a valid delete");
        let ValidatedPayload::SetDelete(payload) = validated else {
            panic!("the payload must be a set delete");
        };
        assert_eq!(payload.training_id, TRAINING_ID);
        assert_eq!(payload.set_id, SET_ID);

        let complete = operation(
            OperationKind::SetComplete,
            json!({"training_id": TRAINING_ID, "set_id": SET_ID, "completed": true}),
        );
        let payload: SetCompletePayload =
            serde_json::from_value(complete.payload.clone()).expect("a complete payload");
        let validated = validate_complete(&complete, payload).expect("a valid complete");
        let ValidatedPayload::SetComplete(payload) = validated else {
            panic!("the payload must be a set complete");
        };
        assert_eq!(payload.set_id, SET_ID);
        assert!(payload.completed);
    }

    #[test]
    fn a_reorder_needs_unique_uuid_ids() {
        let reorder = operation(
            OperationKind::SetReorder,
            json!({
                "training_id": TRAINING_ID,
                "training_exercise_id": TRAINING_EXERCISE_ID,
                "ordered_ids": [SET_ID, OTHER_ID]
            }),
        );
        let payload: SetReorderPayload =
            serde_json::from_value(reorder.payload.clone()).expect("a reorder payload");
        let validated = validate_reorder(&reorder, payload).expect("a valid reorder");
        let ValidatedPayload::SetReorder(payload) = validated else {
            panic!("the payload must be a set reorder");
        };
        assert_eq!(payload.ordered_ids, vec![SET_ID, OTHER_ID]);

        let duplicated = operation(
            OperationKind::SetReorder,
            json!({
                "training_id": TRAINING_ID,
                "training_exercise_id": TRAINING_EXERCISE_ID,
                "ordered_ids": [SET_ID, SET_ID]
            }),
        );
        let payload: SetReorderPayload =
            serde_json::from_value(duplicated.payload.clone()).expect("a reorder payload");
        assert_eq!(
            validate_reorder(&duplicated, payload),
            Err(SyncError::InvalidOrderedIds)
        );
    }

    #[test]
    fn a_payload_becomes_a_set_record_for_weight_reps() {
        let payload = create_payload();
        let record = payload
            .to_record("bench-press", RecordMode::WeightAndReps)
            .expect("a record");
        assert_eq!(record.kind(), SetKind::Normal);
        assert_eq!(record.total_weight_kg(), 500.0);
        assert_eq!(record.max_weight_kg(), Some(100.0));
        assert_eq!(record.segments().len(), 1);
        assert_eq!(
            record.key(),
            &crate::set_calc::BestKey {
                exercise_id: "bench-press".to_owned(),
                grip: None,
                attachment: None,
            }
        );
    }

    #[test]
    fn a_bodyweight_payload_adds_the_recorded_body_weight() {
        let payload = create_payload();
        let record = payload
            .to_record("push-up", RecordMode::BodyweightAndReps)
            .expect("a record");
        assert_eq!(record.total_weight_kg(), 850.0);
        assert_eq!(record.max_weight_kg(), Some(170.0));
    }

    #[test]
    fn a_bodyweight_payload_without_a_body_weight_has_no_weight() {
        let mut payload = create_payload();
        payload.body_weight_kg = None;
        let record = payload
            .to_record("push-up", RecordMode::BodyweightAndReps)
            .expect("a record");
        assert_eq!(record.total_weight_kg(), 0.0);
        assert_eq!(record.max_weight_kg(), None);
        assert_eq!(record.estimated_one_rep_max(), None);
    }

    #[test]
    fn a_time_payload_keeps_the_duration() {
        let payload = SetCreatePayload {
            training_id: TRAINING_ID.to_owned(),
            training_exercise_id: TRAINING_EXERCISE_ID.to_owned(),
            set_id: SET_ID.to_owned(),
            position: 1,
            kind: SetKind::Normal,
            rpe: None,
            grip: None,
            body_weight_kg: None,
            segments: vec![SetSegmentPayload {
                segment_id: SEGMENT_ID.to_owned(),
                position: 1,
                weight_kg: 0.0,
                reps: 0,
                momentum_reps: 0,
                assisted_reps: 0,
                duration_seconds: Some(90),
                left: None,
                right: None,
            }],
        };
        let record = payload
            .to_record("plank", RecordMode::Time)
            .expect("a record");
        assert_eq!(record.longest_duration_seconds(), Some(90));
        assert_eq!(record.total_duration_seconds(), 90);
    }

    #[test]
    fn a_grip_selection_becomes_the_comparison_key() {
        let mut payload = create_payload();
        payload.grip = Some(GripSelection::Grip {
            style: crate::exercise::GripStyle::Pronated,
            width: crate::exercise::GripWidth::Medium,
        });
        let record = payload
            .to_record("bench-press", RecordMode::WeightAndReps)
            .expect("a record");
        assert_eq!(record.key().grip.as_deref(), Some("pronated_medium"));
        assert_eq!(record.key().attachment, None);
    }

    #[test]
    fn segments_report_whether_they_are_complete() {
        let mut empty = segment();
        empty.reps = 0;
        empty.weight_kg = 0.0;
        assert!(!empty.is_complete(RecordMode::WeightAndReps));
        assert!(segment().is_complete(RecordMode::WeightAndReps));

        let mut one_side = segment();
        one_side.reps = 0;
        one_side.left = Some(crate::set_calc::SideValue {
            weight_kg: 10.0,
            reps: 5,
            duration_seconds: None,
        });
        assert!(!one_side.is_complete(RecordMode::WeightAndReps));

        one_side.right = Some(crate::set_calc::SideValue {
            weight_kg: 10.0,
            reps: 4,
            duration_seconds: None,
        });
        assert!(one_side.is_complete(RecordMode::WeightAndReps));

        let mut empty_side = one_side.clone();
        empty_side.right = Some(crate::set_calc::SideValue {
            weight_kg: 10.0,
            reps: 0,
            duration_seconds: None,
        });
        assert!(!empty_side.is_complete(RecordMode::WeightAndReps));

        let mut time = segment();
        time.weight_kg = 0.0;
        time.reps = 0;
        time.duration_seconds = Some(30);
        assert!(time.is_complete(RecordMode::Time));
        assert!(!time.is_complete(RecordMode::WeightAndReps));

        let mut time_empty = time.clone();
        time_empty.duration_seconds = None;
        assert!(!time_empty.is_complete(RecordMode::Time));

        let mut time_side = time.clone();
        time_side.duration_seconds = None;
        time_side.left = Some(crate::set_calc::SideValue {
            weight_kg: 0.0,
            reps: 0,
            duration_seconds: Some(20),
        });
        assert!(!time_side.is_complete(RecordMode::Time));
        time_side.right = Some(crate::set_calc::SideValue {
            weight_kg: 0.0,
            reps: 0,
            duration_seconds: Some(30),
        });
        assert!(time_side.is_complete(RecordMode::Time));

        let mut no_duration = segment();
        no_duration.duration_seconds = None;
        assert!(!no_duration.is_complete(RecordMode::Time));
    }
}
