-- レスト終了のプッシュ通知の予約 (REQ-TMR-009、REQ-TMR-010、ADR-008)。
-- 予約は版を持ち、延長と短縮では版を進めて古い Queue のメッセージを無視する。
-- 時刻は Unix 時刻の秒で持つ。通知は終了予定時刻から 30 秒以内にだけ送る。
CREATE TABLE rest_push_reservations (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    training_id TEXT NOT NULL,
    terminal_token_id TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    end_at INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    sent_at INTEGER,
    received_at INTEGER,
    enqueued_at INTEGER,
    lease_expires_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX rest_push_reservations_due
    ON rest_push_reservations (active, sent_at, end_at);
CREATE INDEX rest_push_reservations_training
    ON rest_push_reservations (user_id, training_id, terminal_token_id, active);
