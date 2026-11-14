-- 利用者の身体情報、目的、週あたりのトレーニング可能日数、日常の活動量。
-- 保持するのは最新値だけであり、推移は記録しない。
CREATE TABLE profiles (
    user_id TEXT PRIMARY KEY NOT NULL,
    height_cm REAL,
    weight_kg REAL,
    birth_date TEXT,
    sex TEXT,
    training_experience TEXT,
    goal TEXT,
    training_days_per_week INTEGER,
    activity_level TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (height_cm IS NULL OR (height_cm >= 100.0 AND height_cm <= 250.0)),
    CHECK (weight_kg IS NULL OR (weight_kg >= 20.0 AND weight_kg <= 300.0)),
    CHECK (sex IS NULL OR sex IN ('male', 'female', 'other')),
    CHECK (training_experience IS NULL OR training_experience IN ('beginner', 'intermediate', 'advanced')),
    CHECK (goal IS NULL OR goal IN ('hypertrophy', 'diet', 'body_make', 'strength', 'performance')),
    CHECK (training_days_per_week IS NULL OR (training_days_per_week >= 0 AND training_days_per_week <= 7)),
    CHECK (activity_level IS NULL OR activity_level IN ('low', 'normal', 'high')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- 自重種目のセットを記録した時点の体重。身体情報を更新しても変わらない。
ALTER TABLE sets ADD COLUMN body_weight_kg REAL;
