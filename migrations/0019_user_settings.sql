-- 利用者共通の設定。レスト時間の既定値、表示モード、PFC の配分を 1 行で持つ。
-- 端末ごとの設定 (画面の消灯防止、ブラウザ通知、プッシュ通知) は端末内の meta に置く。
CREATE TABLE user_settings (
    user_id TEXT PRIMARY KEY NOT NULL,
    rest_default_seconds INTEGER NOT NULL DEFAULT 90,
    display_mode TEXT NOT NULL DEFAULT 'system',
    protein_g_per_kg REAL NOT NULL DEFAULT 2.0,
    fat_ratio REAL NOT NULL DEFAULT 0.20,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (rest_default_seconds >= 1 AND rest_default_seconds <= 3600),
    CHECK (display_mode IN ('system', 'light', 'dark')),
    CHECK (protein_g_per_kg >= 1.6 AND protein_g_per_kg <= 2.2),
    CHECK (fat_ratio >= 0.0 AND fat_ratio <= 1.0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
