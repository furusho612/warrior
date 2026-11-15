-- ルーティン、ルーティンの種目、ルーティンの種目ごとの目標 (REQ-RTN-001)。
-- 種目のスナップショットは保存した時点の内容を持ち、種目マスターの編集の影響を受けない。
CREATE TABLE routines (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    position INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (length(trim(name)) > 0),
    CHECK (length(name) <= 100),
    CHECK (position >= 0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX routines_user_position ON routines (user_id, position);

CREATE TABLE routine_exercises (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    routine_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    snapshot TEXT NOT NULL,
    position INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (position >= 1),
    CHECK (json_valid(snapshot)),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (routine_id) REFERENCES routines (id) ON DELETE CASCADE
);

CREATE INDEX routine_exercises_user_routine_position
    ON routine_exercises (user_id, routine_id, position);

CREATE TABLE routine_exercise_targets (
    routine_exercise_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    set_count INTEGER NOT NULL,
    target_weight_kg REAL,
    target_reps INTEGER,
    target_duration_seconds INTEGER,
    grip TEXT,
    rest_seconds INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (set_count >= 1),
    CHECK (target_weight_kg IS NULL OR target_weight_kg >= 0),
    CHECK (target_reps IS NULL OR target_reps >= 1),
    CHECK (target_duration_seconds IS NULL OR
        (target_duration_seconds >= 1 AND target_duration_seconds <= 86400)),
    CHECK (grip IS NULL OR json_valid(grip)),
    CHECK (rest_seconds IS NULL OR
        (rest_seconds >= 1 AND rest_seconds <= 3600)),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (routine_exercise_id) REFERENCES routine_exercises (id) ON DELETE CASCADE
);

CREATE INDEX routine_exercise_targets_user_routine_exercise
    ON routine_exercise_targets (user_id, routine_exercise_id);
