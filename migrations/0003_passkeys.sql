-- パスキーと、WebAuthn のチャレンジと、再認証の短命トークン。
CREATE TABLE passkeys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    credential_id TEXT NOT NULL UNIQUE,
    public_key TEXT NOT NULL,
    sign_count INTEGER NOT NULL,
    transports TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX passkeys_user_id ON passkeys (user_id);

CREATE TABLE webauthn_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT,
    challenge TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX webauthn_challenges_expires_at ON webauthn_challenges (expires_at);

CREATE TABLE elevation_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

ALTER TABLE oauth_states ADD COLUMN purpose TEXT NOT NULL DEFAULT 'login';
ALTER TABLE oauth_states ADD COLUMN user_id TEXT;
