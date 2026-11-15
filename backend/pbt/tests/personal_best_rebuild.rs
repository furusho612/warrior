//! 過去の記録の編集と削除の後の自己ベストの計算し直しの性質テスト。
//!
//! 対応する要求: `REQ-REC-006`、`REQ-SET-009`、`REQ-SET-011`。
//! 影響する組だけを計算し直した結果が、残っている全ての記録から計算し直した値と一致し、
//! 組の外の行を変えず、比較の単位ごとに 1 行だけを持つことを確かめる。

use proptest::prelude::*;
use warrior_core::jst::JstDate;
use warrior_core::personal_best::{
    affected_groups, recompute, recompute_groups, reconcile, BestGroup, PersonalBestEntry,
    PersonalBestKey, TrainingSets,
};
use warrior_core::set_calc::{BestKey, RecordMode, SetKind, SetRecord, SetSegment};
use warrior_core::training_session::SessionSet;

fn key() -> impl Strategy<Value = BestKey> {
    (0u8..=2, 0u8..=2, 0u8..=1).prop_map(|(exercise, grip, attachment)| BestKey {
        exercise_id: format!("exercise-{exercise}"),
        grip: match grip {
            0 => None,
            1 => Some("pronated_medium".to_owned()),
            _ => Some("parallel_wide".to_owned()),
        },
        attachment: match attachment {
            0 => None,
            _ => Some("wide_bar".to_owned()),
        },
    })
}

fn segment() -> impl Strategy<Value = SetSegment> {
    (0.5f64..=200.0, 1u32..=20).prop_map(|(weight_kg, reps)| SetSegment {
        weight_kg,
        reps,
        momentum_reps: 0,
        assisted_reps: 0,
        left: None,
        right: None,
        duration_seconds: None,
    })
}

fn session_set() -> impl Strategy<Value = SessionSet> {
    (key(), 0u8..=2, prop::collection::vec(segment(), 1..=3)).prop_map(|(key, kind, segments)| {
        let kind = match kind {
            0 => SetKind::Normal,
            1 => SetKind::Warmup,
            _ => SetKind::Drop,
        };
        let record = SetRecord::new(key, kind, segments, None, None, RecordMode::WeightAndReps)
            .expect("the generated set is valid");
        SessionSet {
            record,
            exercise_name: "種目".to_owned(),
            laterality: warrior_core::exercise::Laterality::Bilateral,
        }
    })
}

fn training() -> impl Strategy<Value = Vec<SessionSet>> {
    prop::collection::vec(session_set(), 0..=4)
}

fn date(index: usize) -> JstDate {
    JstDate::new(2026, 11, 1 + u32::try_from(index).unwrap_or(0)).expect("a valid date")
}

fn training_id(index: usize) -> String {
    format!("training-{index}")
}

/// トレーニングの ID と日付。削除の後も元のトレーニングの日付を保つために使う。
#[derive(Clone)]
struct TrainingLabel {
    id: String,
    achieved_on: JstDate,
}

fn training_labels(count: usize) -> Vec<TrainingLabel> {
    (0..count)
        .map(|index| TrainingLabel {
            id: training_id(index),
            achieved_on: date(index),
        })
        .collect()
}

fn snapshots<'a>(
    trainings: &'a [Vec<SessionSet>],
    labels: &'a [TrainingLabel],
) -> Vec<TrainingSets<'a>> {
    trainings
        .iter()
        .enumerate()
        .filter(|(_, sets)| !sets.is_empty())
        .map(|(index, sets)| TrainingSets {
            training_id: labels[index].id.as_str(),
            achieved_on: labels[index].achieved_on,
            sets,
        })
        .collect()
}

fn sorted(entries: &[PersonalBestEntry]) -> Vec<String> {
    let mut values = entries
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}",
                entry.key.exercise_id,
                entry.key.grip.as_deref().unwrap_or(""),
                entry.key.attachment.as_deref().unwrap_or(""),
                entry.key.kind.as_str(),
                entry.value,
                entry.achieved_on,
                entry.training_id
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn keys(entries: &[PersonalBestEntry]) -> Vec<PersonalBestKey> {
    let mut keys = entries
        .iter()
        .map(|entry| entry.key.clone())
        .collect::<Vec<PersonalBestKey>>();
    keys.sort_by(|left, right| {
        left.exercise_id
            .cmp(&right.exercise_id)
            .then_with(|| left.grip.cmp(&right.grip))
            .then_with(|| left.attachment.cmp(&right.attachment))
            .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
    });
    keys
}

/// 計算し直した組の外の行を残したまま、組の中の行を計算し直した値にする。
fn merged(
    existing: &[PersonalBestEntry],
    recomputed: &[PersonalBestEntry],
    groups: &[BestGroup],
) -> Vec<PersonalBestEntry> {
    existing
        .iter()
        .filter(|entry| !groups.iter().any(|group| group.contains(&entry.key)))
        .cloned()
        .chain(recomputed.iter().cloned())
        .collect()
}

/// 組の一覧から重複を除いて合わせる。
fn union(left: &[BestGroup], right: &[BestGroup]) -> Vec<BestGroup> {
    let mut groups = left.to_vec();
    for group in right {
        if !groups.contains(group) {
            groups.push(group.clone());
        }
    }
    groups
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn a_deleted_training_rebuilds_only_the_groups_it_touched(
        trainings in prop::collection::vec(training(), 1..=4),
        cut in 0usize..4,
    ) {
        let cut = cut % trainings.len();
        let mut remaining = trainings.clone();
        let deleted = remaining.remove(cut);
        let groups = affected_groups(&deleted);
        let labels = training_labels(trainings.len());
        let mut remaining_labels = labels.clone();
        remaining_labels.remove(cut);
        let existing = recompute(&snapshots(&trainings, &labels));
        let expected = recompute(&snapshots(&remaining, &remaining_labels));
        let recomputed = recompute_groups(&snapshots(&remaining, &remaining_labels), &groups);
        let rebuilt = reconcile(&existing, &recomputed, &groups);
        let actual = rebuilt.applied(&existing);

        prop_assert_eq!(sorted(&actual), sorted(&merged(&existing, &recomputed, &groups)));
        prop_assert_eq!(sorted(&actual), sorted(&expected));
        let keys = keys(&actual);
        let mut unique = keys.clone();
        unique.dedup();
        prop_assert_eq!(unique.len(), keys.len());
    }

    #[test]
    fn an_edited_training_rebuilds_the_old_and_the_new_groups(
        trainings in prop::collection::vec(training(), 1..=4),
        edit in 0usize..4,
        replacement in training(),
    ) {
        let edit = edit % trainings.len();
        let before = trainings[edit].clone();
        let mut after = trainings.clone();
        after[edit] = replacement;
        let groups = union(&affected_groups(&before), &affected_groups(&after[edit]));
        let labels = training_labels(trainings.len());
        let existing = recompute(&snapshots(&trainings, &labels));
        let expected = recompute(&snapshots(&after, &labels));
        let recomputed = recompute_groups(&snapshots(&after, &labels), &groups);
        let rebuilt = reconcile(&existing, &recomputed, &groups);
        let actual = rebuilt.applied(&existing);

        prop_assert_eq!(sorted(&actual), sorted(&merged(&existing, &recomputed, &groups)));
        prop_assert_eq!(sorted(&actual), sorted(&expected));
    }

    #[test]
    fn a_rebuild_never_keeps_a_row_without_a_remaining_record(
        trainings in prop::collection::vec(training(), 1..=4),
        cut in 0usize..4,
    ) {
        let cut = cut % trainings.len();
        let mut remaining = trainings.clone();
        remaining.remove(cut);
        let deleted = trainings[cut].clone();
        let groups = affected_groups(&deleted);
        let labels = training_labels(trainings.len());
        let mut remaining_labels = labels.clone();
        remaining_labels.remove(cut);
        let existing = recompute(&snapshots(&trainings, &labels));
        let recomputed = recompute_groups(&snapshots(&remaining, &remaining_labels), &groups);
        let rebuilt = reconcile(&existing, &recomputed, &groups);
        let actual = rebuilt.applied(&existing);

        for group in &groups {
            let remaining_keys = recomputed
                .iter()
                .filter(|entry| group.contains(&entry.key))
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>();
            let actual_keys = actual
                .iter()
                .filter(|entry| group.contains(&entry.key))
                .map(|entry| entry.key.clone())
                .collect::<Vec<_>>();
            prop_assert_eq!(actual_keys, remaining_keys);
        }
    }
}
