//! レスト終了のプッシュ通知の予約、取消、再予約、送信 (REQ-TMR-009、REQ-TMR-010、ADR-008)。
//!
//! 予約は D1 の `rest_push_reservations` に置き、Queue には予約 ID と版だけを入れる。
//! 消費側は予約の版と時刻を確かめ、送信の直前に `sent_at` を立てて最大 1 回に抑える。
//! 通知の本文は予約の識別子、版、終了予定時刻だけで、記録の内容を含めない。

use serde::{Deserialize, Serialize};
use warrior_core::rest_push::{
    delay_seconds, end_at, next_version, should_send, ttl_seconds, ReservationState,
    RestPushMessage, RestPushPayload, MAX_REST_SECONDS, MIN_REST_SECONDS,
};
use warrior_core::sql::{SqlStatement, SqlValue};
use warrior_core::web_push::SALT_LEN;
use warrior_core::web_push::{self, PushRandomness, PushRequest, VapidKeys};
use worker::{
    console_error, console_log, Env, Fetch, Headers, MessageBuilder, Method, Request, RequestInit,
    Response, Result, RouteContext,
};

use crate::auth::{self, now_seconds, session};
use crate::db::{changed_rows, Db};
use crate::error::{response, ErrorCode};
use crate::ownership;

const QUEUE_BINDING: &str = "JOBS_QUEUE";
const QUEUE_LEASE_SECONDS: i64 = 10;

/// 予約の作成と変更の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReservationResponse {
    id: String,
    version: i64,
    end_at: i64,
}

/// 予約の作成の入力。
#[derive(Debug, Deserialize)]
struct CreateInput {
    training_id: String,
    remaining_seconds: u32,
}

/// 予約の変更の入力。
#[derive(Debug, Deserialize)]
struct UpdateInput {
    remaining_seconds: u32,
}

/// 通知の受信の入力。
#[derive(Debug, Deserialize)]
struct ReceivedInput {
    reservation_id: String,
    version: i64,
}

/// D1 から読む予約の行。
#[derive(Debug, Deserialize)]
struct ReservationRow {
    id: String,
    user_id: String,
    terminal_token_id: String,
    version: i64,
    end_at: i64,
    active: i64,
    sent_at: Option<i64>,
}

impl ReservationRow {
    fn state(&self) -> ReservationState {
        ReservationState {
            active: self.active == 1,
            version: self.version,
            end_at: self.end_at,
            sent_at: self.sent_at,
        }
    }
}

/// Queue へ送る予約。
#[derive(Debug, Deserialize)]
struct DueRow {
    id: String,
    version: i64,
}

/// 購読の行。
#[derive(Debug, Deserialize)]
struct SubscriptionRow {
    endpoint: String,
    p256dh: String,
    auth: String,
}

/// `POST /api/rest/reservations`
pub async fn create(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let mut request = request;
    let input: CreateInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the request is invalid")),
    };
    if !(MIN_REST_SECONDS..=MAX_REST_SECONDS).contains(&input.remaining_seconds) {
        return Ok(response(
            ErrorCode::BadRequest,
            "the remaining seconds are out of range",
        ));
    }
    let db = Db::new(env.d1("DB")?);
    let Some(terminal_id) = ownership::ensure(&db, &request, &authenticated.id).await? else {
        return Ok(response(ErrorCode::BadRequest, "the terminal is unknown"));
    };
    if !training_is_open(&db, &authenticated.id, &input.training_id, &terminal_id).await? {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    let now = now_seconds();
    let reservation_id = crate::random::random_uuid()?;
    let end_at = end_at(now, input.remaining_seconds);
    let results = db
        .batch(&create_statements(&CreateReservation {
            id: &reservation_id,
            user_id: &authenticated.id,
            training_id: &input.training_id,
            terminal_id: &terminal_id,
            end_at,
            now,
        }))
        .await?;
    let saved = results
        .get(1)
        .and_then(|result| result.results::<ReservationResponse>().ok())
        .and_then(|rows| rows.into_iter().next());
    let Some(response_body) = saved else {
        console_error!("the rest push reservation was not saved");
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    if let Err(error) = enqueue(&env, &response_body.id, response_body.version, now).await {
        console_error!("failed to enqueue the rest push reservation: {error}");
    }
    Ok(Response::from_json(&response_body)?.with_status(201))
}

struct CreateReservation<'a> {
    id: &'a str,
    user_id: &'a str,
    training_id: &'a str,
    terminal_id: &'a str,
    end_at: i64,
    now: i64,
}

fn create_statements(reservation: &CreateReservation<'_>) -> Vec<SqlStatement> {
    vec![
        deactivate_training_statement(
            reservation.user_id,
            reservation.training_id,
            reservation.terminal_id,
            reservation.now,
            true,
        ),
        SqlStatement::new(
            "INSERT INTO rest_push_reservations \
             (id, user_id, training_id, terminal_token_id, version, end_at, active, sent_at, \
              received_at, enqueued_at, lease_expires_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, 1, ?, 1, NULL, NULL, NULL, NULL, ?, ?) \
             RETURNING id, version, end_at",
            vec![
                SqlValue::Text(reservation.id.to_owned()),
                SqlValue::Text(reservation.user_id.to_owned()),
                SqlValue::Text(reservation.training_id.to_owned()),
                SqlValue::Text(reservation.terminal_id.to_owned()),
                SqlValue::Integer(reservation.end_at),
                SqlValue::Integer(reservation.now),
                SqlValue::Integer(reservation.now),
            ],
        ),
    ]
}

/// `PATCH /api/rest/reservations/:id`
pub async fn update(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(reservation_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let mut request = request;
    let input: UpdateInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the request is invalid")),
    };
    if !(MIN_REST_SECONDS..=MAX_REST_SECONDS).contains(&input.remaining_seconds) {
        return Ok(response(
            ErrorCode::BadRequest,
            "the remaining seconds are out of range",
        ));
    }
    let db = Db::new(env.d1("DB")?);
    let Some(row) = load_reservation(&db, &reservation_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    if row.active != 1 {
        return Ok(response(
            ErrorCode::Conflict,
            "the reservation is not active",
        ));
    }
    let now = now_seconds();
    let end_at = end_at(now, input.remaining_seconds);
    let version = next_version(row.version);
    let results = db
        .batch(&[
            SqlStatement::new(
                "UPDATE rest_push_reservations \
                 SET version = ?, end_at = ?, updated_at = ? \
                 WHERE id = ? AND user_id = ? AND active = 1 AND version = ?",
                vec![
                    SqlValue::Integer(version),
                    SqlValue::Integer(end_at),
                    SqlValue::Integer(now),
                    SqlValue::Text(reservation_id.clone()),
                    SqlValue::Text(authenticated.id.clone()),
                    SqlValue::Integer(row.version),
                ],
            ),
            select_reservation_statement(&reservation_id, &authenticated.id),
        ])
        .await?;
    let updated = results.first().map(changed_rows).transpose()?.unwrap_or(0);
    if updated == 0 {
        return Ok(response(
            ErrorCode::Conflict,
            "the reservation was changed by another request",
        ));
    }
    let body = results
        .get(1)
        .and_then(|result| result.results::<ReservationResponse>().ok())
        .and_then(|rows| rows.into_iter().next());
    let Some(body) = body else {
        return Ok(response(ErrorCode::Internal, "internal error"));
    };
    if let Err(error) = enqueue(&env, &body.id, body.version, now).await {
        console_error!("failed to enqueue the rest push reservation: {error}");
    }
    Response::from_json(&body)
}

/// `DELETE /api/rest/reservations/:id`
pub async fn delete(request: Request, context: RouteContext<()>) -> Result<Response> {
    let Some(reservation_id) = context.param("id").cloned() else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let db = Db::new(env.d1("DB")?);
    let Some(row) = load_reservation(&db, &reservation_id, &authenticated.id).await? else {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    };
    let now = now_seconds();
    let result = db
        .run(&cancel_statement(
            &reservation_id,
            &authenticated.id,
            row.version,
            now,
        ))
        .await?;
    if changed_rows(&result)? == 0 {
        return Ok(response(ErrorCode::NotFound, "resource not found"));
    }
    Ok(Response::empty()?.with_status(204))
}

/// `POST /api/push/received`
pub async fn received(request: Request, context: RouteContext<()>) -> Result<Response> {
    let env = context.env;
    let Some(authenticated) = session::resolve(&request, &env).await? else {
        return Ok(response(ErrorCode::Unauthorized, "authentication required"));
    };
    let mut request = request;
    let input: ReceivedInput = match request.json().await {
        Ok(input) => input,
        Err(_) => return Ok(response(ErrorCode::BadRequest, "the request is invalid")),
    };
    let db = Db::new(env.d1("DB")?);
    let now = now_seconds();
    db.run(&received_statement(
        &input.reservation_id,
        &authenticated.id,
        input.version,
        now,
    ))
    .await?;
    Ok(Response::empty()?.with_status(204))
}

/// トレーニングの終了で、そのトレーニングの有効な予約を取り消す。
pub async fn cancel_training_reservations(db: &Db, user_id: &str, training_id: &str) -> Result<()> {
    db.run(&deactivate_training_statement(
        user_id,
        training_id,
        "",
        now_seconds(),
        false,
    ))
    .await?;
    Ok(())
}

fn deactivate_training_statement(
    user_id: &str,
    training_id: &str,
    terminal_id: &str,
    now: i64,
    by_terminal: bool,
) -> SqlStatement {
    let (filter, mut bindings) = if by_terminal {
        (
            "AND terminal_token_id = ?",
            vec![SqlValue::Text(terminal_id.to_owned())],
        )
    } else {
        ("", Vec::new())
    };
    let mut values = vec![
        SqlValue::Integer(now),
        SqlValue::Text(user_id.to_owned()),
        SqlValue::Text(training_id.to_owned()),
    ];
    values.append(&mut bindings);
    SqlStatement::new(
        format!(
            "UPDATE rest_push_reservations SET active = 0, version = version + 1, updated_at = ? \
             WHERE user_id = ? AND training_id = ? AND active = 1 {filter}"
        ),
        values,
    )
}

async fn training_is_open(
    db: &Db,
    user_id: &str,
    training_id: &str,
    terminal_id: &str,
) -> Result<bool> {
    Ok(db
        .first::<TrainingRow>(&SqlStatement::new(
            "SELECT id FROM training_sessions \
             WHERE id = ? AND user_id = ? AND finished_at IS NULL AND owner_terminal_id = ?",
            vec![
                SqlValue::Text(training_id.to_owned()),
                SqlValue::Text(user_id.to_owned()),
                SqlValue::Text(terminal_id.to_owned()),
            ],
        ))
        .await?
        .is_some())
}

#[derive(Debug, Deserialize)]
struct TrainingRow {
    #[allow(dead_code)]
    id: String,
}

async fn load_reservation(
    db: &Db,
    reservation_id: &str,
    user_id: &str,
) -> Result<Option<ReservationRow>> {
    db.first(&SqlStatement::new(
        "SELECT id, user_id, terminal_token_id, version, end_at, active, sent_at \
         FROM rest_push_reservations WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(reservation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    ))
    .await
}

fn select_reservation_statement(reservation_id: &str, user_id: &str) -> SqlStatement {
    SqlStatement::new(
        "SELECT id, version, end_at FROM rest_push_reservations WHERE id = ? AND user_id = ?",
        vec![
            SqlValue::Text(reservation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
        ],
    )
}

fn cancel_statement(reservation_id: &str, user_id: &str, version: i64, now: i64) -> SqlStatement {
    SqlStatement::new(
        "UPDATE rest_push_reservations \
         SET active = 0, version = version + 1, updated_at = ? \
         WHERE id = ? AND user_id = ? AND version = ?",
        vec![
            SqlValue::Integer(now),
            SqlValue::Text(reservation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Integer(version),
        ],
    )
}

fn received_statement(reservation_id: &str, user_id: &str, version: i64, now: i64) -> SqlStatement {
    SqlStatement::new(
        "UPDATE rest_push_reservations SET received_at = ? \
         WHERE id = ? AND user_id = ? AND version = ? AND received_at IS NULL",
        vec![
            SqlValue::Integer(now),
            SqlValue::Text(reservation_id.to_owned()),
            SqlValue::Text(user_id.to_owned()),
            SqlValue::Integer(version),
        ],
    )
}

async fn enqueue(env: &Env, reservation_id: &str, version: i64, now: i64) -> Result<()> {
    let message = RestPushMessage {
        reservation_id: reservation_id.to_owned(),
        version,
    };
    let delay = delay_seconds(
        load_end_at(env, reservation_id, version)
            .await?
            .unwrap_or(now),
        now,
    );
    env.queue(QUEUE_BINDING)?
        .send(MessageBuilder::new(message).delay_seconds(delay).build())
        .await?;
    let db = Db::new(env.d1("DB")?);
    db.run(&SqlStatement::new(
        "UPDATE rest_push_reservations SET enqueued_at = ?, lease_expires_at = ? \
         WHERE id = ? AND version = ?",
        vec![
            SqlValue::Integer(now),
            SqlValue::Integer(now.saturating_add(QUEUE_LEASE_SECONDS)),
            SqlValue::Text(reservation_id.to_owned()),
            SqlValue::Integer(version),
        ],
    ))
    .await?;
    Ok(())
}

async fn load_end_at(env: &Env, reservation_id: &str, version: i64) -> Result<Option<i64>> {
    let db = Db::new(env.d1("DB")?);
    Ok(db
        .first::<EndRow>(&SqlStatement::new(
            "SELECT end_at FROM rest_push_reservations WHERE id = ? AND version = ?",
            vec![
                SqlValue::Text(reservation_id.to_owned()),
                SqlValue::Integer(version),
            ],
        ))
        .await?
        .map(|row| row.end_at))
}

#[derive(Debug, Deserialize)]
struct EndRow {
    end_at: i64,
}

/// Queue のメッセージを処理し、条件を満たす場合だけ通知を送る。
pub async fn consume(env: &Env, message: &RestPushMessage) -> Result<()> {
    let db = Db::new(env.d1("DB")?);
    let Some(row) = db
        .first::<ReservationRow>(&SqlStatement::new(
            "SELECT id, user_id, terminal_token_id, version, end_at, active, sent_at \
             FROM rest_push_reservations WHERE id = ?",
            vec![SqlValue::Text(message.reservation_id.clone())],
        ))
        .await?
    else {
        return Ok(());
    };
    let now = now_seconds();
    if !should_send(row.state(), message.version, now) {
        return Ok(());
    }
    let claimed = db
        .run(&SqlStatement::new(
            "UPDATE rest_push_reservations SET sent_at = ?, updated_at = ? \
             WHERE id = ? AND version = ? AND active = 1 AND sent_at IS NULL",
            vec![
                SqlValue::Integer(now),
                SqlValue::Integer(now),
                SqlValue::Text(row.id.clone()),
                SqlValue::Integer(row.version),
            ],
        ))
        .await?;
    if changed_rows(&claimed)? != 1 {
        return Ok(());
    }
    let Some(subscription) = db
        .first::<SubscriptionRow>(&SqlStatement::new(
            "SELECT endpoint, p256dh, auth FROM push_subscriptions \
             WHERE user_id = ? AND terminal_token_id = ? LIMIT 1",
            vec![
                SqlValue::Text(row.user_id.clone()),
                SqlValue::Text(row.terminal_token_id.clone()),
            ],
        ))
        .await?
    else {
        return Ok(());
    };
    let Some(vapid) = load_vapid(env)? else {
        console_error!("the VAPID keys are not configured");
        return Ok(());
    };
    let payload = serde_json::to_vec(&RestPushPayload {
        reservation_id: row.id.clone(),
        version: row.version,
        end_at: row.end_at,
    })?;
    let request = PushRequest {
        endpoint: subscription.endpoint,
        p256dh: subscription.p256dh,
        auth: subscription.auth,
        payload,
        ttl: ttl_seconds(row.state(), now),
    };
    let randomness = match push_randomness() {
        Ok(randomness) => randomness,
        Err(error) => {
            console_error!("failed to create the Web Push randomness: {error}");
            return Ok(());
        }
    };
    let prepared = match web_push::prepare(&request, &vapid, now, &randomness) {
        Ok(prepared) => prepared,
        Err(error) => {
            console_error!("failed to prepare the Web Push request: {error}");
            return Ok(());
        }
    };
    match send_prepared(&prepared).await {
        Ok(status) if (200..300).contains(&status) => {
            console_log!("the rest push notification was sent with the status {status}");
        }
        Ok(status) => console_error!("the push service returned the status {status}"),
        Err(error) => console_error!("failed to send the Web Push request: {error}"),
    }
    Ok(())
}

fn push_randomness() -> Result<PushRandomness> {
    let salt: [u8; SALT_LEN] = crate::random::random_bytes(SALT_LEN)?
        .try_into()
        .map_err(|_| worker::Error::RustError("the salt had an unexpected length".to_owned()))?;
    for _ in 0..4 {
        let secret = crate::random::random_bytes(32)?;
        if let Ok(randomness) = PushRandomness::new(salt, &secret) {
            return Ok(randomness);
        }
    }
    Err(worker::Error::RustError(
        "failed to generate a valid ephemeral key".to_owned(),
    ))
}

fn load_vapid(env: &Env) -> Result<Option<VapidKeys>> {
    let Ok(private_key) = auth::var(env, "VAPID_PRIVATE_KEY") else {
        return Ok(None);
    };
    let Ok(public_key) = auth::var(env, "VAPID_PUBLIC_KEY") else {
        return Ok(None);
    };
    let Ok(subject) = auth::var(env, "VAPID_SUBJECT") else {
        return Ok(None);
    };
    let private_key = match warrior_core::base64url::decode(private_key.trim()) {
        Some(key) => key,
        None => return Ok(None),
    };
    let public_key = match warrior_core::base64url::decode(public_key.trim()) {
        Some(key) => key,
        None => return Ok(None),
    };
    Ok(VapidKeys::new(&private_key, &public_key, subject.trim()).ok())
}

async fn send_prepared(prepared: &web_push::PreparedPush) -> Result<u16> {
    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(
        js_sys::Uint8Array::from(prepared.body.as_slice()).into(),
    ));
    let headers = Headers::new();
    for (name, value) in &prepared.headers {
        headers.set(name, value)?;
    }
    init.with_headers(headers);
    let request = Request::new_with_init(&prepared.url, &init)?;
    let response = Fetch::Request(request).send().await?;
    Ok(response.status_code())
}

/// 期限の近い有効な予約を Queue へ送り直す。
///
/// 作成や変更のときの送信に失敗した場合と、Queue のメッセージが失われた場合に備える。
/// 同じ版のメッセージが複数届いても、消費側の `sent_at` の更新で 1 回だけ送る。
pub async fn enqueue_due(env: &Env, now: i64) -> Result<()> {
    let db = Db::new(env.d1("DB")?);
    let rows = db
        .all(&SqlStatement::new(
            "SELECT id, version FROM rest_push_reservations \
             WHERE active = 1 AND sent_at IS NULL AND end_at <= ? \
             AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
             ORDER BY end_at, id LIMIT 100",
            vec![
                SqlValue::Integer(now.saturating_add(30)),
                SqlValue::Integer(now),
            ],
        ))
        .await?
        .results::<DueRow>()?;
    for row in rows {
        if let Err(error) = enqueue(env, &row.id, row.version, now).await {
            console_error!("failed to re-enqueue the rest push reservation: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! 予約の SQL の例示テスト。
    //!
    //! 対応する要求: `REQ-TMR-010`。

    use warrior_core::sql::SqlValue;

    use super::{cancel_statement, create_statements, CreateReservation};

    #[test]
    fn req_tmr_010_creating_a_reservation_deactivates_the_previous_one() {
        let statements = create_statements(&CreateReservation {
            id: "reservation-1",
            user_id: "user-1",
            training_id: "training-1",
            terminal_id: "terminal-1",
            end_at: 1_700_000_100,
            now: 1_700_000_000,
        });
        assert_eq!(statements.len(), 2);
        assert!(statements[0]
            .sql()
            .contains("SET active = 0, version = version + 1"));
        assert!(statements[0].sql().contains("terminal_token_id = ?"));
        assert!(statements[1].sql().contains("VALUES (?, ?, ?, ?, 1, ?, 1"));
        assert_eq!(
            statements[1].bindings()[4],
            SqlValue::Integer(1_700_000_100)
        );
    }

    #[test]
    fn req_tmr_010_cancelling_advances_the_version_without_creating_a_reservation() {
        let statement = cancel_statement("reservation-1", "user-1", 3, 1_700_000_010);
        assert!(statement
            .sql()
            .contains("SET active = 0, version = version + 1"));
        assert!(!statement.sql().contains("INSERT"));
        assert_eq!(statement.bindings()[3], SqlValue::Integer(3));
    }
}
