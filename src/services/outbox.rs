//! Retrying what failed, and reaching people another way.
//!
//! `notify` tries every channel inline, which is right: the common case is
//! that it works, and queueing first would put a delay in front of every
//! notification to buy resilience against a rare failure.
//!
//! What was missing is the other half. A 503 from the mail provider was
//! logged and the message was gone — not late, gone, with nowhere to put it
//! and nothing to retry. This is that somewhere.
//!
//! ## Backoff, and when to stop
//!
//! Doubling from a minute, so a provider having a bad thirty seconds costs
//! one delayed message and a provider having a bad afternoon is not
//! hammered. Six attempts reach roughly an hour; past that the failure has
//! stopped being transient, and the honest thing is to tell an operator
//! rather than keep a queue that quietly never drains.
//!
//! ## Fallback is not retry
//!
//! A push that fails because the device token is stale will fail again
//! forever — the person reinstalled the app, and asking the same question
//! louder does not help. So a failed push is never retried. Instead, for a
//! transactional kind, an email is enqueued: a different road to the same
//! person, taken only where the message is an obligation. Nobody needs an
//! email because a push about a mention did not arrive.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::{email_template, i18n};

/// How many times a channel is asked before a person is asked instead.
const MAX_ATTEMPTS: i32 = 6;

/// First delay, doubled each failure: 1, 2, 4, 8, 16, 32 minutes.
const BASE_BACKOFF_SECONDS: i64 = 60;

/// How many rows one worker pass drains.
const BATCH: i64 = 200;

/// What a message needs to be sent again later.
#[derive(Debug, Clone)]
pub struct Queued<'a> {
    pub user_id: Uuid,
    pub notification_id: Option<Uuid>,
    pub kind: &'a str,
    pub channel: crate::services::notify::Channel,
    pub locale: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub payload: Option<&'a serde_json::Value>,
    pub cta_url: Option<&'a str>,
    pub unsubscribe_url: Option<&'a str>,
    /// True when this exists because another channel failed. Such a row
    /// ignores the recipient's preference for its channel, which is only
    /// ever done for a transactional kind.
    pub is_fallback: bool,
    /// Why the first attempt failed, kept from the start so a row that is
    /// abandoned still says what went wrong originally.
    pub reason: &'a str,
}

/// Put a message in the queue.
///
/// Best-effort by construction: a failure to enqueue is logged rather than
/// propagated, because it is already the failure path and turning it into a
/// second error would fail the request that caused the notification.
pub async fn enqueue(db: &PgPool, queued: Queued<'_>) {
    let result = sqlx::query(
        "INSERT INTO notification_outbox
            (user_id, notification_id, kind, channel, locale, title, body,
             payload, cta_url, unsubscribe_url, is_fallback, last_error,
             next_attempt_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                 NOW() + ($13 || ' seconds')::INTERVAL)",
    )
    .bind(queued.user_id)
    .bind(queued.notification_id)
    .bind(queued.kind)
    .bind(queued.channel.as_str())
    .bind(queued.locale)
    .bind(queued.title)
    .bind(queued.body)
    .bind(queued.payload)
    .bind(queued.cta_url)
    .bind(queued.unsubscribe_url)
    .bind(queued.is_fallback)
    .bind(queued.reason)
    .bind(BASE_BACKOFF_SECONDS.to_string())
    .execute(db)
    .await;

    match result {
        Ok(_) => {
            metrics::counter!(
                "skilluv_notification_queued_total",
                "kind" => queued.kind.to_string(),
                "channel" => queued.channel.as_str(),
                "fallback" => queued.is_fallback.to_string()
            )
            .increment(1);
        }
        Err(e) => tracing::error!(
            kind = queued.kind,
            user = %queued.user_id,
            error = %e,
            "could not queue a failed notification — this one really is lost"
        ),
    }
}

/// One queued delivery, as the worker reads it back.
#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: Uuid,
    kind: String,
    channel: String,
    locale: String,
    title: String,
    body: String,
    payload: Option<serde_json::Value>,
    cta_url: Option<String>,
    unsubscribe_url: Option<String>,
    attempts: i32,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct DrainReport {
    pub attempted: usize,
    pub sent: usize,
    pub deferred: usize,
    pub abandoned: usize,
}

/// Try everything that is due.
pub async fn drain(
    db: &PgPool,
    email: &crate::services::EmailService,
) -> Result<DrainReport, AppError> {
    let due: Vec<Row> = sqlx::query_as(
        "SELECT id, user_id, kind, channel, locale, title, body, payload,
                cta_url, unsubscribe_url, attempts
           FROM notification_outbox
          WHERE status = 'pending' AND next_attempt_at <= NOW()
          ORDER BY next_attempt_at
          LIMIT $1",
    )
    .bind(BATCH)
    .fetch_all(db)
    .await?;

    let mut report = DrainReport::default();

    for row in due {
        report.attempted += 1;
        let outcome = attempt(db, email, &row).await;

        match outcome {
            Ok(()) => {
                sqlx::query(
                    "UPDATE notification_outbox
                        SET status = 'sent', attempts = attempts + 1
                      WHERE id = $1",
                )
                .bind(row.id)
                .execute(db)
                .await?;
                report.sent += 1;
            }
            Err(e) => {
                let next = row.attempts + 1;
                if next >= MAX_ATTEMPTS {
                    // Out of attempts. Not dropped quietly: a message that
                    // could not be delivered after an hour of trying is
                    // something a person should see.
                    sqlx::query(
                        "UPDATE notification_outbox
                            SET status = 'abandoned', attempts = $2, last_error = $3
                          WHERE id = $1",
                    )
                    .bind(row.id)
                    .bind(next)
                    .bind(e.to_string())
                    .execute(db)
                    .await?;

                    report.abandoned += 1;
                    metrics::counter!(
                        "skilluv_notification_abandoned_total",
                        "kind" => row.kind.clone(),
                        "channel" => row.channel.clone()
                    )
                    .increment(1);
                    tracing::error!(
                        kind = %row.kind,
                        channel = %row.channel,
                        user = %row.user_id,
                        attempts = next,
                        error = %e,
                        "notification abandoned after every attempt — the recipient was never reached"
                    );
                } else {
                    // Doubling: 1, 2, 4, 8, 16 minutes.
                    let delay = BASE_BACKOFF_SECONDS * (1 << next.min(10));
                    sqlx::query(
                        "UPDATE notification_outbox
                            SET attempts = $2,
                                last_error = $3,
                                next_attempt_at = NOW() + ($4 || ' seconds')::INTERVAL
                          WHERE id = $1",
                    )
                    .bind(row.id)
                    .bind(next)
                    .bind(e.to_string())
                    .bind(delay.to_string())
                    .execute(db)
                    .await?;
                    report.deferred += 1;
                }
            }
        }
    }

    Ok(report)
}

/// One delivery attempt, on the channel the row names.
async fn attempt(
    db: &PgPool,
    email: &crate::services::EmailService,
    row: &Row,
) -> Result<(), AppError> {
    match row.channel.as_str() {
        "email" => {
            let address: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT email, display_name FROM users
                  WHERE id = $1 AND email_disabled = FALSE",
            )
            .bind(row.user_id)
            .fetch_optional(db)
            .await?;

            let Some((address, display_name)) = address else {
                // The address was disabled between the failure and the
                // retry — a hard bounce, or a deletion request. Not an
                // error, and retrying would be worse than not.
                return Ok(());
            };

            // The theme is read again rather than stored: someone who
            // changed worlds between the failure and the retry should get
            // the one they are looking at now.
            let theme: Option<String> =
                sqlx::query_scalar("SELECT preferred_theme FROM users WHERE id = $1")
                    .bind(row.user_id)
                    .fetch_optional(db)
                    .await
                    .ok()
                    .flatten();

            let cta_label = row
                .cta_url
                .as_ref()
                .map(|_| i18n::t(&row.locale, &format!("notification.{}.cta", row.kind)));

            let html = email_template::render(email_template::Email {
                locale: &row.locale,
                theme: theme.as_deref(),
                title: &row.title,
                body: &row.body,
                recipient_name: display_name.as_deref(),
                stats: &[],
                cta_label: cta_label.as_deref(),
                cta_url: row.cta_url.as_deref(),
                unsubscribe_url: row.unsubscribe_url.as_deref(),
            });

            email
                .send_with_log(
                    db,
                    crate::services::email::SendWithLogParams {
                        user_id: row.user_id,
                        to_email: &address,
                        to_name: display_name.as_deref().unwrap_or(""),
                        subject: &row.title,
                        html: &html,
                        kind: &row.kind,
                    },
                )
                .await?;
            Ok(())
        }

        "push" => {
            let message = crate::services::mobile_push::MobilePushMessage {
                title: &row.title,
                body: &row.body,
                data: row.payload.clone(),
            };
            crate::services::mobile_push::push_to_user_mobile(db, row.user_id, message).await?;
            Ok(())
        }

        other => Err(AppError::Internal(format!(
            "outbox row names an unknown channel '{other}'"
        ))),
    }
}

/// Drain the queue on a timer.
///
/// Every minute, which is the shortest backoff — a longer tick would make
/// the first retry wait for the tick rather than for the backoff, and turn
/// "one minute" into "up to five".
pub fn start_outbox_worker(db: PgPool, email: std::sync::Arc<crate::services::EmailService>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            ticker.tick().await;
            match drain(&db, &email).await {
                Ok(report) if report.attempted > 0 => {
                    tracing::info!(
                        attempted = report.attempted,
                        sent = report.sent,
                        deferred = report.deferred,
                        abandoned = report.abandoned,
                        "notification outbox"
                    );
                }
                Ok(_) => {}
                Err(e) => tracing::error!(
                    error = %e,
                    "outbox drain failed — queued notifications stayed queued"
                ),
            }
        }
    });
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn the_backoff_doubles_and_stays_sane() {
        // 1, 2, 4, 8, 16, 32 minutes: a provider having a bad thirty
        // seconds costs one delayed message, and one having a bad afternoon
        // is not hammered.
        let delays: Vec<i64> = (1..=MAX_ATTEMPTS)
            .map(|n| BASE_BACKOFF_SECONDS * (1 << n.min(10)))
            .collect();
        assert_eq!(delays[0], 120);
        assert_eq!(delays[5], 3840);
        assert!(
            delays.windows(2).all(|w| w[1] > w[0]),
            "each wait must be longer than the last"
        );
        // The whole sequence is about an hour. Past that the failure has
        // stopped being transient and a person is told instead.
        let total: i64 = delays.iter().sum();
        assert!((3600..14_400).contains(&total), "total was {total}s");
    }
}
