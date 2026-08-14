//! A message whose channel failed is not gone.
//!
//! Two behaviours, and they are deliberately not the same one. An email
//! that fails is **retried**, because the cause is usually transient. A
//! push that fails is **not** retried — a stale device token does not heal,
//! and asking again gets the same answer forever — but a transactional one
//! takes another road instead.

mod common;

use common::TestApp;
use skilluv_backend::services::EmailService;
use skilluv_backend::services::notify::Channel;
use skilluv_backend::services::outbox::{self, Queued};
use uuid::Uuid;

/// The same service the app builds: no Brevo key, so it logs rather than
/// sends. What is under test is the queue, not the provider.
fn mailer() -> EmailService {
    EmailService::new(None, "test@skill-uv.com", "Skilluv Test")
}

async fn person(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name,
                            first_name, last_name, email_verified)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L', TRUE)
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

async fn queue_one(app: &TestApp, user: Uuid, channel: Channel, is_fallback: bool) {
    outbox::enqueue(
        &app.db,
        Queued {
            user_id: user,
            notification_id: None,
            kind: "payout.failed",
            channel,
            locale: "fr",
            title: "Ton virement a échoué",
            body: "Le numéro n'est pas enregistré.",
            payload: None,
            cta_url: Some("https://skill-uv.com/wallet"),
            unsubscribe_url: None,
            is_fallback,
            reason: "provider returned 503",
        },
    )
    .await;
}

#[tokio::test]
async fn a_failed_channel_lands_in_the_queue_rather_than_nowhere() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_queued").await;
    queue_one(&app, user, Channel::Email, false).await;

    let (status, attempts, error): (String, i32, Option<String>) = sqlx::query_as(
        "SELECT status, attempts, last_error FROM notification_outbox WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(status, "pending");
    assert_eq!(attempts, 0);
    // "Email failed" is not something anyone can act on. The provider's own
    // words are.
    assert_eq!(error.as_deref(), Some("provider returned 503"));
}

#[tokio::test]
async fn a_queued_message_waits_for_its_backoff() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_backoff").await;
    queue_one(&app, user, Channel::Email, false).await;

    // The first attempt is a minute out, so an immediate drain must not
    // pick it up — retrying instantly is how a failing provider gets
    // hammered by its own error.
    let report = outbox::drain(&app.db, &mailer()).await.unwrap();
    assert_eq!(report.attempted, 0);

    sqlx::query("UPDATE notification_outbox SET next_attempt_at = NOW() - INTERVAL '1 minute'")
        .execute(&app.db)
        .await
        .unwrap();

    let report = outbox::drain(&app.db, &mailer()).await.unwrap();
    assert_eq!(report.attempted, 1, "due now, so it is attempted");
}

#[tokio::test]
async fn the_queue_cannot_hold_a_row_the_worker_cannot_interpret() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_spent").await;
    queue_one(&app, user, Channel::Email, false).await;

    // A row naming a channel nothing implements would be attempted, fail
    // for a reason no operator can act on, and eventually be abandoned —
    // six times over, for a typo. It cannot exist.
    let bad = sqlx::query("UPDATE notification_outbox SET channel = 'nonsense' WHERE user_id = $1")
        .bind(user)
        .execute(&app.db)
        .await;
    assert!(bad.is_err());
}

#[tokio::test]
async fn a_row_that_is_abandoned_must_say_why() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_reason").await;
    queue_one(&app, user, Channel::Email, false).await;

    // A message that could not be delivered after every attempt is a
    // support question. Abandoning it without a reason leaves nobody able
    // to answer it.
    let bad = sqlx::query(
        "UPDATE notification_outbox SET status = 'abandoned', last_error = NULL
          WHERE user_id = $1",
    )
    .bind(user)
    .execute(&app.db)
    .await;
    assert!(bad.is_err(), "an abandoned row must carry its reason");
}

#[tokio::test]
async fn a_disabled_address_is_not_retried_forever() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_bounced").await;
    queue_one(&app, user, Channel::Email, false).await;

    // The address hard-bounced between the failure and the retry. Sending
    // again would keep mailing an address the provider already rejected,
    // which is how a sending domain loses its reputation.
    sqlx::query("UPDATE users SET email_disabled = TRUE WHERE id = $1")
        .bind(user)
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_outbox SET next_attempt_at = NOW() - INTERVAL '1 minute'")
        .execute(&app.db)
        .await
        .unwrap();

    let report = outbox::drain(&app.db, &mailer()).await.unwrap();
    assert_eq!(report.attempted, 1);
    assert_eq!(report.sent, 1, "resolved, not retried into the void");

    let status: String =
        sqlx::query_scalar("SELECT status FROM notification_outbox WHERE user_id = $1")
            .bind(user)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "sent");
}

#[tokio::test]
async fn a_fallback_is_recorded_as_one() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_fallback").await;
    queue_one(&app, user, Channel::Email, true).await;

    // A fallback ignores the recipient's preference for its channel, which
    // is only ever done for a transactional kind. Recording it makes that
    // auditable instead of implicit — someone will ask why they received an
    // email they had turned off, and the answer has to exist.
    let is_fallback: bool =
        sqlx::query_scalar("SELECT is_fallback FROM notification_outbox WHERE user_id = $1")
            .bind(user)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(is_fallback);
}

#[tokio::test]
async fn the_queue_only_ever_holds_channels_that_can_be_sent() {
    let app = TestApp::spawn().await;
    let user = person(&app, "obx_channel").await;

    // In-app cannot be queued: it either wrote its row or it did not, and
    // there is nothing to retry against a database that just refused.
    let bad = sqlx::query(
        "INSERT INTO notification_outbox (user_id, kind, channel, locale, title, body)
         VALUES ($1, 'payout.failed', 'in_app', 'fr', 't', 'b')",
    )
    .bind(user)
    .execute(&app.db)
    .await;
    assert!(bad.is_err(), "only push and email are queueable");
}
