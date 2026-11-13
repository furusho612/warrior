-- Google OAuth の状態と、サーバーが発行するセッション。
CREATE TABLE oauth_states (
    state TEXT PRIMARY KEY NOT NULL,
    nonce TEXT NOT NULL,
    code_verifier TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT
);

CREATE INDEX oauth_states_expires_at ON oauth_states (expires_at);

CREATE TABLE auth_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX auth_sessions_user_id ON auth_sessions (user_id);
