//! ドロップセットの段の数と集計の性質テスト。
//!
//! 対応する要求: `REQ-SET-003`、`REQ-SET-009`。
//! 段の数が変わってもセット数は 1 であり、総重量は全段、最大重量と推定 1RM は
//! 最初の段だけを対象にすることを、warrior_core::set_calc の計算規則と照らす。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::set_calc::{
    estimated_one_rep_max, BestKey, PartRole, PersonalBest, RecordMode, SetKind, SetRecord,
    SetSegment,
};
use warrior_core::set_rules::drop_set_positions_valid;

fn key() -> BestKey {
    BestKey {
        exercise_id: "bench-press".to_owned(),
        grip: None,
        attachment: None,
    }
}

fn date() -> JstDate {
    JstDate::new(2026, 11, 12).unwrap()
}

fn kind() -> impl Strategy<Value = SetKind> {
    prop_oneof![
        Just(SetKind::Normal),
        Just(SetKind::Warmup),
        Just(SetKind::Drop),
    ]
}

fn segment() -> impl Strategy<Value = SetSegment> {
    (0.5f64..=500.0, 1u32..=20).prop_map(|(weight_kg, reps)| SetSegment {
        weight_kg,
        reps,
        momentum_reps: 0,
        assisted_reps: 0,
        left: None,
        right: None,
        duration_seconds: None,
    })
}

fn drop_record(segments: Vec<SetSegment>) -> SetRecord {
    SetRecord::new(
        key(),
        SetKind::Drop,
        segments,
        None,
        None,
        RecordMode::WeightAndReps,
    )
    .expect("a valid drop set")
}

fn valid_kinds() -> impl Strategy<Value = Vec<SetKind>> {
    prop::collection::vec(kind(), 0..=8).prop_map(|kinds| {
        let mut valid = Vec::with_capacity(kinds.len());
        for kind in kinds {
            let previous = valid.last().copied();
            let kind = if kind == SetKind::Drop
                && !matches!(previous, Some(SetKind::Normal | SetKind::Drop))
            {
                SetKind::Normal
            } else {
                kind
            };
            valid.push(kind);
        }
        valid
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn valid_drop_sequences_are_accepted(kinds in valid_kinds()) {
        prop_assert!(drop_set_positions_valid(&kinds));
    }

    #[test]
    fn a_first_drop_set_is_rejected(rest in prop::collection::vec(kind(), 0..=7)) {
        let mut kinds = vec![SetKind::Drop];
        kinds.extend(rest);
        prop_assert!(!drop_set_positions_valid(&kinds));
    }

    #[test]
    fn a_drop_set_after_a_warmup_is_rejected(
        prefix in prop::collection::vec(kind(), 0..=3),
        suffix in prop::collection::vec(kind(), 0..=3),
    ) {
        let mut kinds = prefix;
        kinds.push(SetKind::Warmup);
        kinds.push(SetKind::Drop);
        kinds.extend(suffix);
        prop_assert!(!drop_set_positions_valid(&kinds));
    }

    #[test]
    fn breaking_one_position_of_a_valid_sequence_is_rejected(
        kinds in valid_kinds().prop_filter("a non-empty sequence", |kinds| !kinds.is_empty()),
    ) {
        let mut broken = kinds;
        broken[0] = SetKind::Drop;
        prop_assert!(!drop_set_positions_valid(&broken));
    }

    #[test]
    fn a_drop_set_counts_as_one_set_whatever_the_segment_count(
        segments in prop::collection::vec(segment(), 1..=8),
    ) {
        let record = drop_record(segments);
        prop_assert_eq!(record.counted_sets(), 1.0);
        prop_assert_eq!(record.part_set_count(PartRole::Primary), 1.0);
        prop_assert_eq!(record.part_set_count(PartRole::Assisted), 0.5);
        prop_assert_eq!(record.part_set_count(PartRole::Unrelated), 0.0);
    }

    #[test]
    fn a_drop_set_sums_every_segment_into_the_total_weight(
        segments in prop::collection::vec(segment(), 1..=8),
    ) {
        let expected = segments
            .iter()
            .map(|segment| segment.weight_kg * f64::from(segment.reps))
            .sum::<f64>();
        let record = drop_record(segments);
        prop_assert!((record.total_weight_kg() - expected).abs() < 1e-9);
        prop_assert!((record.strict_total_weight_kg() - expected).abs() < 1e-9);
    }

    #[test]
    fn a_drop_set_uses_the_first_segment_for_the_best_values(
        segments in prop::collection::vec(segment(), 1..=8),
    ) {
        let first = segments[0].clone();
        let expected_total = segments
            .iter()
            .map(|segment| segment.weight_kg * f64::from(segment.reps))
            .sum::<f64>();
        let record = drop_record(segments);
        prop_assert_eq!(record.max_weight_kg(), first.max_weight_kg());
        prop_assert_eq!(record.estimated_one_rep_max(), first.estimated_one_rep_max());
        prop_assert_eq!(
            record.estimated_one_rep_max(),
            estimated_one_rep_max(first.weight_kg, first.reps)
        );

        let mut best = PersonalBest::new(key());
        let updates = best.update(&record, date());
        prop_assert!(updates.max_weight);
        prop_assert!(updates.estimated_one_rep_max);
        prop_assert!(updates.total_weight);
        prop_assert_eq!(best.max_weight().map(|value| value.value), first.max_weight_kg());
        prop_assert_eq!(
            best.estimated_one_rep_max().map(|value| value.value),
            first.estimated_one_rep_max()
        );
        prop_assert_eq!(
            best.total_weight().map(|value| value.value),
            Some(expected_total)
        );
    }
}
