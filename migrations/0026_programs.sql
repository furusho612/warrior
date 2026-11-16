-- プログラムの構成、推奨、取り込み、進行 (REQ-PRG-001、REQ-PRG-002、REQ-PRG-019)。
-- プログラムと日はマスターの表に置き、利用者に属さない。
-- 日ごとの種目は、種目名、セット数、回数または時間、レスト時間の JSON の配列で持つ。
-- 取り込んだルーティンは source_program_day_id で元の日を指す。
CREATE TABLE programs (
    id TEXT PRIMARY KEY NOT NULL,
    goal TEXT NOT NULL,
    level TEXT NOT NULL,
    days_per_week INTEGER NOT NULL,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    CHECK (goal IN ('hypertrophy', 'diet', 'body_make', 'strength', 'performance')),
    CHECK (level IN ('beginner', 'intermediate', 'advanced')),
    CHECK (days_per_week >= 2 AND days_per_week <= 5)
);

CREATE TABLE program_days (
    id TEXT PRIMARY KEY NOT NULL,
    program_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL,
    exercises TEXT NOT NULL,
    UNIQUE (program_id, position),
    CHECK (position >= 1),
    CHECK (json_valid(exercises)),
    FOREIGN KEY (program_id) REFERENCES programs (id) ON DELETE CASCADE
);

ALTER TABLE routines ADD COLUMN source_program_day_id TEXT;
ALTER TABLE routines ADD COLUMN source_custom_program_day_id TEXT;
CREATE INDEX routines_source_program_day ON routines (user_id, source_program_day_id);

CREATE TABLE custom_programs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (length(trim(name)) > 0),
    CHECK (length(name) <= 100),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX custom_programs_user ON custom_programs (user_id, created_at);

CREATE TABLE custom_program_days (
    id TEXT PRIMARY KEY NOT NULL,
    custom_program_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    routine_id TEXT NOT NULL,
    UNIQUE (custom_program_id, position),
    CHECK (position >= 1),
    FOREIGN KEY (custom_program_id) REFERENCES custom_programs (id) ON DELETE CASCADE,
    FOREIGN KEY (routine_id) REFERENCES routines (id) ON DELETE RESTRICT
);

CREATE TABLE user_program_progress (
    user_id TEXT PRIMARY KEY NOT NULL,
    program_id TEXT,
    custom_program_id TEXT,
    day_position INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (day_position >= 1),
    CHECK ((program_id IS NOT NULL) <> (custom_program_id IS NOT NULL)),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (program_id) REFERENCES programs (id) ON DELETE CASCADE,
    FOREIGN KEY (custom_program_id) REFERENCES custom_programs (id) ON DELETE CASCADE
);
