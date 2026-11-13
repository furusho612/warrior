//! 種目マスターの初期データの結合テスト。
//!
//! 対応する要求: `REQ-LIB-001`。
//! ローカルの D1 に migrations を適用し、投入した 106 種目と 24 個の筋肉の単位を読む。
//! データが固定であるため PBT は使わない。

mod support;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use support::DevServer;
use warrior_core::exercise::{
    BodyPart, Equipment, Exercise, GripMode, GripSelection, Laterality, MuscleAssignment,
    MuscleRole, MuscleUnit, OtherScope,
};
use warrior_core::ids::is_valid_uuid_v4;
use warrior_core::set_calc::RecordMode;

const PORT: u16 = 8810;

/// 15 本のプログラムの主な種目と、その種目を表す部位区分と記録方式。
///
/// 種目名がマスターに無い場合は、同じ部位区分かつ同じ記録方式の種目を代替にする。
/// 器具の網羅のため追加した 3 種は、プログラムの主な種目に含めない。
const MAIN_EXERCISES: [(&str, &str, &str); 37] = [
    ("スクワット", "legs", "weight_and_reps"),
    ("ベンチプレス", "chest", "weight_and_reps"),
    ("ラットプルダウン", "back", "weight_and_reps"),
    ("ショルダープレス", "shoulders", "weight_and_reps"),
    ("レッグカール", "legs", "weight_and_reps"),
    ("プランク", "abs", "time"),
    ("ベントオーバーロウ", "back", "weight_and_reps"),
    ("ルーマニアンデッドリフト", "legs", "weight_and_reps"),
    ("レッグプレス", "legs", "weight_and_reps"),
    ("インクラインベンチプレス", "chest", "weight_and_reps"),
    ("ダンベルフライ", "chest", "weight_and_reps"),
    ("チンニング", "back", "bodyweight_and_reps"),
    ("シーテッドロウ", "back", "weight_and_reps"),
    ("プッシュアップ", "chest", "bodyweight_and_reps"),
    ("トレッドミル", "other", "time"),
    ("エアロバイク", "other", "time"),
    ("サイドレイズ", "shoulders", "weight_and_reps"),
    ("ランジ", "legs", "weight_and_reps"),
    ("ケーブルクロスオーバー", "chest", "weight_and_reps"),
    ("ブルガリアンスクワット", "legs", "weight_and_reps"),
    ("ローイングマシン", "other", "time"),
    ("クランチ", "abs", "bodyweight_and_reps"),
    ("ヒップスラスト", "legs", "weight_and_reps"),
    ("レッグエクステンション", "legs", "weight_and_reps"),
    ("ケーブルキックバック", "legs", "weight_and_reps"),
    ("デッドリフト", "back", "weight_and_reps"),
    ("オーバーヘッドプレス", "shoulders", "weight_and_reps"),
    ("フロントスクワット", "legs", "weight_and_reps"),
    ("ナローベンチプレス", "triceps", "weight_and_reps"),
    ("縄跳び", "other", "time"),
    ("フェイスプル", "shoulders", "weight_and_reps"),
    ("バーピー", "other", "bodyweight_and_reps"),
    ("スミスマシンスクワット", "legs", "weight_and_reps"),
    ("インクラインダンベルプレス", "chest", "weight_and_reps"),
    ("マシンロウ", "back", "weight_and_reps"),
    ("ハックスクワット", "legs", "weight_and_reps"),
    ("ダンベルプレス", "chest", "weight_and_reps"),
];

#[derive(Debug, Clone)]
struct SeedExercise {
    id: String,
    name: String,
    name_kana: String,
    body_part: String,
    equipment: String,
    record_mode: String,
    laterality: String,
    grip_mode: String,
    other_scope: Option<String>,
    instructions: String,
    cautions: String,
    tips: String,
    sort_order: i64,
    grip_options: String,
}

impl SeedExercise {
    fn from_row(row: &Value) -> Self {
        Self {
            id: row_text(row, "id"),
            name: row_text(row, "name"),
            name_kana: row_text(row, "name_kana"),
            body_part: row_text(row, "body_part"),
            equipment: row_text(row, "equipment"),
            record_mode: row_text(row, "record_mode"),
            laterality: row_text(row, "laterality"),
            grip_mode: row_text(row, "grip_mode"),
            other_scope: row_optional_text(row, "other_scope"),
            instructions: row_text(row, "instructions"),
            cautions: row_text(row, "cautions"),
            tips: row_text(row, "tips"),
            sort_order: row_integer(row, "sort_order"),
            grip_options: row_text(row, "grip_options"),
        }
    }
}

#[derive(Debug, Clone)]
struct SeedMuscle {
    exercise_id: String,
    muscle_id: String,
    role: String,
}

impl SeedMuscle {
    fn from_row(row: &Value) -> Self {
        Self {
            exercise_id: row_text(row, "exercise_id"),
            muscle_id: row_text(row, "muscle_id"),
            role: row_text(row, "role"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeedMuscleUnit {
    id: String,
    body_part: String,
    name_ja: String,
    display_order: i64,
}

impl SeedMuscleUnit {
    fn from_row(row: &Value) -> Self {
        Self {
            id: row_text(row, "id"),
            body_part: row_text(row, "body_part"),
            name_ja: row_text(row, "name_ja"),
            display_order: row_integer(row, "display_order"),
        }
    }
}

#[test]
fn req_lib_001_the_seed_covers_the_library_and_program_exercises() {
    let server = DevServer::start(PORT);
    server
        .apply_migrations()
        .expect("the local D1 migrations must apply");

    let exercises = seed_exercises(&server);
    let muscles = seed_muscles(&server);
    let units = seed_muscle_units(&server);

    assert_seed_counts(&exercises, &muscles);
    assert_muscle_units(&units);
    assert_equipment_coverage(&exercises);
    assert_program_exercises(&exercises);
    assert_muscle_integrity(&exercises, &muscles);
    assert_kana_order(&exercises);
    assert_grip_options(&exercises);
    assert_exercise_type_validation(&exercises, &muscles);
    assert_the_seed_is_idempotent(&server, &exercises, &muscles);
}

fn assert_seed_counts(exercises: &[SeedExercise], muscles: &[SeedMuscle]) {
    assert!(
        (100..=110).contains(&exercises.len()),
        "the seed must contain 100 to 110 exercises but had {}",
        exercises.len()
    );
    assert_eq!(
        exercises.len(),
        106,
        "the appendix has 103 exercises and 3 additions"
    );
    assert_eq!(
        muscles.len(),
        281,
        "the appendix has 122 primary and 159 secondary assignments"
    );
    assert_eq!(
        muscles
            .iter()
            .filter(|muscle| muscle.role == "primary")
            .count(),
        122
    );
    assert_eq!(
        muscles
            .iter()
            .filter(|muscle| muscle.role == "secondary")
            .count(),
        159
    );
    assert_eq!(
        exercises
            .iter()
            .filter(|exercise| exercise.body_part == "other")
            .count(),
        8
    );
}

fn assert_muscle_units(units: &[SeedMuscleUnit]) {
    let expected = MuscleUnit::ALL
        .iter()
        .enumerate()
        .map(|(index, unit)| SeedMuscleUnit {
            id: unit.id().to_owned(),
            body_part: unit.body_part().as_str().to_owned(),
            name_ja: unit.name_ja().to_owned(),
            display_order: (index + 1) as i64,
        })
        .collect::<Vec<_>>();
    assert_eq!(units, expected.as_slice());
}

fn assert_equipment_coverage(exercises: &[SeedExercise]) {
    let expected = BTreeMap::from([
        (
            "chest",
            BTreeSet::from(["barbell", "dumbbell", "machine", "cable", "bodyweight"]),
        ),
        (
            "back",
            BTreeSet::from(["barbell", "dumbbell", "machine", "cable", "bodyweight"]),
        ),
        (
            "shoulders",
            BTreeSet::from(["barbell", "dumbbell", "machine", "cable", "bodyweight"]),
        ),
        (
            "biceps_forearm",
            BTreeSet::from(["barbell", "dumbbell", "machine", "cable", "bodyweight"]),
        ),
        (
            "triceps",
            BTreeSet::from(["barbell", "dumbbell", "machine", "cable", "bodyweight"]),
        ),
        (
            "legs",
            BTreeSet::from(["barbell", "dumbbell", "machine", "cable", "bodyweight"]),
        ),
        (
            "abs",
            BTreeSet::from([
                "barbell",
                "dumbbell",
                "machine",
                "cable",
                "bodyweight",
                "other",
            ]),
        ),
        ("other", BTreeSet::from(["machine", "bodyweight", "other"])),
    ]);
    let actual = exercises.iter().fold(
        BTreeMap::<&str, BTreeSet<&str>>::new(),
        |mut map, exercise| {
            map.entry(exercise.body_part.as_str())
                .or_default()
                .insert(exercise.equipment.as_str());
            map
        },
    );
    assert_eq!(actual, expected);
}

fn assert_program_exercises(exercises: &[SeedExercise]) {
    let by_name = exercises
        .iter()
        .map(|exercise| (exercise.name.as_str(), exercise))
        .collect::<BTreeMap<_, _>>();
    let by_mode = exercises
        .iter()
        .map(|exercise| (exercise.body_part.as_str(), exercise.record_mode.as_str()))
        .collect::<BTreeSet<_>>();
    for (name, body_part, record_mode) in MAIN_EXERCISES {
        if let Some(exercise) = by_name.get(name) {
            assert_eq!(
                (exercise.body_part.as_str(), exercise.record_mode.as_str()),
                (body_part, record_mode),
                "{name} must keep the planned body part and record mode"
            );
        } else {
            assert!(
                by_mode.contains(&(body_part, record_mode)),
                "{name} needs an exercise with body part {body_part} and record mode {record_mode}"
            );
        }
    }
}

fn assert_muscle_integrity(exercises: &[SeedExercise], muscles: &[SeedMuscle]) {
    let unit_body_parts = muscle_unit_body_parts();
    let mut by_exercise = BTreeMap::<&str, Vec<&SeedMuscle>>::new();
    for muscle in muscles {
        assert!(
            unit_body_parts.contains_key(muscle.muscle_id.as_str()),
            "the muscle {} is not one of the 24 units",
            muscle.muscle_id
        );
        assert!(
            matches!(muscle.role.as_str(), "primary" | "secondary"),
            "the role {} is invalid",
            muscle.role
        );
        by_exercise
            .entry(muscle.exercise_id.as_str())
            .or_default()
            .push(muscle);
    }

    for exercise in exercises {
        let assignments = by_exercise
            .get(exercise.id.as_str())
            .cloned()
            .unwrap_or_default();
        if exercise.body_part == "other" {
            assert!(
                assignments.is_empty(),
                "the other exercise {} must not have muscles",
                exercise.name
            );
            assert!(
                exercise.other_scope.is_some(),
                "the other exercise {} needs a scope",
                exercise.name
            );
            continue;
        }
        assert!(
            exercise.other_scope.is_none(),
            "the exercise {} must not have a scope",
            exercise.name
        );
        assert!(
            assignments
                .iter()
                .any(|assignment| assignment.role == "primary"),
            "the exercise {} needs a primary muscle",
            exercise.name
        );
        assert!(
            assignments.iter().any(|assignment| {
                assignment.role == "primary"
                    && unit_body_parts[assignment.muscle_id.as_str()] == exercise.body_part.as_str()
            }),
            "the exercise {} needs a primary muscle in its body part",
            exercise.name
        );
        let mut seen = BTreeSet::new();
        for assignment in &assignments {
            assert!(
                seen.insert(assignment.muscle_id.as_str()),
                "the exercise {} repeats the muscle {}",
                exercise.name,
                assignment.muscle_id
            );
        }
    }

    assert_representative_muscles(exercises, &by_exercise);
}

fn assert_representative_muscles(
    exercises: &[SeedExercise],
    by_exercise: &BTreeMap<&str, Vec<&SeedMuscle>>,
) {
    assert_muscles(
        exercises,
        by_exercise,
        "ベンチプレス",
        &["pectoralis_major_middle"],
        &["triceps_brachii", "anterior_deltoid"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "スクワット",
        &["quadriceps", "gluteus_maximus"],
        &["hamstrings", "erector_spinae", "adductors"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "デッドリフト",
        &[
            "erector_spinae",
            "latissimus_dorsi",
            "trapezius_middle_lower",
        ],
        &["gluteus_maximus", "hamstrings", "forearm"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "プランク",
        &[
            "rectus_abdominis_upper",
            "rectus_abdominis_lower",
            "obliques",
        ],
        &["gluteus_maximus", "quadriceps", "erector_spinae"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "フェイスプル",
        &["posterior_deltoid", "trapezius_middle_lower"],
        &["lateral_deltoid"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "ハンマーカール",
        &["biceps_brachii", "forearm"],
        &[],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "レッグエクステンション",
        &["quadriceps"],
        &[],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "バックエクステンション",
        &["erector_spinae"],
        &["gluteus_maximus", "hamstrings"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "アブローラー",
        &["rectus_abdominis_upper", "rectus_abdominis_lower"],
        &["obliques", "erector_spinae"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "パイクプッシュアップ",
        &["anterior_deltoid"],
        &["triceps_brachii", "trapezius_upper"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "インバーテッドカール",
        &["biceps_brachii"],
        &["forearm"],
    );
    assert_muscles(
        exercises,
        by_exercise,
        "バーベルロールアウト",
        &["rectus_abdominis_upper", "rectus_abdominis_lower"],
        &["obliques", "erector_spinae"],
    );
}

fn assert_muscles(
    exercises: &[SeedExercise],
    by_exercise: &BTreeMap<&str, Vec<&SeedMuscle>>,
    name: &str,
    primary: &[&str],
    secondary: &[&str],
) {
    let exercise = exercises
        .iter()
        .find(|exercise| exercise.name == name)
        .unwrap_or_else(|| panic!("{name} must exist"));
    let assignments = by_exercise
        .get(exercise.id.as_str())
        .cloned()
        .unwrap_or_default();
    let actual_primary = assignments
        .iter()
        .filter(|assignment| assignment.role == "primary")
        .map(|assignment| assignment.muscle_id.as_str())
        .collect::<BTreeSet<_>>();
    let actual_secondary = assignments
        .iter()
        .filter(|assignment| assignment.role == "secondary")
        .map(|assignment| assignment.muscle_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_primary,
        primary.iter().copied().collect::<BTreeSet<_>>(),
        "{name} has the wrong primary muscles"
    );
    assert_eq!(
        actual_secondary,
        secondary.iter().copied().collect::<BTreeSet<_>>(),
        "{name} has the wrong secondary muscles"
    );
}

fn assert_kana_order(exercises: &[SeedExercise]) {
    let body_parts = [
        "chest",
        "back",
        "shoulders",
        "biceps_forearm",
        "triceps",
        "legs",
        "abs",
        "other",
    ];
    let equipment = [
        "barbell",
        "dumbbell",
        "machine",
        "cable",
        "bodyweight",
        "other",
    ];
    let rank = |order: &[&str], value: &str| {
        order
            .iter()
            .position(|candidate| *candidate == value)
            .unwrap_or_else(|| panic!("{value} is not in {order:?}"))
    };
    for (index, exercise) in exercises.iter().enumerate() {
        assert_eq!(
            exercise.sort_order,
            (index + 1) as i64,
            "the sort order must be continuous"
        );
        assert!(
            !exercise.name_kana.is_empty(),
            "{} needs a reading",
            exercise.name
        );
        assert!(
            exercise
                .name_kana
                .chars()
                .all(|character| ('ぁ'..='ゖ').contains(&character) || character == 'ー'),
            "the reading {} must contain only hiragana and long vowel marks",
            exercise.name_kana
        );
    }
    for pair in exercises.windows(2) {
        let current_key = kana_sort_key(&pair[0].name_kana);
        let next_key = kana_sort_key(&pair[1].name_kana);
        let current = (
            rank(&body_parts, &pair[0].body_part),
            rank(&equipment, &pair[0].equipment),
            current_key.as_str(),
        );
        let next = (
            rank(&body_parts, &pair[1].body_part),
            rank(&equipment, &pair[1].equipment),
            next_key.as_str(),
        );
        assert!(
            current < next,
            "{} {} ({}) must come before {} {} ({})",
            pair[0].body_part,
            pair[0].equipment,
            pair[0].name,
            pair[1].body_part,
            pair[1].equipment,
            pair[1].name
        );
    }
}

/// 長音を直前の母音に置き換え、SQLite の既定の文字列順でも五十音順になる読みにする。
fn kana_sort_key(text: &str) -> String {
    let mut key = String::new();
    let mut previous = None;
    for character in text.chars() {
        if character == 'ー' {
            key.push(previous.and_then(long_vowel).unwrap_or(character));
        } else {
            key.push(character);
        }
        previous = Some(character);
    }
    key
}

/// 長音が続く母音を返す。
fn long_vowel(character: char) -> Option<char> {
    match character {
        'あ' | 'か' | 'が' | 'さ' | 'ざ' | 'た' | 'だ' | 'な' | 'は' | 'ば' | 'ぱ' | 'ま'
        | 'や' | 'ら' | 'わ' | 'ぁ' | 'ゃ' => Some('あ'),
        'い' | 'き' | 'ぎ' | 'し' | 'じ' | 'ち' | 'ぢ' | 'に' | 'ひ' | 'び' | 'ぴ' | 'み'
        | 'り' | 'ぃ' => Some('い'),
        'う' | 'く' | 'ぐ' | 'す' | 'ず' | 'つ' | 'づ' | 'ぬ' | 'ふ' | 'ぶ' | 'ぷ' | 'む'
        | 'ゆ' | 'る' | 'ぅ' | 'ゅ' | 'ゔ' => Some('う'),
        'え' | 'け' | 'げ' | 'せ' | 'ぜ' | 'て' | 'で' | 'ね' | 'へ' | 'べ' | 'ぺ' | 'め'
        | 'れ' | 'ぇ' => Some('え'),
        'お' | 'こ' | 'ご' | 'そ' | 'ぞ' | 'と' | 'ど' | 'の' | 'ほ' | 'ぼ' | 'ぽ' | 'も'
        | 'よ' | 'ろ' | 'を' | 'ぉ' | 'ょ' => Some('お'),
        _ => None,
    }
}

fn assert_grip_options(exercises: &[SeedExercise]) {
    for exercise in exercises {
        let mode = parse::<GripMode>(&exercise.grip_mode);
        let options = serde_json::from_str::<Vec<GripSelection>>(&exercise.grip_options)
            .unwrap_or_else(|error| {
                panic!("the grip options of {} are invalid: {error}", exercise.name)
            });
        for option in &options {
            option.validate_for(mode).unwrap_or_else(|error| {
                panic!(
                    "the grip option {option:?} of {} does not match {mode:?}: {error:?}",
                    exercise.name
                )
            });
        }
        match mode {
            GripMode::None => assert!(
                options.is_empty(),
                "{} must not have grip options",
                exercise.name
            ),
            GripMode::Grip => assert_eq!(
                options.len(),
                9,
                "{} needs the 3 styles and 3 widths",
                exercise.name
            ),
            GripMode::Attachment => assert_eq!(
                options.len(),
                12,
                "{} needs the 10 attachments and 2 styles",
                exercise.name
            ),
        }
        if parse::<RecordMode>(&exercise.record_mode) == RecordMode::Time {
            assert_eq!(mode, GripMode::None);
            assert!(options.is_empty());
        }
    }
}

fn assert_exercise_type_validation(exercises: &[SeedExercise], muscles: &[SeedMuscle]) {
    let mut by_exercise = BTreeMap::<&str, Vec<&SeedMuscle>>::new();
    for muscle in muscles {
        by_exercise
            .entry(muscle.exercise_id.as_str())
            .or_default()
            .push(muscle);
    }
    let mut ids = BTreeSet::new();
    for row in exercises {
        assert!(is_valid_uuid_v4(&row.id), "{} is not a UUID v4", row.id);
        assert!(ids.insert(row.id.as_str()), "{} is duplicated", row.id);
        assert_eq!(row.instructions, "", "{} has an instruction", row.name);
        assert_eq!(row.cautions, "", "{} has a caution", row.name);
        assert_eq!(row.tips, "", "{} has a tip", row.name);
        let assignments = by_exercise
            .get(row.id.as_str())
            .map(|items| {
                items
                    .iter()
                    .map(|item| MuscleAssignment {
                        muscle: parse::<MuscleUnit>(&item.muscle_id),
                        role: parse::<MuscleRole>(&item.role),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let exercise = Exercise {
            id: row.id.clone(),
            name: row.name.clone(),
            name_kana: row.name_kana.clone(),
            body_part: parse::<BodyPart>(&row.body_part),
            equipment: parse::<Equipment>(&row.equipment),
            record_mode: parse::<RecordMode>(&row.record_mode),
            laterality: parse::<Laterality>(&row.laterality),
            grip_mode: parse::<GripMode>(&row.grip_mode),
            other_scope: row.other_scope.as_deref().map(parse::<OtherScope>),
            instructions: row.instructions.clone(),
            cautions: row.cautions.clone(),
            tips: row.tips.clone(),
            sort_order: i32::try_from(row.sort_order).expect("the sort order must fit in i32"),
            muscles: assignments,
            grip_options: serde_json::from_str(&row.grip_options)
                .expect("the grip options must deserialize"),
        };
        exercise
            .validate()
            .unwrap_or_else(|error| panic!("{} must validate: {error:?}", row.name));
    }
}

fn assert_the_seed_is_idempotent(
    server: &DevServer,
    exercises: &[SeedExercise],
    muscles: &[SeedMuscle],
) {
    server
        .apply_migrations()
        .expect("the migrations must apply a second time");
    let after_exercises = seed_exercises(server);
    let after_muscles = seed_muscles(server);
    assert_eq!(
        after_exercises
            .iter()
            .map(|exercise| exercise.id.as_str())
            .collect::<BTreeSet<_>>(),
        exercises
            .iter()
            .map(|exercise| exercise.id.as_str())
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        after_muscles
            .iter()
            .map(|muscle| (muscle.exercise_id.as_str(), muscle.muscle_id.as_str()))
            .collect::<BTreeSet<_>>(),
        muscles
            .iter()
            .map(|muscle| (muscle.exercise_id.as_str(), muscle.muscle_id.as_str()))
            .collect::<BTreeSet<_>>()
    );
}

fn muscle_unit_body_parts() -> BTreeMap<&'static str, &'static str> {
    MuscleUnit::ALL
        .iter()
        .map(|unit| (unit.id(), unit.body_part().as_str()))
        .collect()
}

fn seed_exercises(server: &DevServer) -> Vec<SeedExercise> {
    query_rows(
        server,
        "SELECT id, name, name_kana, body_part, equipment, record_mode, laterality, \
         grip_mode, other_scope, instructions, cautions, tips, sort_order, grip_options \
         FROM master_exercises ORDER BY sort_order",
    )
    .iter()
    .map(SeedExercise::from_row)
    .collect()
}

fn seed_muscles(server: &DevServer) -> Vec<SeedMuscle> {
    query_rows(
        server,
        "SELECT exercise_id, muscle_id, role FROM master_exercise_muscles",
    )
    .iter()
    .map(SeedMuscle::from_row)
    .collect()
}

fn seed_muscle_units(server: &DevServer) -> Vec<SeedMuscleUnit> {
    query_rows(
        server,
        "SELECT id, body_part, name_ja, display_order FROM muscles ORDER BY display_order",
    )
    .iter()
    .map(SeedMuscleUnit::from_row)
    .collect()
}

fn query_rows(server: &DevServer, sql: &str) -> Vec<Value> {
    let output = server
        .execute_d1_json(sql)
        .unwrap_or_else(|error| panic!("the query must succeed: {error}"));
    output
        .get(0)
        .and_then(|entry| entry.get("results"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| panic!("the query must return rows: {output}"))
}

fn row_text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("the column {key} must be a string: {row}"))
        .to_owned()
}

fn row_optional_text(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn row_integer(row: &Value, key: &str) -> i64 {
    row.get(key)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("the column {key} must be an integer: {row}"))
}

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_value(Value::String(text.to_owned()))
        .unwrap_or_else(|error| panic!("{text} must be a valid value: {error}"))
}
