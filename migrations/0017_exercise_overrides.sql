-- 独自種目の出典と、マスターの種目の上書き、操作の冪等性。
--
-- 種目マスターと独自種目は別の表に分かれている (ADR-015)。
-- 上書きの表は 0005 で作った master_exercise_overrides を使い、ここでは
-- 一覧の読み取りと操作の再送に必要な列と索引だけを足す。
ALTER TABLE master_exercises ADD COLUMN source TEXT NOT NULL DEFAULT 'master'
    CHECK (source = 'master');
ALTER TABLE user_exercises ADD COLUMN source TEXT NOT NULL DEFAULT 'custom'
    CHECK (source = 'custom');

CREATE INDEX master_exercise_overrides_user_id ON master_exercise_overrides (user_id);
CREATE INDEX user_exercises_user_kana ON user_exercises (user_id, name_kana);

-- 独自種目の追加と、マスターの編集、削除、初期化の再送を重複させない。
CREATE TABLE exercise_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX exercise_operations_user_id ON exercise_operations (user_id);
