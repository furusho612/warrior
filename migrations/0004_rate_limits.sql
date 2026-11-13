-- 固定の時間窓のレート制限のカウンタ。
CREATE TABLE user_rate_limits (
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    count INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (user_id, kind, window_start),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE ip_rate_limits (
    ip TEXT NOT NULL,
    kind TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    count INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    PRIMARY KEY (ip, kind, window_start)
);
