-- 種目ごとのメモ (REQ-REC-007)。トレーニングのメモは 0006 の training_sessions が持つ。
-- 文字数は SQLite の length() で数え、2000 文字を超える保存を拒む。
ALTER TABLE training_exercises ADD COLUMN memo TEXT NOT NULL DEFAULT ''
    CHECK (length(memo) <= 2000);
