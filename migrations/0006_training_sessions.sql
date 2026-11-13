-- 記録中のトレーニング、種目、セット、ドロップセットの段、スーパーセット、適用済みの操作。
CREATE TABLE training_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    date TEXT NOT NULL,
    started_at TEXT,
    memo TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (length(memo) <= 2000),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX training_sessions_user_date ON training_sessions (user_id, date DESC);
CREATE INDEX training_sessions_user_started_at ON training_sessions (user_id, started_at DESC);

CREATE TABLE training_exercises (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    training_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    snapshot TEXT NOT NULL,
    position INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX training_exercises_user_training_position ON training_exercises (user_id, training_id, position);

CREATE TABLE sets (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    training_exercise_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    completed INTEGER NOT NULL DEFAULT 0,
    rpe REAL,
    grip TEXT,
    recorded_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (kind IN ('normal', 'warmup', 'drop')),
    CHECK (completed IN (0, 1)),
    CHECK (rpe IS NULL OR (rpe >= 6.0 AND rpe <= 10.0 AND rpe * 2 = CAST(rpe * 2 AS INTEGER))),
    CHECK (grip IS NULL OR json_valid(grip)),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_exercise_id) REFERENCES training_exercises (id) ON DELETE CASCADE
);

CREATE INDEX sets_user_training_exercise_position ON sets (user_id, training_exercise_id, position);

CREATE TABLE set_segments (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    set_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    weight_kg REAL,
    reps INTEGER,
    momentum_reps INTEGER NOT NULL DEFAULT 0,
    assisted_reps INTEGER NOT NULL DEFAULT 0,
    duration_seconds INTEGER,
    left_weight_kg REAL,
    left_reps INTEGER,
    left_duration_seconds INTEGER,
    right_weight_kg REAL,
    right_reps INTEGER,
    right_duration_seconds INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (set_id) REFERENCES sets (id) ON DELETE CASCADE
);

CREATE INDEX set_segments_user_set_position ON set_segments (user_id, set_id, position);

CREATE TABLE supersets (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    training_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX supersets_user_training_position ON supersets (user_id, training_id, position);

CREATE TABLE superset_members (
    user_id TEXT NOT NULL,
    superset_id TEXT NOT NULL,
    training_exercise_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (superset_id, training_exercise_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (superset_id) REFERENCES supersets (id) ON DELETE CASCADE,
    FOREIGN KEY (training_exercise_id) REFERENCES training_exercises (id) ON DELETE CASCADE
);

CREATE INDEX superset_members_user_training_exercise ON superset_members (user_id, training_exercise_id);

CREATE TABLE applied_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    training_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX applied_operations_user_training ON applied_operations (user_id, training_id);
