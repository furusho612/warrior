-- 利用者の種目別の自己ベスト。
-- 比較の単位は利用者、種目、グリップ、アタッチメント、種類であり、1 行だけを持つ。
-- グリップとアタッチメントを選ばない記録は空文字で保存する。SQLite の一意制約は
-- NULL を互いに異なる値として扱うため、NULL ではなく空文字で 1 行を保証する。
-- 時間で記録する種目の最長時間は秒を value に保存する。
CREATE TABLE personal_bests (
    user_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    grip TEXT NOT NULL DEFAULT '',
    attachment TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,
    value REAL NOT NULL,
    achieved_on TEXT NOT NULL,
    training_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, exercise_id, grip, attachment, kind),
    CHECK (kind IN ('max_weight', 'estimated_one_rep_max', 'total_weight', 'longest_duration')),
    CHECK (value > 0.0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX personal_bests_user_training ON personal_bests (user_id, training_id);
