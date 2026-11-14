-- 端末トークンと、記録中のトレーニングの編集権を持つ端末 (ADR-003)。
CREATE TABLE terminal_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX terminal_tokens_user_id ON terminal_tokens (user_id);

-- 記録中のトレーニングの編集権を持つ端末トークンの ID。
-- terminal_tokens の削除でトレーニングの記録を壊さないため、外部キーは張らない。
ALTER TABLE training_sessions ADD COLUMN owner_terminal_id TEXT;
