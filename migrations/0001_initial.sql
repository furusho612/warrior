-- 利用者自身の行を持つ最小の表。
-- 認証の列は後続の変更で追加する。
CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    google_sub TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
