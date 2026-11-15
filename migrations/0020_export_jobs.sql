-- 記録の CSV の書き出しのメタデータ (REQ-MYP-008、ADR-015、ADR-017)。
-- CSV の本体は R2 の exports/<利用者 ID>/<書き出し ID>.csv に置き、D1 にはメタデータだけを持つ。
-- status は書き出しの状態である。R2 への保存が終わった書き出しだけを ready として保存する。
CREATE TABLE export_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    status TEXT NOT NULL,
    r2_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX export_jobs_user_created ON export_jobs (user_id, created_at);
