-- トレーニングの開始元と、終了時にサーバーが計算して保存するサマリー。
ALTER TABLE training_sessions ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'empty';
ALTER TABLE training_sessions ADD COLUMN routine_id TEXT;
ALTER TABLE training_sessions ADD COLUMN program_day_id TEXT;
ALTER TABLE training_sessions ADD COLUMN custom_program_day_id TEXT;
ALTER TABLE training_sessions ADD COLUMN finished_at TEXT;
ALTER TABLE training_sessions ADD COLUMN duration_seconds INTEGER;
ALTER TABLE training_sessions ADD COLUMN total_weight_kg REAL;
ALTER TABLE training_sessions ADD COLUMN exercise_count INTEGER;
ALTER TABLE training_sessions ADD COLUMN set_count INTEGER;
ALTER TABLE training_sessions ADD COLUMN personal_bests TEXT;

CREATE INDEX training_sessions_user_finished_at ON training_sessions (user_id, finished_at);
