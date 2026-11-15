//! ルーティンの画面の入力と表示の検証。
//!
//! 対応する要求: `REQ-RTN-001`。
//! 保存した目標の要約と、任意の入力の解釈を確かめる。

use warrior_core::exercise::ExerciseSnapshot;
use warrior_core::routine::{Routine, RoutineExercise, RoutineTarget};

use super::{parse_optional_seconds, parse_optional_weight, routine_summary};

#[test]
fn req_rtn_001_the_summary_shows_the_sets_and_reps() {
    let routine = routine_with("胸の日", vec![exercise(3, Some(5), None)]);
    assert_eq!(routine_summary(&routine), "ベンチプレス 3×5回");
}

#[test]
fn req_rtn_001_the_summary_shows_the_time_target() {
    let routine = routine_with("体幹", vec![exercise(3, None, Some(90))]);
    assert_eq!(routine_summary(&routine), "プランク 3×01:30");
}

#[test]
fn req_rtn_001_optional_inputs_accept_empty_and_ordered_values() {
    assert_eq!(parse_optional_weight(""), None);
    assert_eq!(parse_optional_weight("100.5"), Some(100.5));
    assert_eq!(parse_optional_weight("-1"), None);
    assert_eq!(parse_optional_seconds("", 3_600), None);
    assert_eq!(parse_optional_seconds("01:30", 3_600), Some(90));
    assert_eq!(parse_optional_seconds("90", 60), None);
}

fn routine_with(name: &str, exercises: Vec<RoutineExercise>) -> Routine {
    Routine {
        id: "3f2504e0-4f89-41d3-9a0c-0305e82c5501".to_owned(),
        name: name.to_owned(),
        position: 1,
        exercises,
        created_at: "2026-11-12T00:00:00Z".to_owned(),
        updated_at: "2026-11-12T00:00:00Z".to_owned(),
    }
}

fn exercise(set_count: u32, reps: Option<u32>, duration: Option<u32>) -> RoutineExercise {
    let time = duration.is_some();
    RoutineExercise {
        id: "3f2504e0-4f89-41d3-9a0c-0305e82c5502".to_owned(),
        routine_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5501".to_owned(),
        exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5503".to_owned(),
        snapshot: snapshot(time),
        position: 1,
        target: RoutineTarget {
            set_count,
            weight_kg: if time { None } else { Some(100.0) },
            reps,
            duration_seconds: duration,
            grip: None,
            rest_seconds: None,
        },
        created_at: "2026-11-12T00:00:00Z".to_owned(),
        updated_at: "2026-11-12T00:00:00Z".to_owned(),
    }
}

fn snapshot(time: bool) -> ExerciseSnapshot {
    use warrior_core::exercise::{
        BodyPart, GripMode, Laterality, MuscleAssignment, MuscleRole, MuscleUnit,
    };
    use warrior_core::set_calc::RecordMode;
    ExerciseSnapshot {
        exercise_id: "3f2504e0-4f89-41d3-9a0c-0305e82c5503".to_owned(),
        name: if time {
            "プランク"
        } else {
            "ベンチプレス"
        }
        .to_owned(),
        body_part: BodyPart::Chest,
        muscles: vec![MuscleAssignment {
            muscle: MuscleUnit::PectoralisMajorMiddle,
            role: MuscleRole::Primary,
        }],
        record_mode: if time {
            RecordMode::Time
        } else {
            RecordMode::WeightAndReps
        },
        laterality: Laterality::Bilateral,
        grip_mode: GripMode::None,
        grip_options: Vec::new(),
    }
}
