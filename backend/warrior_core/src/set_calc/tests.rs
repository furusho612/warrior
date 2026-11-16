//! 対応する要求: `REQ-SET-004`。

use super::{
    estimated_one_rep_max, is_reference_one_rep_max, strict_reps, BestKey, PartRole, PersonalBest,
    RecordMode, SetKind, SetRecord, SetSegment, SetValidationError, Side, SideValue,
};
use crate::jst::JstDate;

fn key() -> BestKey {
    BestKey {
        exercise_id: "bench-press".to_owned(),
        grip: None,
        attachment: None,
    }
}

fn date(day: i64) -> JstDate {
    JstDate::new(2026, 11, day as u32).unwrap()
}

fn segment(weight: f64, reps: u32) -> SetSegment {
    SetSegment {
        weight_kg: weight,
        reps,
        momentum_reps: 0,
        assisted_reps: 0,
        left: None,
        right: None,
        duration_seconds: None,
    }
}

fn record(kind: SetKind, segments: Vec<SetSegment>) -> SetRecord {
    SetRecord::new(key(), kind, segments, None, None, RecordMode::WeightAndReps).unwrap()
}

fn unilateral_segment(
    left: (f64, u32),
    right: (f64, u32),
    momentum_reps: u32,
    assisted_reps: u32,
) -> SetSegment {
    SetSegment {
        weight_kg: 0.0,
        reps: 0,
        momentum_reps,
        assisted_reps,
        left: Some(SideValue {
            weight_kg: left.0,
            reps: left.1,
            duration_seconds: None,
        }),
        right: Some(SideValue {
            weight_kg: right.0,
            reps: right.1,
            duration_seconds: None,
        }),
        duration_seconds: None,
    }
}

#[test]
fn strict_reps_subtracts_momentum_and_assistance() {
    assert_eq!(strict_reps(10, 3, 2), 5);
    assert_eq!(strict_reps(3, 2, 2), 0);
}

#[test]
fn one_rep_max_for_one_rep_is_the_weight() {
    assert_eq!(estimated_one_rep_max(100.0, 1), Some(100.0));
}

#[test]
fn one_rep_max_uses_the_formula() {
    let value = estimated_one_rep_max(100.0, 5).unwrap();
    let expected = 100.0 * 100.0 / (48.8 + 53.8 * (-0.075 * 5.0f64).exp());
    assert!((value - expected).abs() < 1e-9);
}

#[test]
fn one_rep_max_marks_more_than_ten_reps_as_reference() {
    assert!(!is_reference_one_rep_max(10));
    assert!(is_reference_one_rep_max(11));
}

#[test]
fn record_one_rep_max_carries_the_reference_flag() {
    let ten = record(SetKind::Normal, vec![segment(100.0, 10)]);
    assert!(!ten.one_rep_max().unwrap().is_reference);
    let eleven = record(SetKind::Normal, vec![segment(100.0, 11)]);
    assert!(eleven.one_rep_max().unwrap().is_reference);
}

#[test]
fn unilateral_zero_weight_does_not_create_a_personal_best() {
    let record = record(
        SetKind::Normal,
        vec![unilateral_segment((0.0, 5), (0.0, 5), 0, 0)],
    );
    assert_eq!(record.max_weight_kg(), None);
}

#[test]
fn one_rep_max_rejects_zero_or_negative_weight_and_zero_reps() {
    assert_eq!(estimated_one_rep_max(0.0, 5), None);
    assert_eq!(estimated_one_rep_max(-1.0, 5), None);
    assert_eq!(estimated_one_rep_max(100.0, 0), None);
}

#[test]
fn total_weight_excludes_assistance_but_includes_momentum() {
    let mut segment = segment(100.0, 10);
    segment.momentum_reps = 3;
    segment.assisted_reps = 2;
    let record = record(SetKind::Normal, vec![segment]);

    assert_eq!(record.total_weight_kg(), 800.0);
    assert_eq!(record.strict_total_weight_kg(), 500.0);
}

#[test]
fn warmup_is_excluded_from_weight_and_personal_best() {
    let record = record(SetKind::Warmup, vec![segment(100.0, 10)]);
    assert_eq!(record.total_weight_kg(), 0.0);
    assert_eq!(record.max_weight_kg(), None);
    assert_eq!(record.estimated_one_rep_max(), None);
    assert_eq!(record.counted_sets(), 0.0);
}

#[test]
fn warmup_weight_does_not_update_the_personal_best() {
    let mut best = PersonalBest::new(key());
    let updates = best.update(&record(SetKind::Warmup, vec![segment(100.0, 10)]), date(12));
    assert_eq!(updates, Default::default());
    assert!(best.max_weight().is_none());
    assert!(best.estimated_one_rep_max().is_none());
    assert!(best.total_weight().is_none());
    assert!(best.longest_duration().is_none());
}

#[test]
fn warmup_time_does_not_update_the_longest_duration_best() {
    let mut best = PersonalBest::new(key());
    let record = SetRecord::new(
        key(),
        SetKind::Warmup,
        Vec::new(),
        None,
        Some(90),
        RecordMode::Time,
    )
    .unwrap();
    let updates = best.update(&record, date(12));
    assert_eq!(updates, Default::default());
    assert!(best.longest_duration().is_none());
}

#[test]
fn drop_sets_include_all_segments_in_total_but_only_the_first_in_best() {
    let record = record(SetKind::Drop, vec![segment(100.0, 10), segment(80.0, 8)]);
    assert_eq!(record.total_weight_kg(), 1_000.0 + 640.0);
    assert_eq!(record.max_weight_kg(), Some(100.0));
    assert_eq!(
        record.estimated_one_rep_max(),
        estimated_one_rep_max(100.0, 10)
    );
    assert_eq!(record.counted_sets(), 1.0);
}

#[test]
fn req_set_004_a_unilateral_segment_needs_both_sides() {
    let segment = SetSegment {
        weight_kg: 0.0,
        reps: 0,
        momentum_reps: 0,
        assisted_reps: 0,
        left: Some(SideValue {
            weight_kg: 10.0,
            reps: 5,
            duration_seconds: None,
        }),
        right: None,
        duration_seconds: None,
    };
    assert_eq!(
        SetRecord::new(
            key(),
            SetKind::Normal,
            vec![segment],
            None,
            None,
            RecordMode::WeightAndReps,
        ),
        Err(SetValidationError::MissingSide)
    );
}

#[test]
fn req_set_004_a_unilateral_drop_set_sums_both_sides_of_every_segment() {
    let record = record(
        SetKind::Drop,
        vec![
            unilateral_segment((10.0, 5), (12.0, 4), 0, 0),
            unilateral_segment((8.0, 6), (9.0, 5), 0, 0),
        ],
    );
    assert_eq!(record.total_weight_kg(), 50.0 + 48.0 + 48.0 + 45.0);
    assert_eq!(record.max_weight_kg(), Some(12.0));
    assert_eq!(
        record.estimated_one_rep_max(),
        estimated_one_rep_max(12.0, 4)
    );
}

#[test]
fn req_set_004_unilateral_weight_sums_both_sides() {
    let record = record(
        SetKind::Normal,
        vec![unilateral_segment((10.0, 5), (12.0, 4), 0, 0)],
    );
    assert_eq!(record.total_weight_kg(), 50.0 + 48.0);
    assert_eq!(record.max_weight_kg(), Some(12.0));
    assert_eq!(
        record.estimated_one_rep_max(),
        estimated_one_rep_max(12.0, 4)
    );
}

#[test]
fn unilateral_breakdown_applies_to_each_side() {
    let record = record(
        SetKind::Normal,
        vec![unilateral_segment((10.0, 10), (20.0, 8), 2, 1)],
    );
    let segment = &record.segments()[0];
    assert_eq!(segment.side(Side::Left).unwrap().strict_reps(2, 1), 7);
    assert_eq!(segment.side(Side::Right).unwrap().strict_reps(2, 1), 5);
    assert_eq!(record.total_weight_kg(), 230.0);
    assert_eq!(record.strict_total_weight_kg(), 170.0);
    assert_eq!(
        record.estimated_one_rep_max(),
        estimated_one_rep_max(20.0, 5)
    );
}

#[test]
fn unilateral_assistance_must_fit_each_side() {
    let accepted = SetRecord::new(
        key(),
        SetKind::Normal,
        vec![unilateral_segment((10.0, 5), (20.0, 4), 0, 4)],
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .expect("assistance equal to the smaller side");
    assert_eq!(accepted.total_weight_kg(), 10.0);
    assert_eq!(accepted.strict_total_weight_kg(), 10.0);
    assert_eq!(
        accepted.segments()[0]
            .side(Side::Right)
            .unwrap()
            .strict_reps(0, 4),
        0
    );

    let rejected = SetRecord::new(
        key(),
        SetKind::Normal,
        vec![unilateral_segment((10.0, 5), (20.0, 4), 0, 5)],
        None,
        None,
        RecordMode::WeightAndReps,
    );
    assert_eq!(rejected, Err(SetValidationError::TooManyAssistedReps));
}

#[test]
fn req_set_004_unilateral_time_uses_the_max_and_the_sum() {
    let record = SetRecord::new(
        key(),
        SetKind::Normal,
        vec![SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(30),
            }),
            right: Some(SideValue {
                weight_kg: 0.0,
                reps: 0,
                duration_seconds: Some(40),
            }),
            duration_seconds: None,
        }],
        None,
        None,
        RecordMode::Time,
    )
    .unwrap();
    assert_eq!(record.longest_duration_seconds(), Some(40));
    assert_eq!(record.total_duration_seconds(), 70);
    assert_eq!(record.counted_sets(), 1.0);
    assert_eq!(record.total_weight_kg(), 0.0);
    assert_eq!(record.estimated_one_rep_max(), None);
}

#[test]
fn time_sets_are_excluded_from_weight_and_counted_as_one_set() {
    let record = SetRecord::new(
        key(),
        SetKind::Normal,
        Vec::new(),
        None,
        Some(60),
        RecordMode::Time,
    )
    .unwrap();
    assert_eq!(record.total_weight_kg(), 0.0);
    assert_eq!(record.max_weight_kg(), None);
    assert_eq!(record.longest_duration_seconds(), Some(60));
    assert_eq!(record.part_set_count(PartRole::Primary), 1.0);
}

#[test]
fn time_cannot_store_duration_on_both_the_record_and_a_segment() {
    let result = SetRecord::new(
        key(),
        SetKind::Normal,
        vec![SetSegment {
            weight_kg: 0.0,
            reps: 0,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: Some(30),
        }],
        None,
        Some(60),
        RecordMode::Time,
    );
    assert_eq!(result, Err(SetValidationError::ConflictingDuration));
}

#[test]
fn assisted_part_counts_half_only_for_ai_reference() {
    let record = record(SetKind::Normal, vec![segment(100.0, 10)]);
    assert_eq!(record.part_set_count(PartRole::Primary), 1.0);
    assert_eq!(record.part_set_count(PartRole::Assisted), 0.5);
    assert_eq!(record.part_set_count(PartRole::Unrelated), 0.0);
}

#[test]
fn personal_best_updates_each_target() {
    let mut best = PersonalBest::new(key());
    let updates = best.update(
        &record(SetKind::Drop, vec![segment(100.0, 10), segment(80.0, 8)]),
        date(12),
    );
    assert!(updates.max_weight);
    assert!(updates.estimated_one_rep_max);
    assert!(updates.total_weight);
    assert!(!updates.longest_duration);
    assert_eq!(best.max_weight().unwrap().value, 100.0);
    assert_eq!(best.max_weight().unwrap().achieved_on, date(12));
    assert_eq!(best.total_weight().unwrap().value, 1_640.0);
}

#[test]
fn personal_best_does_not_update_when_the_key_differs() {
    let mut best = PersonalBest::new(key());
    let mut other = key();
    other.grip = Some("wide".to_owned());
    let record = SetRecord::new(
        other,
        SetKind::Normal,
        vec![segment(100.0, 10)],
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .unwrap();

    assert_eq!(best.update(&record, date(12)), Default::default());
    assert!(best.max_weight().is_none());
}

#[test]
fn time_personal_best_uses_the_longest_duration() {
    let mut best = PersonalBest::new(key());
    let record = SetRecord::new(
        key(),
        SetKind::Normal,
        Vec::new(),
        None,
        Some(90),
        RecordMode::Time,
    )
    .unwrap();
    let updates = best.update(&record, date(12));
    assert!(updates.longest_duration);
    assert_eq!(best.longest_duration().unwrap().value, 90);
}

#[test]
fn validation_accepts_breakdown_equal_to_the_reps() {
    let mut segment = segment(100.0, 5);
    segment.momentum_reps = 3;
    segment.assisted_reps = 2;
    let record = SetRecord::new(
        key(),
        SetKind::Normal,
        vec![segment],
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .expect("a set whose breakdown equals the reps");
    assert_eq!(record.segments()[0].strict_reps(), 0);
    assert_eq!(record.total_weight_kg(), 300.0);
    assert_eq!(record.strict_total_weight_kg(), 0.0);
    assert_eq!(record.estimated_one_rep_max(), None);
}

#[test]
fn validation_rejects_too_many_assisted_reps() {
    let mut segment = segment(100.0, 5);
    segment.momentum_reps = 3;
    segment.assisted_reps = 3;
    assert_eq!(
        SetRecord::new(
            key(),
            SetKind::Normal,
            vec![segment],
            None,
            None,
            RecordMode::WeightAndReps,
        ),
        Err(SetValidationError::TooManyAssistedReps)
    );
}

#[test]
fn validation_accepts_an_empty_rpe() {
    let record = SetRecord::new(
        key(),
        SetKind::Normal,
        vec![segment(100.0, 5)],
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .expect("an empty RPE is allowed");
    assert_eq!(record.rpe(), None);
}

#[test]
fn validation_rejects_invalid_rpe() {
    for rpe in [5.5, 10.5, 7.25, 6.25] {
        assert_eq!(
            SetRecord::new(
                key(),
                SetKind::Normal,
                vec![segment(100.0, 5)],
                Some(rpe),
                None,
                RecordMode::WeightAndReps,
            ),
            Err(SetValidationError::InvalidRpe)
        );
    }
}

#[test]
fn validation_accepts_rpe_of_six_to_ten_in_half_steps() {
    for rpe in [6.0, 6.5, 7.5, 10.0] {
        assert!(SetRecord::new(
            key(),
            SetKind::Normal,
            vec![segment(100.0, 5)],
            Some(rpe),
            None,
            RecordMode::WeightAndReps,
        )
        .is_ok());
    }
}

#[test]
fn validation_rejects_time_without_duration() {
    assert_eq!(
        SetRecord::new(
            key(),
            SetKind::Normal,
            Vec::new(),
            None,
            None,
            RecordMode::Time,
        ),
        Err(SetValidationError::MissingDuration)
    );
    assert_eq!(
        SetRecord::new(
            key(),
            SetKind::Normal,
            Vec::new(),
            None,
            Some(0),
            RecordMode::Time,
        ),
        Err(SetValidationError::InvalidDuration)
    );
}

#[test]
fn side_lookup_returns_the_requested_side() {
    let segment = SetSegment {
        weight_kg: 0.0,
        reps: 0,
        momentum_reps: 0,
        assisted_reps: 0,
        left: Some(SideValue {
            weight_kg: 10.0,
            reps: 5,
            duration_seconds: None,
        }),
        right: None,
        duration_seconds: None,
    };
    assert_eq!(segment.side(Side::Left).unwrap().weight_kg, 10.0);
    assert!(segment.side(Side::Right).is_none());
}
