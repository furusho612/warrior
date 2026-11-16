-- メニューの最適化の生成と、作り直しの元の出力を扱う (REQ-AI-010、REQ-AI-015)。
-- SQLite は CHECK 制約を変更できないため、表を作り直して既存の行を写す。
PRAGMA foreign_keys = OFF;

ALTER TABLE ai_outputs RENAME TO ai_outputs_old;
ALTER TABLE ai_generation RENAME TO ai_generation_old;
DROP INDEX IF EXISTS ai_generation_pending;
DROP INDEX IF EXISTS ai_generation_user_created;
DROP INDEX IF EXISTS ai_outputs_user_created;

CREATE TABLE ai_generation (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    training_id TEXT,
    exercise_id TEXT,
    period_start TEXT,
    period_end TEXT,
    unique_key TEXT NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    lease_expires_at TEXT,
    next_attempt_at TEXT,
    output_id TEXT,
    redo_of TEXT,
    last_error_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (user_id, unique_key),
    CHECK (kind IN ('daily_evaluation', 'set_proposal', 'weekly_evaluation', 'monthly_evaluation', 'menu_optimization')),
    CHECK (status IN ('pending', 'enqueued', 'generated', 'failed')),
    CHECK (attempts >= 0 AND attempts <= 4),
    CHECK (
        (kind = 'daily_evaluation' AND training_id IS NOT NULL AND exercise_id IS NULL
            AND period_start IS NULL AND period_end IS NULL)
        OR (kind = 'set_proposal' AND training_id IS NOT NULL AND exercise_id IS NOT NULL
            AND period_start IS NULL AND period_end IS NULL)
        OR (kind IN ('weekly_evaluation', 'monthly_evaluation', 'menu_optimization')
            AND training_id IS NULL AND exercise_id IS NULL
            AND period_start IS NOT NULL AND period_end IS NOT NULL)
    ),
    CHECK (status = 'enqueued' OR lease_expires_at IS NULL),
    CHECK (status = 'pending' OR next_attempt_at IS NULL),
    CHECK (status <> 'generated' OR output_id = id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

INSERT INTO ai_generation
    (id, user_id, kind, training_id, exercise_id, period_start, period_end, unique_key,
     status, attempts, lease_expires_at, next_attempt_at, output_id, redo_of,
     last_error_code, created_at, updated_at)
SELECT id, user_id, kind, training_id, exercise_id, period_start, period_end, unique_key,
       status, attempts, lease_expires_at, next_attempt_at, output_id, NULL,
       last_error_code, created_at, updated_at
FROM ai_generation_old;

CREATE INDEX ai_generation_pending ON ai_generation (status, next_attempt_at);
CREATE INDEX ai_generation_user_created ON ai_generation (user_id, created_at DESC);

CREATE TABLE ai_outputs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    training_id TEXT,
    exercise_id TEXT,
    model TEXT NOT NULL,
    prompt_version TEXT NOT NULL,
    input_key TEXT NOT NULL,
    output_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    CHECK (kind IN ('daily_evaluation', 'set_proposal', 'weekly_evaluation', 'monthly_evaluation', 'menu_optimization')),
    FOREIGN KEY (id) REFERENCES ai_generation (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

INSERT INTO ai_outputs
    (id, user_id, kind, training_id, exercise_id, model, prompt_version,
     input_key, output_key, created_at)
SELECT id, user_id, kind, training_id, exercise_id, model, prompt_version,
       input_key, output_key, created_at
FROM ai_outputs_old;

CREATE INDEX ai_outputs_user_created ON ai_outputs (user_id, created_at DESC);

DROP TABLE ai_outputs_old;
DROP TABLE ai_generation_old;
PRAGMA foreign_keys = ON;
