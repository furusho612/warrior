//! 種目ライブラリの絞り込み、並び、ページングの性質テスト。
//!
//! 対応する要求: `REQ-LIB-006`、`REQ-LIB-007`。
//! 絞り込みの結果が全ての条件を満たし条件を満たす種目を落とさないこと、並びが部位区分、
//! 使用器具、読みの順序の比較と一致すること、ページを順に取ると全件を重複なく
//! 元の順で取れることを確かめる。

mod support;

use std::num::NonZeroUsize;

use proptest::prelude::*;
use warrior_core::cursor::decode_cursor;
use warrior_core::exercise::{BodyPart, Equipment, Exercise, GripMode, Laterality, RecordMode};
use warrior_core::library::{
    filter_exercises, kana_sort_key, paginate, sort_exercises, ExerciseFilter, ExerciseSource,
    LibraryExercise,
};

fn body_part() -> impl Strategy<Value = BodyPart> {
    prop::sample::select(BodyPart::ALL.to_vec())
}

fn equipment() -> impl Strategy<Value = Equipment> {
    prop::sample::select(Equipment::ALL.to_vec())
}

fn kana() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::char::range('ぁ', 'ん'), 0..6)
        .prop_map(|characters| characters.into_iter().collect())
}

fn library_exercise() -> impl Strategy<Value = LibraryExercise> {
    (
        support::uuid_v4_text(),
        kana(),
        kana(),
        body_part(),
        equipment(),
        any::<bool>(),
    )
        .prop_map(
            |(id, name, name_kana, body_part, equipment, favorite)| LibraryExercise {
                exercise: Exercise {
                    id,
                    name,
                    name_kana,
                    body_part,
                    equipment,
                    record_mode: RecordMode::WeightAndReps,
                    laterality: Laterality::Bilateral,
                    grip_mode: GripMode::None,
                    other_scope: None,
                    instructions: String::new(),
                    cautions: String::new(),
                    tips: String::new(),
                    sort_order: 0,
                    muscles: Vec::new(),
                    grip_options: Vec::new(),
                },
                favorite,
                source: ExerciseSource::Master,
                deleted: false,
            },
        )
}

fn filter() -> impl Strategy<Value = ExerciseFilter> {
    (
        prop_oneof![Just(Option::<BodyPart>::None), body_part().prop_map(Some)],
        prop_oneof![Just(Option::<Equipment>::None), equipment().prop_map(Some)],
        any::<bool>(),
        prop_oneof![
            Just(String::new()),
            kana(),
            kana().prop_map(|text| format!(" {text} "))
        ],
    )
        .prop_map(
            |(body_part, equipment, favorite_only, query)| ExerciseFilter {
                body_part,
                equipment,
                favorite_only,
                query,
            },
        )
}

fn satisfies(item: &LibraryExercise, filter: &ExerciseFilter) -> bool {
    if filter
        .body_part
        .is_some_and(|part| item.exercise.body_part != part)
    {
        return false;
    }
    if filter
        .equipment
        .is_some_and(|equipment| item.exercise.equipment != equipment)
    {
        return false;
    }
    if filter.favorite_only && !item.favorite {
        return false;
    }
    let query = filter.query.trim();
    query.is_empty()
        || item.exercise.name.contains(query)
        || item.exercise.name_kana.contains(query)
}

fn order_key(item: &LibraryExercise) -> (u8, u8, String, String) {
    (
        item.exercise.body_part.order(),
        item.exercise.equipment.order(),
        kana_sort_key(&item.exercise.name_kana),
        item.exercise.id.clone(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn filtering_keeps_exactly_the_exercises_that_satisfy_every_condition(
        items in proptest::collection::vec(library_exercise(), 0..30),
        filter in filter(),
    ) {
        let result = filter_exercises(items.clone(), &filter);
        for item in &result {
            prop_assert!(satisfies(item, &filter));
        }
        for item in &items {
            if satisfies(item, &filter) {
                prop_assert!(result.contains(item));
            } else {
                prop_assert!(!result.contains(item));
            }
        }
        prop_assert_eq!(
            result.len(),
            items.iter().filter(|item| satisfies(item, &filter)).count()
        );
    }

    #[test]
    fn sorting_follows_the_body_part_equipment_and_kana_order(
        items in proptest::collection::vec(library_exercise(), 0..30),
    ) {
        let mut sorted = items.clone();
        sort_exercises(&mut sorted);
        prop_assert_eq!(sorted.len(), items.len());
        for pair in sorted.windows(2) {
            prop_assert!(order_key(&pair[0]) <= order_key(&pair[1]));
        }
        let mut expected = items;
        expected.sort_by_key(order_key);
        prop_assert_eq!(sorted, expected);
    }

    #[test]
    fn paging_returns_every_exercise_once_in_order(
        items in proptest::collection::vec(library_exercise(), 0..30),
        page_size in 1usize..=8,
    ) {
        let mut sorted = items;
        sort_exercises(&mut sorted);
        let page_size = NonZeroUsize::new(page_size).expect("a positive page size");
        let mut cursor = None;
        let mut collected = Vec::new();
        let mut steps = 0;
        loop {
            let page = paginate(&sorted, cursor.as_ref(), page_size);
            if page.exercises.is_empty() {
                prop_assert!(page.next_cursor.is_none());
                break;
            }
            collected.extend(page.exercises);
            steps += 1;
            prop_assert!(steps <= sorted.len());
            let Some(next) = page.next_cursor else {
                break;
            };
            cursor = Some(decode_cursor(&next).expect("a decodable cursor"));
        }
        prop_assert_eq!(collected, sorted);
    }
}
