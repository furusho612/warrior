//! 自己ベストの候補の作成、保存済みの値との比較、全記録からの計算し直し。
//!
//! 対応する要求: `REQ-SET-009`、`REQ-SET-011`。
//! 候補は `set_calc` の `SetRecord` だけから作り、種目、グリップ、アタッチメント、種類を
//! 比較の単位とする。保存と読み込みは Worker の `personal_best` が担う。
//! 過去の記録の編集と削除の後の計算し直しにも `recompute` を使う。

use serde::{Deserialize, Serialize};

use crate::exercise::Laterality;
use crate::jst::JstDate;
use crate::set_calc::{BestKey, PersonalBest, RecordMode};
use crate::training_session::{BestKind, PersonalBestItem, SessionSet};

/// 自己ベストの比較の単位。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PersonalBestKey {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。選ばない種目は None。
    pub grip: Option<String>,
    /// アタッチメント。選ばない種目は None。
    pub attachment: Option<String>,
    /// 自己ベストの種類。
    pub kind: BestKind,
}

/// 自己ベストの候補。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonalBestCandidate {
    /// 種目 ID。
    pub exercise_id: String,
    /// 記録時点の種目名。
    pub exercise_name: Option<String>,
    /// 記録方式。
    pub record_mode: RecordMode,
    /// 左右別に行うか。
    pub laterality: Laterality,
    /// グリップ。
    pub grip: Option<String>,
    /// アタッチメント。
    pub attachment: Option<String>,
    /// 自己ベストの種類。
    pub kind: BestKind,
    /// 値。時間で記録する種目は秒を表す。
    pub value: f64,
}

impl PersonalBestCandidate {
    /// 比較の単位。
    pub fn key(&self) -> PersonalBestKey {
        PersonalBestKey {
            exercise_id: self.exercise_id.clone(),
            grip: self.grip.clone(),
            attachment: self.attachment.clone(),
            kind: self.kind,
        }
    }

    /// 画面とセッションの保存に載せる項目。
    pub fn item(&self) -> PersonalBestItem {
        PersonalBestItem {
            exercise_id: self.exercise_id.clone(),
            exercise_name: self.exercise_name.clone(),
            record_mode: self.record_mode,
            laterality: self.laterality,
            grip: self.grip.clone(),
            attachment: self.attachment.clone(),
            kind: self.kind,
            value: self.value,
        }
    }
}

/// 保存済みの自己ベスト。
#[derive(Debug, Clone, PartialEq)]
pub struct PersonalBestEntry {
    /// 比較の単位。
    pub key: PersonalBestKey,
    /// 値。
    pub value: f64,
    /// 達成日 (日本時間)。
    pub achieved_on: JstDate,
    /// 達成したトレーニングの ID。
    pub training_id: String,
}

/// 計算し直しの対象にする、1 つのトレーニングの全セット。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainingSets<'a> {
    /// トレーニングの ID。
    pub training_id: &'a str,
    /// トレーニングの日付 (日本時間)。
    pub achieved_on: JstDate,
    /// 完了したセットの記録。
    pub sets: &'a [SessionSet],
}

/// 1 つのトレーニングのセットから、自己ベストの候補を種類ごとに作る。
///
/// 最大重量と推定 1RM はセットの最初の段、種目別の総重量は種目の全セットを使う。
/// ウォームアップと時間で記録する種目の重量の候補は作らない。
pub fn candidates(achieved_on: JstDate, sets: &[SessionSet]) -> Vec<PersonalBestCandidate> {
    let mut groups = Vec::<CandidateGroup>::new();
    for set in sets {
        let index = match groups.iter().position(|group| group.matches(set)) {
            Some(index) => index,
            None => {
                groups.push(CandidateGroup::new(set));
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        group.best.update(&set.record, achieved_on);
        group.total_weight += set.record.strict_total_weight_kg();
    }

    let mut candidates = Vec::new();
    for group in &groups {
        if let Some(value) = group.best.max_weight() {
            candidates.push(group.candidate(BestKind::MaxWeight, value.value));
        }
        if let Some(value) = group.best.estimated_one_rep_max() {
            candidates.push(group.candidate(BestKind::EstimatedOneRepMax, value.value));
        }
        if group.total_weight > 0.0 {
            candidates.push(group.candidate(BestKind::TotalWeight, group.total_weight));
        }
        if let Some(value) = group.best.longest_duration() {
            candidates.push(group.candidate(BestKind::LongestDuration, f64::from(value.value)));
        }
    }
    candidates
}

/// 保存済みの値より大きい候補だけを返す。
///
/// 値が等しい候補は更新としない。これにより、同じトレーニングで同じ種類を複数回
/// 更新した場合も、最初に達成した日付とトレーニングを残す。
pub fn updated(
    existing: &[PersonalBestEntry],
    candidates: &[PersonalBestCandidate],
) -> Vec<PersonalBestCandidate> {
    let mut updates = Vec::<PersonalBestCandidate>::new();
    for candidate in candidates {
        let key = candidate.key();
        let saved = existing
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value);
        let pending = updates
            .iter()
            .find(|update| update.key() == key)
            .map(|update| update.value);
        let previous = match (saved, pending) {
            (Some(saved), Some(pending)) => Some(saved.max(pending)),
            (Some(saved), None) => Some(saved),
            (None, Some(pending)) => Some(pending),
            (None, None) => None,
        };
        if previous.is_some_and(|previous| candidate.value <= previous) {
            continue;
        }
        match updates.iter_mut().find(|update| update.key() == key) {
            Some(update) => *update = candidate.clone(),
            None => updates.push(candidate.clone()),
        }
    }
    updates
}

/// 候補のうち、保存済みの値より大きいものを保存済みの一覧へ反映する。
///
/// 同じトレーニングで同じ種類を複数回更新した場合も、値は増える方向にだけ進むため、
/// 最後の候補が残る。
pub fn apply(
    existing: &mut Vec<PersonalBestEntry>,
    achieved_on: JstDate,
    training_id: &str,
    candidates: &[PersonalBestCandidate],
) {
    for candidate in updated(existing, candidates) {
        let entry = PersonalBestEntry {
            key: candidate.key(),
            value: candidate.value,
            achieved_on,
            training_id: training_id.to_owned(),
        };
        match existing.iter().position(|saved| saved.key == entry.key) {
            Some(index) => existing[index] = entry,
            None => existing.push(entry),
        }
    }
}

/// 全ての記録から、比較の単位ごとの自己ベストを計算し直す。
///
/// トレーニングは日付の昇順で渡す。同じ値のトレーニングが複数ある場合は、
/// 先に達成したトレーニングの日付と ID を残す。
pub fn recompute(trainings: &[TrainingSets<'_>]) -> Vec<PersonalBestEntry> {
    let mut entries = Vec::<PersonalBestEntry>::new();
    for training in trainings {
        let candidates = candidates(training.achieved_on, training.sets);
        apply(
            &mut entries,
            training.achieved_on,
            training.training_id,
            &candidates,
        );
    }
    entries
}

/// 自己ベストの計算し直しの対象にする、種目、グリップ、アタッチメントの組。
///
/// 自己ベストの種類は組に含めない。組を対象にすると、その組が持ちうる全ての種類を
/// 残っている記録から作り直す。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BestGroup {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。選ばない種目は None。
    pub grip: Option<String>,
    /// アタッチメント。選ばない種目は None。
    pub attachment: Option<String>,
}

impl BestGroup {
    /// セットの記録が比較に使う組を返す。
    pub fn of(key: &BestKey) -> Self {
        Self {
            exercise_id: key.exercise_id.clone(),
            grip: key.grip.clone(),
            attachment: key.attachment.clone(),
        }
    }

    /// 比較の単位がこの組に属するか返す。
    pub fn contains(&self, key: &PersonalBestKey) -> bool {
        self.exercise_id == key.exercise_id
            && self.grip == key.grip
            && self.attachment == key.attachment
    }
}

/// セットの記録から、影響する組を重複なく返す。
///
/// 編集または削除したトレーニングのセットを渡し、計算し直す範囲を決める。
pub fn affected_groups(sets: &[SessionSet]) -> Vec<BestGroup> {
    let mut groups = Vec::<BestGroup>::new();
    for set in sets {
        let group = BestGroup::of(set.record.key());
        if !groups.contains(&group) {
            groups.push(group);
        }
    }
    groups
}

/// 全ての記録から、指定した組だけの自己ベストを計算し直す。
///
/// トレーニングは日付の昇順で渡す。組の外の候補は捨てる。
pub fn recompute_groups(
    trainings: &[TrainingSets<'_>],
    groups: &[BestGroup],
) -> Vec<PersonalBestEntry> {
    recompute(trainings)
        .into_iter()
        .filter(|entry| groups.iter().any(|group| group.contains(&entry.key)))
        .collect()
}

/// 保存済みの行のうち、指定した組の行を計算し直した値で置き換える。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RebuiltBests {
    /// 書き込む、または値を更新する行。
    pub upserts: Vec<PersonalBestEntry>,
    /// 値がなくなった行のキー。
    pub deletes: Vec<PersonalBestKey>,
}

impl RebuiltBests {
    /// 適用の後の自己ベストの一覧を返す。
    ///
    /// 組の外の行は変えず、組の中の行だけを計算し直した値にする。
    pub fn applied(&self, existing: &[PersonalBestEntry]) -> Vec<PersonalBestEntry> {
        let mut entries = existing
            .iter()
            .filter(|entry| !self.deletes.contains(&entry.key))
            .filter(|entry| !self.upserts.iter().any(|upsert| upsert.key == entry.key))
            .cloned()
            .collect::<Vec<_>>();
        entries.extend(self.upserts.iter().cloned());
        entries
    }
}

/// 計算し直した値と保存済みの行の差を返す。
///
/// 対象の組の行は計算し直した値で置き換え、計算し直した値が無い行は削除する。
/// 同じ値の行も書き直す。書き込みを冪等にし、再送で状態を変えないためである。
pub fn reconcile(
    existing: &[PersonalBestEntry],
    recomputed: &[PersonalBestEntry],
    groups: &[BestGroup],
) -> RebuiltBests {
    let upserts = recomputed
        .iter()
        .filter(|entry| groups.iter().any(|group| group.contains(&entry.key)))
        .cloned()
        .collect::<Vec<_>>();
    let deletes = existing
        .iter()
        .map(|entry| entry.key.clone())
        .filter(|key| groups.iter().any(|group| group.contains(key)))
        .filter(|key| !upserts.iter().any(|upsert| upsert.key == *key))
        .collect::<Vec<_>>();
    RebuiltBests { upserts, deletes }
}

struct CandidateGroup {
    key: BestKey,
    mode: RecordMode,
    laterality: Laterality,
    exercise_name: Option<String>,
    best: PersonalBest,
    total_weight: f64,
}

impl CandidateGroup {
    fn new(set: &SessionSet) -> Self {
        let key = set.record.key().clone();
        Self {
            best: PersonalBest::new(key.clone()),
            key,
            mode: set.record.mode(),
            laterality: set.laterality,
            exercise_name: Some(set.exercise_name.clone()),
            total_weight: 0.0,
        }
    }

    fn matches(&self, set: &SessionSet) -> bool {
        self.key == *set.record.key()
            && self.mode == set.record.mode()
            && self.laterality == set.laterality
    }

    fn candidate(&self, kind: BestKind, value: f64) -> PersonalBestCandidate {
        PersonalBestCandidate {
            exercise_id: self.key.exercise_id.clone(),
            exercise_name: self.exercise_name.clone(),
            record_mode: self.mode,
            laterality: self.laterality,
            grip: self.key.grip.clone(),
            attachment: self.key.attachment.clone(),
            kind,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    //! 自己ベストの候補と比較の例示テスト。
    //!
    //! 対応する要求: `REQ-SET-009`、`REQ-SET-011`。
    //! 4 種類、ドロップセット、ウォームアップ、グリップの分離、左右別の時間を固定する。

    use super::{
        affected_groups, candidates, recompute, recompute_groups, reconcile, updated, BestGroup,
        PersonalBestEntry, PersonalBestKey, TrainingSets,
    };
    use crate::exercise::Laterality;
    use crate::jst::JstDate;
    use crate::set_calc::{
        estimated_one_rep_max, BestKey, RecordMode, SetKind, SetRecord, SetSegment, SideValue,
    };
    use crate::training_session::{BestKind, SessionSet};

    #[test]
    fn candidates_cover_the_four_kinds() {
        let sets = vec![
            session_set(
                "bench",
                weight_record(SetKind::Normal, vec![segment(100.0, 5)]),
            ),
            session_set("plank", time_record(90)),
        ];
        let candidates = candidates(date(12), &sets);

        assert_eq!(candidates.len(), 4);
        assert_candidate(&candidates[0], "bench", BestKind::MaxWeight, 100.0);
        assert_candidate(
            &candidates[1],
            "bench",
            BestKind::EstimatedOneRepMax,
            estimated_one_rep_max(100.0, 5).unwrap(),
        );
        assert_candidate(&candidates[2], "bench", BestKind::TotalWeight, 500.0);
        assert_candidate(&candidates[3], "plank", BestKind::LongestDuration, 90.0);
    }

    #[test]
    fn drop_set_uses_the_first_segment_for_max_and_one_rep_max_but_all_segments_for_total() {
        let sets = vec![session_set(
            "bench",
            weight_record(SetKind::Drop, vec![segment(100.0, 8), segment(80.0, 6)]),
        )];
        let candidates = candidates(date(12), &sets);

        assert_candidate(&candidates[0], "bench", BestKind::MaxWeight, 100.0);
        assert_candidate(
            &candidates[1],
            "bench",
            BestKind::EstimatedOneRepMax,
            estimated_one_rep_max(100.0, 8).unwrap(),
        );
        assert_candidate(
            &candidates[2],
            "bench",
            BestKind::TotalWeight,
            800.0 + 480.0,
        );
    }

    #[test]
    fn a_heavier_second_drop_segment_does_not_replace_the_first_segment() {
        let sets = vec![session_set(
            "bench",
            weight_record(SetKind::Drop, vec![segment(80.0, 6), segment(100.0, 8)]),
        )];
        let candidates = candidates(date(12), &sets);

        assert_candidate(&candidates[0], "bench", BestKind::MaxWeight, 80.0);
        assert_candidate(
            &candidates[1],
            "bench",
            BestKind::EstimatedOneRepMax,
            estimated_one_rep_max(80.0, 6).unwrap(),
        );
        assert_candidate(
            &candidates[2],
            "bench",
            BestKind::TotalWeight,
            480.0 + 800.0,
        );
    }

    #[test]
    fn warmup_is_excluded_from_the_candidates() {
        let sets = vec![session_set(
            "bench",
            weight_record(SetKind::Warmup, vec![segment(100.0, 10)]),
        )];
        assert!(candidates(date(12), &sets).is_empty());
    }

    #[test]
    fn grip_and_attachment_split_the_comparison_key() {
        let grip = BestKey {
            exercise_id: "bench".to_owned(),
            grip: Some("pronated_medium".to_owned()),
            attachment: None,
        };
        let attachment = BestKey {
            exercise_id: "bench".to_owned(),
            grip: Some("parallel".to_owned()),
            attachment: Some("wide_bar".to_owned()),
        };
        let sets = vec![
            session_set_with_key(
                grip,
                weight_record(SetKind::Normal, vec![segment(100.0, 5)]),
            ),
            session_set_with_key(
                attachment,
                weight_record(SetKind::Normal, vec![segment(80.0, 5)]),
            ),
        ];
        let max_weights = candidates(date(12), &sets)
            .into_iter()
            .filter(|candidate| candidate.kind == BestKind::MaxWeight)
            .collect::<Vec<_>>();

        assert_eq!(max_weights.len(), 2);
        assert_eq!(max_weights[0].grip.as_deref(), Some("pronated_medium"));
        assert_eq!(max_weights[0].attachment, None);
        assert_eq!(max_weights[1].grip.as_deref(), Some("parallel"));
        assert_eq!(max_weights[1].attachment.as_deref(), Some("wide_bar"));
        assert_ne!(max_weights[0].key(), max_weights[1].key());
    }

    #[test]
    fn a_unilateral_time_set_uses_the_longest_side_and_ignores_the_total() {
        let record = SetRecord::new(
            key("side-plank"),
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
        .expect("a valid time set");
        assert_eq!(record.total_duration_seconds(), 70);
        assert_eq!(record.longest_duration_seconds(), Some(40));
        let candidates = candidates(date(12), &[session_set("side-plank", record)]);

        assert_eq!(candidates.len(), 1);
        assert_candidate(
            &candidates[0],
            "side-plank",
            BestKind::LongestDuration,
            40.0,
        );
    }

    #[test]
    fn candidates_with_the_same_comparison_key_keep_the_largest_value() {
        let heavier = session_set(
            "bench",
            SetRecord::new(
                key("bench"),
                SetKind::Normal,
                vec![segment(100.0, 1)],
                None,
                None,
                RecordMode::BodyweightAndReps,
            )
            .expect("a valid set"),
        );
        let lighter = session_set(
            "bench",
            weight_record(SetKind::Normal, vec![segment(50.0, 1)]),
        );
        let candidates = candidates(date(12), &[heavier, lighter]);
        let updates = updated(&[], &candidates);
        let max_weight = updates
            .iter()
            .find(|candidate| candidate.kind == BestKind::MaxWeight)
            .expect("the max weight candidate");

        assert_eq!(updates.len(), 3);
        assert_eq!(max_weight.value, 100.0);
    }

    #[test]
    fn an_update_must_exceed_the_saved_value() {
        let candidate = candidates(
            date(12),
            &[session_set(
                "bench",
                weight_record(SetKind::Normal, vec![segment(100.0, 5)]),
            )],
        )
        .into_iter()
        .find(|candidate| candidate.kind == BestKind::MaxWeight)
        .expect("the max weight candidate");
        let existing = vec![PersonalBestEntry {
            key: candidate.key(),
            value: 100.0,
            achieved_on: date(11),
            training_id: "past".to_owned(),
        }];

        assert!(updated(&existing, std::slice::from_ref(&candidate)).is_empty());
        assert_eq!(updated(&[], std::slice::from_ref(&candidate)).len(), 1);
    }

    #[test]
    fn recompute_keeps_the_first_training_of_equal_values() {
        let first = session_set(
            "bench",
            weight_record(SetKind::Normal, vec![segment(100.0, 5)]),
        );
        let second = session_set(
            "bench",
            weight_record(SetKind::Normal, vec![segment(100.0, 5)]),
        );
        let entries = recompute(&[
            TrainingSets {
                training_id: "first",
                achieved_on: date(11),
                sets: std::slice::from_ref(&first),
            },
            TrainingSets {
                training_id: "second",
                achieved_on: date(12),
                sets: std::slice::from_ref(&second),
            },
        ]);
        let max_weight = entries
            .iter()
            .find(|entry| entry.key.kind == BestKind::MaxWeight)
            .expect("the max weight best");

        assert_eq!(max_weight.training_id, "first");
        assert_eq!(max_weight.achieved_on, date(11));
        assert_eq!(max_weight.value, 100.0);
    }

    #[test]
    fn the_key_includes_the_kind() {
        let first = PersonalBestKey {
            exercise_id: "bench".to_owned(),
            grip: None,
            attachment: None,
            kind: BestKind::MaxWeight,
        };
        let second = PersonalBestKey {
            kind: BestKind::EstimatedOneRepMax,
            ..first.clone()
        };
        assert_ne!(first, second);
    }

    fn date(day: u32) -> JstDate {
        JstDate::new(2026, 11, day).unwrap()
    }

    fn key(exercise_id: &str) -> BestKey {
        BestKey {
            exercise_id: exercise_id.to_owned(),
            grip: None,
            attachment: None,
        }
    }

    fn segment(weight_kg: f64, reps: u32) -> SetSegment {
        SetSegment {
            weight_kg,
            reps,
            momentum_reps: 0,
            assisted_reps: 0,
            left: None,
            right: None,
            duration_seconds: None,
        }
    }

    fn weight_record(kind: SetKind, segments: Vec<SetSegment>) -> SetRecord {
        SetRecord::new(
            key("bench"),
            kind,
            segments,
            None,
            None,
            RecordMode::WeightAndReps,
        )
        .expect("a valid weight set")
    }

    fn time_record(duration: u32) -> SetRecord {
        SetRecord::new(
            key("plank"),
            SetKind::Normal,
            Vec::new(),
            None,
            Some(duration),
            RecordMode::Time,
        )
        .expect("a valid time set")
    }

    fn session_set(exercise_name: &str, record: SetRecord) -> SessionSet {
        SessionSet {
            record,
            exercise_name: exercise_name.to_owned(),
            laterality: Laterality::Bilateral,
        }
    }

    fn session_set_with_key(key: BestKey, record: SetRecord) -> SessionSet {
        let record = SetRecord::new(
            key,
            record.kind(),
            record.segments().to_vec(),
            record.rpe(),
            record.duration_seconds(),
            record.mode(),
        )
        .expect("a valid set with the new key");
        session_set("bench", record)
    }

    fn assert_candidate(
        candidate: &super::PersonalBestCandidate,
        exercise_id: &str,
        kind: BestKind,
        value: f64,
    ) {
        assert_eq!(candidate.exercise_id, exercise_id);
        assert_eq!(candidate.kind, kind);
        assert!(
            (candidate.value - value).abs() < 1e-9,
            "{kind:?} must be {value} but was {}",
            candidate.value
        );
    }

    #[test]
    fn a_rebuild_replaces_the_affected_rows_and_keeps_the_others() {
        let affected = group("bench");
        let untouched = group("squat");
        let existing = vec![
            entry(&affected, BestKind::MaxWeight, 100.0, 11, "first"),
            entry(&untouched, BestKind::MaxWeight, 150.0, 10, "squat"),
        ];
        let recomputed = vec![entry(&affected, BestKind::MaxWeight, 90.0, 12, "second")];

        let rebuilt = reconcile(&existing, &recomputed, std::slice::from_ref(&affected));

        assert_eq!(rebuilt.upserts, recomputed);
        assert!(rebuilt.deletes.is_empty());
        assert_eq!(
            rebuilt.applied(&existing),
            vec![
                entry(&untouched, BestKind::MaxWeight, 150.0, 10, "squat"),
                entry(&affected, BestKind::MaxWeight, 90.0, 12, "second"),
            ]
        );
    }

    #[test]
    fn a_rebuild_deletes_the_rows_whose_value_is_gone() {
        let affected = group("bench");
        let untouched = group("squat");
        let existing = vec![
            entry(&affected, BestKind::MaxWeight, 100.0, 11, "first"),
            entry(&affected, BestKind::EstimatedOneRepMax, 120.0, 11, "first"),
            entry(&untouched, BestKind::MaxWeight, 150.0, 10, "squat"),
        ];

        let rebuilt = reconcile(&existing, &[], std::slice::from_ref(&affected));

        assert!(rebuilt.upserts.is_empty());
        assert_eq!(
            rebuilt.deletes,
            vec![
                PersonalBestKey {
                    kind: BestKind::MaxWeight,
                    ..key_of(&affected)
                },
                PersonalBestKey {
                    kind: BestKind::EstimatedOneRepMax,
                    ..key_of(&affected)
                },
            ]
        );
        assert_eq!(
            rebuilt.applied(&existing),
            vec![entry(&untouched, BestKind::MaxWeight, 150.0, 10, "squat")]
        );
    }

    #[test]
    fn recompute_groups_only_returns_the_requested_groups() {
        let bench = session_set(
            "bench",
            weight_record(SetKind::Normal, vec![segment(100.0, 5)]),
        );
        let squat_key = BestKey {
            exercise_id: "squat".to_owned(),
            grip: None,
            attachment: None,
        };
        let squat = session_set(
            "squat",
            SetRecord::new(
                squat_key,
                SetKind::Normal,
                vec![segment(150.0, 5)],
                None,
                None,
                RecordMode::WeightAndReps,
            )
            .expect("a valid set"),
        );
        let entries = recompute_groups(
            &[TrainingSets {
                training_id: "training",
                achieved_on: date(12),
                sets: &[bench, squat],
            }],
            &[group("bench")],
        );

        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| entry.key.exercise_id == "bench"));
        assert_eq!(
            affected_groups(&[session_set(
                "bench",
                weight_record(SetKind::Normal, vec![segment(100.0, 5)])
            )]),
            vec![group("bench")]
        );
    }

    fn group(exercise_id: &str) -> BestGroup {
        BestGroup {
            exercise_id: exercise_id.to_owned(),
            grip: None,
            attachment: None,
        }
    }

    fn key_of(group: &BestGroup) -> PersonalBestKey {
        PersonalBestKey {
            exercise_id: group.exercise_id.clone(),
            grip: group.grip.clone(),
            attachment: group.attachment.clone(),
            kind: BestKind::MaxWeight,
        }
    }

    fn entry(
        group: &BestGroup,
        kind: BestKind,
        value: f64,
        day: u32,
        training_id: &str,
    ) -> PersonalBestEntry {
        PersonalBestEntry {
            key: PersonalBestKey {
                kind,
                ..key_of(group)
            },
            value,
            achieved_on: date(day),
            training_id: training_id.to_owned(),
        }
    }
}
