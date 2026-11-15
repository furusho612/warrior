-- 利用者の種目とグリップとアタッチメントの組ごとの目標重量 (REQ-GOAL-001)。
-- 目標重量は必須で、達成したい時期は任意とする。時期の妥当性は API が検証する。
-- グリップとアタッチメントを選ばない種目は空文字で保存する。SQLite の一意制約は
-- NULL を互いに異なる値として扱うため、NULL ではなく空文字で 1 行を保証する。
CREATE TABLE exercise_goals (
    user_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    grip TEXT NOT NULL DEFAULT '',
    attachment TEXT NOT NULL DEFAULT '',
    target_weight_kg REAL NOT NULL,
    target_date TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, exercise_id, grip, attachment),
    CHECK (target_weight_kg > 0.0 AND target_weight_kg <= 1000.0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX exercise_goals_user_id ON exercise_goals (user_id);
