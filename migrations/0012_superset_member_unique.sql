-- 1 つの種目は 1 つの組にだけ属する。
-- 同じ種目を複数の組へ入れる操作を、事前検査の競合にかかわらずデータベースで拒否する。
DROP INDEX IF EXISTS superset_members_user_training_exercise;
CREATE UNIQUE INDEX superset_members_user_training_exercise
    ON superset_members (user_id, training_exercise_id);
