-- 利用者の種目別の自己ベスト。
-- 比較の単位は利用者、種目、グリップ、アタッチメント、種類であり、1 行だけを持つ。
-- グリップとアタッチメントを選ばない記録は空文字で保存する。SQLite の一意制約は
-- NULL を互いに異なる値として扱うため、NULL ではなく空文字で 1 行を保証する。
-- 時間で記録する種目の最長時間は秒を value に保存する。
CREATE TABLE personal_bests (
    user_id TEXT NOT NULL,
    exercise_id TEXT NOT NULL,
    grip TEXT NOT NULL DEFAULT '',
    attachment TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL,
    value REAL NOT NULL,
    achieved_on TEXT NOT NULL,
    training_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (user_id, exercise_id, grip, attachment, kind),
    CHECK (kind IN ('max_weight', 'estimated_one_rep_max', 'total_weight', 'longest_duration')),
    CHECK (value > 0.0),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (training_id) REFERENCES training_sessions (id) ON DELETE CASCADE
);

CREATE INDEX personal_bests_user_training ON personal_bests (user_id, training_id);

-- 旧 training_sessions.personal_bests の JSON を新しい表へ移す。
-- 同じ比較の単位が複数のトレーニングにある場合は、値が最大の行だけを残す。
-- 値が等しい場合は、先に達成したトレーニングの日付と ID を残す。
INSERT INTO personal_bests
    (user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id,
     created_at, updated_at)
SELECT user_id, exercise_id, grip, attachment, kind, value, achieved_on, training_id,
       created_at, updated_at
FROM (
    SELECT ts.user_id AS user_id,
           json_extract(best.value, '$.exercise_id') AS exercise_id,
           COALESCE(json_extract(best.value, '$.grip'), '') AS grip,
           COALESCE(json_extract(best.value, '$.attachment'), '') AS attachment,
           json_extract(best.value, '$.kind') AS kind,
           CAST(json_extract(best.value, '$.value') AS REAL) AS value,
           ts.date AS achieved_on,
           ts.id AS training_id,
           ts.created_at AS created_at,
           ts.updated_at AS updated_at,
           ROW_NUMBER() OVER (
               PARTITION BY ts.user_id,
                            json_extract(best.value, '$.exercise_id'),
                            COALESCE(json_extract(best.value, '$.grip'), ''),
                            COALESCE(json_extract(best.value, '$.attachment'), ''),
                            json_extract(best.value, '$.kind')
               ORDER BY CAST(json_extract(best.value, '$.value') AS REAL) DESC,
                        ts.date ASC, ts.id ASC
           ) AS best_rank
    FROM training_sessions ts, json_each(ts.personal_bests) AS best
    WHERE ts.personal_bests IS NOT NULL
)
WHERE best_rank = 1
  AND value > 0.0
  AND kind IN ('max_weight', 'estimated_one_rep_max', 'total_weight', 'longest_duration');

-- 自己ベストは新しい表だけが持つ。
ALTER TABLE training_sessions DROP COLUMN personal_bests;
