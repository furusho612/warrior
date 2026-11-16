//! `JOBS_QUEUE` のメッセージの種別ごとの振り分け。
//!
//! AI の生成と、レスト終了のプッシュ通知の予約が同じ Queue を使う。
//! メッセージには本文を入れず、生成の ID または予約の ID と版だけを入れる (ADR-021)。

use serde::{Deserialize, Serialize};
use warrior_core::rest_push::RestPushMessage;
use worker::{console_error, Env, MessageBatch, MessageExt, Result};

use crate::ai_job;

/// `JOBS_QUEUE` のメッセージ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JobMessage {
    /// AI の生成。
    Ai(ai_job::JobMessage),
    /// レスト終了のプッシュ通知の予約。
    Rest(RestPushMessage),
}

/// Queue のメッセージを処理する。
pub async fn consume(batch: MessageBatch<JobMessage>, env: Env) -> Result<()> {
    for message in batch.iter() {
        let message = match message {
            Ok(message) => message,
            Err(error) => {
                console_error!("failed to read the job message: {error}");
                continue;
            }
        };
        let correlation_id = message.id();
        let result = match message.body() {
            JobMessage::Ai(ai) => {
                ai_job::consume_one(&env, &ai.generation_id, &correlation_id).await
            }
            JobMessage::Rest(rest) => crate::rest_push::consume(&env, rest).await,
        };
        if let Err(error) = result {
            console_error!("failed to process the job message: {error}");
        }
        message.ack();
    }
    Ok(())
}
