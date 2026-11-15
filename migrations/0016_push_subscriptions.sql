-- 端末ごとの Web Push の購読 (REQ-TMR-008、ADR-008)。
-- endpoint は利用者ごとに 1 行とし、同じ endpoint を送り直したら鍵と端末を更新する。
-- 端末トークンの削除と利用者の削除で、購読も消す。
CREATE TABLE push_subscriptions (
    user_id TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    terminal_token_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, endpoint),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (terminal_token_id) REFERENCES terminal_tokens (id) ON DELETE CASCADE
);

CREATE INDEX push_subscriptions_user_terminal
    ON push_subscriptions (user_id, terminal_token_id);
