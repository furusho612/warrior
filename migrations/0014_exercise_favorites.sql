-- 利用者の種目のお気に入り。
-- マスターの種目と独自種目は別の表に置くため、種別と種目 ID の組で表す。
-- 利用者ごとに 1 行だけを持つ。
CREATE TABLE exercise_favorites (
    user_id TEXT NOT NULL,
    exercise_kind TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (user_id, exercise_kind, exercise_id),
    CHECK (exercise_kind IN ('master', 'user')),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX exercise_favorites_user_id ON exercise_favorites (user_id);
