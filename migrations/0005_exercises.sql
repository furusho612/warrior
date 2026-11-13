-- 種目マスター、筋肉の単位、利用者の独自種目、マスターの上書き。
CREATE TABLE muscles (
    id TEXT PRIMARY KEY NOT NULL,
    body_part TEXT NOT NULL,
    name_ja TEXT NOT NULL,
    display_order INTEGER NOT NULL UNIQUE
);

CREATE TABLE master_exercises (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    name_kana TEXT NOT NULL,
    body_part TEXT NOT NULL,
    equipment TEXT NOT NULL,
    record_mode TEXT NOT NULL,
    laterality TEXT NOT NULL,
    grip_mode TEXT NOT NULL,
    other_scope TEXT,
    instructions TEXT NOT NULL,
    cautions TEXT NOT NULL,
    tips TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    grip_options TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE master_exercise_muscles (
    exercise_id TEXT NOT NULL,
    muscle_id TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (exercise_id, muscle_id),
    FOREIGN KEY (exercise_id) REFERENCES master_exercises (id) ON DELETE CASCADE,
    FOREIGN KEY (muscle_id) REFERENCES muscles (id)
);

CREATE TABLE user_exercises (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    name_kana TEXT NOT NULL,
    body_part TEXT NOT NULL,
    equipment TEXT NOT NULL,
    record_mode TEXT NOT NULL,
    laterality TEXT NOT NULL,
    grip_mode TEXT NOT NULL,
    other_scope TEXT,
    instructions TEXT NOT NULL,
    cautions TEXT NOT NULL,
    tips TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    grip_options TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX user_exercises_user_id ON user_exercises (user_id);

CREATE TABLE user_exercise_muscles (
    exercise_id TEXT NOT NULL,
    muscle_id TEXT NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (exercise_id, muscle_id),
    FOREIGN KEY (exercise_id) REFERENCES user_exercises (id) ON DELETE CASCADE,
    FOREIGN KEY (muscle_id) REFERENCES muscles (id)
);

CREATE TABLE master_exercise_overrides (
    user_id TEXT NOT NULL,
    master_exercise_id TEXT NOT NULL,
    name TEXT,
    name_kana TEXT,
    body_part TEXT,
    equipment TEXT,
    record_mode TEXT,
    laterality TEXT,
    grip_mode TEXT,
    other_scope TEXT,
    instructions TEXT,
    cautions TEXT,
    tips TEXT,
    sort_order INTEGER,
    grip_options TEXT,
    muscles TEXT,
    deleted INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, master_exercise_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (master_exercise_id) REFERENCES master_exercises (id) ON DELETE CASCADE
);
