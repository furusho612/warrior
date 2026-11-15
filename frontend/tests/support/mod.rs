//! 端末内の保存のブラウザテストが共有する準備。

#![allow(dead_code)]

use std::cell::Cell;

use serde_json::json;
use warrior_core::sync::{Operation, OperationKind, SessionCreatePayload, SessionRecord};
use warrior_core::training_record::{TrainingExerciseRecord, TrainingRecord};
use warrior_core::training_session::SessionSource;
use warrior_frontend::storage::{Storage, Store};

pub mod exercise_tree;
pub mod set;

thread_local! {
    static SEQUENCE: Cell<u32> = const { Cell::new(0) };
}

/// テストごとに一意なデータベースの名前。
pub fn database_name(test: &str) -> String {
    let sequence = SEQUENCE.with(|sequence| {
        let next = sequence.get() + 1;
        sequence.set(next);
        next
    });
    format!(
        "warrior-test-{test}-{}-{sequence}",
        js_sys::Date::now() as u64
    )
}

/// テスト用のデータベースを開く。
pub async fn open(name: &str) -> Storage {
    Storage::open_with(name, warrior_frontend::storage::SCHEMA_VERSION)
        .await
        .expect("the database must open")
}

/// テストで使うトレーニングの ID。
pub const TRAINING_ID: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c4301";

/// テスト用のトレーニングを作る。
pub fn session(memo: &str) -> SessionRecord {
    session_with_id(TRAINING_ID, memo)
}

/// ID を指定してテスト用のトレーニングを作る。
pub fn session_with_id(id: &str, memo: &str) -> SessionRecord {
    SessionRecord {
        id: id.to_owned(),
        date: "2026-11-12".to_owned(),
        started_at: Some("2026-11-12T03:00:00Z".to_owned()),
        memo: memo.to_owned(),
        confirmed_memo: String::new(),
        confirmed_sequence: 0,
        source_kind: SessionSource::Empty,
        routine_id: None,
        program_day_id: None,
        custom_program_day_id: None,
        finished_at: None,
        duration_seconds: None,
        total_weight_kg: None,
        exercise_count: None,
        set_count: None,
        personal_bests: Vec::new(),
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

/// メモだけを指定して作成の操作を作る。
pub fn create_operation(operation_id: &str) -> Operation {
    create_operation_with_memo(operation_id, "")
}

/// メモを指定して作成の操作を作る。
pub fn create_operation_with_memo(operation_id: &str, memo: &str) -> Operation {
    create_operation_for(TRAINING_ID, operation_id, memo)
}

/// トレーニングの ID を指定して作成の操作を作る。
pub fn create_operation_for(training_id: &str, operation_id: &str, memo: &str) -> Operation {
    Operation {
        operation_id: operation_id.to_owned(),
        training_id: training_id.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingSessionCreate,
        parent_operation_id: None,
        payload: serde_json::to_value(SessionCreatePayload {
            id: training_id.to_owned(),
            date: "2026-11-12".to_owned(),
            started_at: "2026-11-12T03:00:00Z".to_owned(),
            memo: memo.to_owned(),
            source_kind: SessionSource::Empty,
            routine_id: None,
            program_day_id: None,
            custom_program_day_id: None,
        })
        .expect("a create payload"),
    }
}

/// 作成をサーバーが受理した状態のトレーニングを作る。
pub async fn create_and_confirm(storage: &Storage, operation_id: &str, memo: &str) {
    storage
        .save_session_create(
            &session(memo),
            &create_operation_with_memo(operation_id, memo),
        )
        .await
        .expect("the create must be saved");
    storage
        .mark_success(operation_id)
        .await
        .expect("the create must succeed");
}

/// サーバーが返す 1 つのトレーニングの記録。
pub fn server_record(training_id: &str, memo: &str) -> TrainingRecord {
    serde_json::from_value(json!({
        "session": {
            "id": training_id,
            "date": "2026-11-12",
            "started_at": "2026-11-12T03:00:00Z",
            "memo": memo,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T04:00:00Z"
        },
        "exercises": [{
            "id": "exercise-1",
            "training_id": training_id,
            "exercise_id": "bench-press",
            "snapshot": "{}",
            "position": 1,
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }],
        "sets": [{
            "id": "set-1",
            "training_exercise_id": "exercise-1",
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
            "training_id": training_id,
            "position": 1,
            "members": [{
                "superset_id": "superset-1",
                "training_exercise_id": "exercise-1",
                "position": 1
            }],
            "created_at": "2026-11-12T03:00:00Z",
            "updated_at": "2026-11-12T03:00:00Z"
        }]
    }))
    .expect("the server record must deserialize")
}

/// ストアの行の ID を並べる。
pub async fn record_ids(storage: &Storage, store: Store) -> Vec<String> {
    storage
        .all_records(store)
        .await
        .expect("the store must be readable")
        .iter()
        .map(|record| record["id"].as_str().expect("a record id").to_owned())
        .collect()
}

/// テスト用の種目の行。
pub fn exercise_row(id: &str, exercise_id: &str, position: i64) -> TrainingExerciseRecord {
    TrainingExerciseRecord {
        id: id.to_owned(),
        training_id: TRAINING_ID.to_owned(),
        exercise_id: exercise_id.to_owned(),
        snapshot: "{}".to_owned(),
        memo: String::new(),
        confirmed_memo: String::new(),
        position,
        created_at: "2026-11-12T03:00:00Z".to_owned(),
        updated_at: "2026-11-12T03:00:00Z".to_owned(),
    }
}

/// テスト用の種目の追加の操作。
pub fn create_exercise_operation(id: &str) -> Operation {
    Operation {
        operation_id: format!("3f2504e0-4f89-41d3-9a0c-0305e82c57{}", &id[35..36]),
        training_id: TRAINING_ID.to_owned(),
        sequence: 0,
        kind: OperationKind::TrainingExerciseCreate,
        parent_operation_id: None,
        payload: json!({
            "training_id": TRAINING_ID,
            "training_exercise_id": id,
            "exercise_id": "bench-press",
            "snapshot": {
                "exercise_id": "bench-press",
                "name": "ベンチプレス",
                "body_part": "chest",
                "muscles": [],
                "record_mode": "weight_and_reps",
                "laterality": "bilateral",
                "grip_mode": "none",
                "grip_options": []
            },
            "position": 1
        }),
    }
}

/// 種目を保存して、サーバーが受理した状態にする。
pub async fn save_exercise(storage: &Storage, row: TrainingExerciseRecord) {
    let operation = create_exercise_operation(&row.id);
    storage
        .save_training_exercise_create(&row, &operation)
        .await
        .expect("the exercise must be saved");
    storage
        .mark_success(&operation.operation_id)
        .await
        .expect("the exercise creation must succeed");
}

/// レスト終了の音と振動のブラウザの API を差し替え、呼び出しを数える。
pub fn install_sound_stub() {
    js_sys::eval(
        r#"
        window.__restAudioStarts = 0;
        window.__restVibrations = 0;
        class FakeAudioParam {
            constructor() { this.value = 0; }
            setValueAtTime(value) { this.value = value; return this; }
        }
        class FakeOscillator {
            constructor() { this.frequency = new FakeAudioParam(); }
            connect() {}
            start() { window.__restAudioStarts++; }
            stop() {}
        }
        class FakeGain {
            constructor() { this.gain = new FakeAudioParam(); }
            connect() {}
        }
        class FakeAudioContext {
            constructor() {
                this.currentTime = 0;
                this.state = 'running';
                this.destination = {};
            }
            createOscillator() { return new FakeOscillator(); }
            createGain() { return new FakeGain(); }
            resume() { this.state = 'running'; return Promise.resolve(); }
        }
        Object.defineProperty(window, 'AudioContext', {
            configurable: true,
            value: FakeAudioContext
        });
        Object.defineProperty(navigator, 'vibrate', {
            configurable: true,
            value: function () { window.__restVibrations++; return true; }
        });
        "#,
    )
    .expect("the sound stub must be installed");
}

/// `window` の下の数値のカウンタを読む。
pub fn counter(name: &str) -> f64 {
    js_sys::eval(&format!("window.{name} || 0"))
        .expect("the counter must be readable")
        .as_f64()
        .unwrap_or(0.0)
}
