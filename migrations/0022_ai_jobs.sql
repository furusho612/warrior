-- AI の生成ジョブの状態と、AI の入出力スナップショットのメタデータ (ADR-017、ADR-021)。
-- 入出力の本文は R2 に置き、D1 にはキーと版だけを持つ。
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
    last_error_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (user_id, unique_key),
    CHECK (kind IN ('daily_evaluation', 'set_proposal', 'weekly_evaluation', 'monthly_evaluation')),
    CHECK (status IN ('pending', 'enqueued', 'generated', 'failed')),
    CHECK (attempts >= 0 AND attempts <= 4),
    CHECK (
        (kind = 'daily_evaluation' AND training_id IS NOT NULL AND exercise_id IS NULL
            AND period_start IS NULL AND period_end IS NULL)
        OR (kind = 'set_proposal' AND training_id IS NOT NULL AND exercise_id IS NOT NULL
            AND period_start IS NULL AND period_end IS NULL)
        OR (kind IN ('weekly_evaluation', 'monthly_evaluation') AND training_id IS NULL
            AND exercise_id IS NULL AND period_start IS NOT NULL AND period_end IS NOT NULL)
    ),
    CHECK (status = 'enqueued' OR lease_expires_at IS NULL),
    CHECK (status = 'pending' OR next_attempt_at IS NULL),
    CHECK (status <> 'generated' OR output_id = id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX ai_generation_pending
    ON ai_generation (status, next_attempt_at);
CREATE INDEX ai_generation_user_created
    ON ai_generation (user_id, created_at DESC);

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
    CHECK (kind IN ('daily_evaluation', 'set_proposal', 'weekly_evaluation', 'monthly_evaluation')),
    FOREIGN KEY (id) REFERENCES ai_generation (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX ai_outputs_user_created
    ON ai_outputs (user_id, created_at DESC);
