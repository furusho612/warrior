//! 部位区分ごとの回復状況の取得。
//!
//! 対応する要求: `REQ-ANL-008`、`REQ-ANL-009`。
//! 最後のトレーニングの時刻の規則と 3 段階の判定は `warrior_core::recovery` に任せ、
//! このモジュールは終了したトレーニングの読み込みと応答の組み立てを担う。表示の時点の
//! 時刻には、要求を処理する時点の UTC を使う (ADR-005)。

use serde::Deserialize;
use warrior_core::exercise::BodyPart;
use warrior_core::jst::UtcInstant;
use warrior_core::recovery::{last_training_instant, recovery_status, RecoveryStatus};
use warrior_core::sql::{SqlStatement, SqlValue};
use worker::{console_error, Request, Response, Result, RouteContext};

use crate::auth::{self, session};
use crate::db::Db;
use crate::error::{response, ErrorCode};
use crate::history::parse_date;

/// 回復状況の 1 行。トレーニングの時刻と、記録時の種目の部位区分を持つ。
#[derive(Debug, Deserialize)]
struct RecoveryRow {
    recovery_date: String,
    recovery_started_at: Option<String>,
    recovery_finished_at: String,
    recovery_body_part: String,
}

/// `GET /api/analysis/recovery`
///
/// 部位区分ごとの最後のトレーニングの時刻と、その時点の回復の 3 段階を返す。
/// 部位区分「その他」は含めず、最後のトレーニングが無い部位は状態を持たない。
pub async fn recovery(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let rows = match load_rows(&db, &authenticated.id).await {
        Ok(rows) => rows,
        Err(error) => {
            console_error!("failed to load the recovery status: {error}");
            return Ok(response(ErrorCode::Internal, "internal error"));
        }
    };
    let now = UtcInstant::from_epoch_seconds(auth::now_seconds());
    match recovery_response(&rows, now) {
        Ok(body) => Response::from_json(&body),
        Err(error) => {
            console_error!("failed to build the recovery status: {error}");
            Ok(response(ErrorCode::Internal, "internal error"))
        }
    }
}

fn recovery_statement(user_id: &str) -> SqlStatement {
    SqlStatement::new(
        "SELECT ts.date AS recovery_date, ts.started_at AS recovery_started_at, \
         ts.finished_at AS recovery_finished_at, \
         json_extract(te.snapshot, '$.body_part') AS recovery_body_part \
         FROM training_sessions ts \
         JOIN training_exercises te ON te.training_id = ts.id AND te.user_id = ts.user_id \
         WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL \
          AND EXISTS (SELECT 1 FROM sets s WHERE s.training_exercise_id = te.id \
           AND s.user_id = ts.user_id AND s.completed = 1) \
         ORDER BY ts.date, ts.finished_at, ts.id, te.position",
        vec![SqlValue::Text(user_id.to_owned())],
    )
}

async fn load_rows(db: &Db, user_id: &str) -> Result<Vec<RecoveryRow>> {
    db.all(&recovery_statement(user_id))
        .await?
        .results::<RecoveryRow>()
}

fn recovery_response(rows: &[RecoveryRow], now: UtcInstant) -> Result<RecoveryStatus> {
    let mut trainings = Vec::with_capacity(rows.len());
    for row in rows {
        trainings.push(row.last_training()?);
    }
    Ok(recovery_status(now, &trainings))
}

impl RecoveryRow {
    fn last_training(&self) -> Result<(BodyPart, UtcInstant)> {
        let body_part = BodyPart::parse(&self.recovery_body_part)
            .ok_or_else(|| invalid_stored("the stored body part is unknown"))?;
        let date = parse_date(&self.recovery_date)?;
        // 開始時刻を持たない記録は日付だけの記録である (ADR-005)。その終了の時刻は同期の
        // 時刻であり、トレーニングした時刻を示さないため使わない。
        let finished_at = match &self.recovery_started_at {
            Some(_) => Some(parse_instant(&self.recovery_finished_at)?),
            None => None,
        };
        let instant = last_training_instant(date, finished_at)
            .ok_or_else(|| invalid_stored("the stored recovery time is outside the range"))?;
        Ok((body_part, instant))
    }
}

fn parse_instant(text: &str) -> Result<UtcInstant> {
    UtcInstant::parse(text).ok_or_else(|| {
        #[cfg(target_arch = "wasm32")]
        console_error!("the stored finished time is invalid");
        worker::Error::RustError("the stored finished time is invalid".to_owned())
    })
}

fn invalid_stored(reason: &str) -> worker::Error {
    #[cfg(target_arch = "wasm32")]
    console_error!("the stored recovery row is invalid: {reason}");
    worker::Error::RustError(reason.to_owned())
}

#[cfg(test)]
mod tests {
    //! 回復状況の応答の組み立ての例示テスト。
    //!
    //! 対応する要求: `REQ-ANL-008`。
    //! 要求の受入基準は E2E が担い、ここは D1 の行から応答への変換、部位ごとの最後の
    //! トレーニングの選択、日付だけの記録の時刻、部位区分「その他」の除外を確かめる。
    //! 3 段階の境界と任意入力の性質は `warrior_core::recovery` と
    //! `backend/pbt/tests/recovery.rs` が担う。

    use warrior_core::exercise::BodyPart;
    use warrior_core::jst::UtcInstant;
    use warrior_core::recovery::RecoveryState;
    use warrior_core::sql::SqlValue;

    use super::{recovery_response, recovery_statement, RecoveryRow};

    const USER_ID: &str = "user-a";

    #[test]
    fn the_statement_scopes_the_user_the_finished_sessions_and_the_completed_sets() {
        let statement = recovery_statement(USER_ID);
        assert!(statement
            .sql()
            .contains("WHERE ts.user_id = ? AND ts.finished_at IS NOT NULL"));
        assert!(statement.sql().contains(
            "s.training_exercise_id = te.id AND s.user_id = ts.user_id AND s.completed = 1"
        ));
        assert!(statement
            .sql()
            .contains("json_extract(te.snapshot, '$.body_part')"));
        assert_eq!(statement.bindings(), [SqlValue::Text(USER_ID.to_owned())]);
    }

    #[test]
    fn the_response_uses_the_latest_training_of_each_part() {
        let rows = vec![
            row(
                "2026-11-11",
                Some("2026-11-11T13:00:00Z"),
                "2026-11-11T14:00:00Z",
                "chest",
            ),
            row(
                "2026-11-07",
                Some("2026-11-07T19:00:00Z"),
                "2026-11-07T20:00:00Z",
                "chest",
            ),
            row(
                "2026-11-09",
                Some("2026-11-09T21:00:00Z"),
                "2026-11-09T22:00:00Z",
                "back",
            ),
            row(
                "2026-11-08",
                Some("2026-11-08T15:00:00Z"),
                "2026-11-08T16:00:00Z",
                "legs",
            ),
            row("2026-11-11", None, "2026-11-11T23:00:00Z", "abs"),
            row(
                "2026-11-11",
                Some("2026-11-11T22:00:00Z"),
                "2026-11-11T23:00:00Z",
                "other",
            ),
        ];
        let status =
            recovery_response(&rows, instant("2026-11-12T00:00:00Z")).expect("a recovery response");
        assert_eq!(status.parts.len(), 7);
        assert!(status
            .parts
            .iter()
            .all(|part| part.body_part != BodyPart::Other));
        assert_eq!(
            state(&status, BodyPart::Chest),
            Some(RecoveryState::Recovering)
        );
        assert_eq!(
            last(&status, BodyPart::Chest).as_deref(),
            Some("2026-11-11T14:00:00Z")
        );
        assert_eq!(
            state(&status, BodyPart::Back),
            Some(RecoveryState::AlmostRecovered)
        );
        assert_eq!(
            state(&status, BodyPart::Legs),
            Some(RecoveryState::Recovered)
        );
        assert_eq!(
            last(&status, BodyPart::Abs).as_deref(),
            Some("2026-11-11T15:00:00Z"),
            "a date-only record uses the next midnight in Japan"
        );
        assert_eq!(
            state(&status, BodyPart::Abs),
            Some(RecoveryState::Recovering)
        );
        assert_eq!(last(&status, BodyPart::Shoulders), None);
        assert_eq!(state(&status, BodyPart::Shoulders), None);
    }

    #[test]
    fn the_response_rejects_an_unknown_body_part() {
        let rows = vec![row("2026-11-11", None, "2026-11-11T23:00:00Z", "arms")];
        assert!(recovery_response(&rows, instant("2026-11-12T00:00:00Z")).is_err());
    }

    #[test]
    fn the_response_rejects_an_invalid_stored_date() {
        let rows = vec![row("2026-02-30", None, "2026-11-11T23:00:00Z", "chest")];
        assert!(recovery_response(&rows, instant("2026-11-12T00:00:00Z")).is_err());
    }

    #[test]
    fn the_response_rejects_an_invalid_finished_time() {
        let rows = vec![row(
            "2026-11-11",
            Some("2026-11-11T13:00:00Z"),
            "yesterday",
            "chest",
        )];
        assert!(recovery_response(&rows, instant("2026-11-12T00:00:00Z")).is_err());
    }

    fn state(
        status: &warrior_core::recovery::RecoveryStatus,
        body_part: BodyPart,
    ) -> Option<RecoveryState> {
        status
            .parts
            .iter()
            .find(|part| part.body_part == body_part)
            .and_then(|part| part.state)
    }

    fn last(
        status: &warrior_core::recovery::RecoveryStatus,
        body_part: BodyPart,
    ) -> Option<String> {
        status
            .parts
            .iter()
            .find(|part| part.body_part == body_part)
            .and_then(|part| part.last_trained_at.clone())
    }

    fn row(
        date: &str,
        started_at: Option<&str>,
        finished_at: &str,
        body_part: &str,
    ) -> RecoveryRow {
        RecoveryRow {
            recovery_date: date.to_owned(),
            recovery_started_at: started_at.map(str::to_owned),
            recovery_finished_at: finished_at.to_owned(),
            recovery_body_part: body_part.to_owned(),
        }
    }

    fn instant(text: &str) -> UtcInstant {
        UtcInstant::parse(text).expect("a valid instant")
    }
}
