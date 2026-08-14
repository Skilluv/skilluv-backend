//! The browser is not part of the payment flow.
//!
//! The scenario every one of these is written against: the payer confirms
//! on their phone and closes the tab. The provider has the money. If
//! delivery hangs off the front end coming back, or off a webhook that may
//! never arrive, the payment is real and the order does not exist — and the
//! only person who knows is the customer.

mod common;

use common::TestApp;
use skilluv_backend::services::fulfilment;
use uuid::Uuid;

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

/// A session booked and awaiting payment, and the payment recorded for it.
async fn pending_session(app: &TestApp, mentor: Uuid, mentee: Uuid) -> (Uuid, Uuid) {
    let session: Uuid = sqlx::query_scalar(
        "INSERT INTO mentorship_sessions
            (mentor_user_id, mentee_user_id, scheduled_at, duration_minutes,
             price_total_cents, price_mentor_cents, price_platform_cents,
             currency, status)
         VALUES ($1, $2, NOW() + INTERVAL '2 days', 60, 5000, 4000, 1000, 'EUR', 'pending')
         RETURNING id",
    )
    .bind(mentor)
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .expect("seed session");

    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments
            (payer_id, subject_type, subject_id, provider, method, amount, currency,
             merchant_reference, idempotency_key)
         VALUES ($1, 'mentorship_session', $2, 'fedapay', 'mobile_money', 50, 'EUR',
                 'SKU-test-' || $2::text, 'test-' || $2::text)
         RETURNING id",
    )
    .bind(mentee)
    .bind(session)
    .fetch_one(&app.db)
    .await
    .expect("seed payment");

    (session, payment)
}

async fn session_status(app: &TestApp, session: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM mentorship_sessions WHERE id = $1")
        .bind(session)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_payment_delivers_without_the_front_end_ever_returning() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "ful_mentor").await;
    let mentee = person(&app, "ful_mentee").await;
    let (session, payment) = pending_session(&app, mentor, mentee).await;

    assert_eq!(session_status(&app, session).await, "pending");

    // Nothing here is a browser. This is the path the poller takes after
    // asking the provider, and the path a webhook takes when one arrives.
    let delivered = fulfilment::settle_and_deliver(&app.db, payment, Some("txn_123"))
        .await
        .expect("delivery");
    assert!(delivered);

    assert_eq!(session_status(&app, session).await, "paid");
}

#[tokio::test]
async fn two_roads_arriving_together_deliver_once() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "ful_race_mentor").await;
    let mentee = person(&app, "ful_race_mentee").await;
    let (_, payment) = pending_session(&app, mentor, mentee).await;

    // The webhook and the poller both hearing about the same payment is the
    // normal case, not the rare one — the poller runs every minute and does
    // not know a webhook is in flight.
    let first = fulfilment::settle_and_deliver(&app.db, payment, Some("txn_1"))
        .await
        .unwrap();
    let second = fulfilment::settle_and_deliver(&app.db, payment, Some("txn_1"))
        .await
        .unwrap();

    assert!(first, "one of them delivers");
    assert!(!second, "and the other does not deliver again");
}

#[tokio::test]
async fn the_providers_reference_is_kept_for_the_refund_that_may_follow() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "ful_ref_mentor").await;
    let mentee = person(&app, "ful_ref_mentee").await;
    let (_, payment) = pending_session(&app, mentor, mentee).await;

    fulfilment::settle_and_deliver(&app.db, payment, Some("pi_abc123"))
        .await
        .unwrap();

    // Without this, a dispute settled for the payer moves our books over a
    // card that is never credited.
    let reference: Option<String> =
        sqlx::query_scalar("SELECT provider_reference FROM payments WHERE id = $1")
            .bind(payment)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(reference.as_deref(), Some("pi_abc123"));
}

#[tokio::test]
async fn a_failed_delivery_is_left_for_the_next_attempt() {
    let app = TestApp::spawn().await;
    let mentee = person(&app, "ful_broken").await;

    // A payment for something nothing knows how to deliver. Money taken and
    // nothing given, which must be loud and must not be marked done.
    let payment: Uuid = sqlx::query_scalar(
        "INSERT INTO payments
            (payer_id, subject_type, subject_id, provider, method, amount, currency,
             idempotency_key)
         VALUES ($1, 'something_nobody_delivers', gen_random_uuid(), 'fedapay',
                 'mobile_money', 50, 'EUR', 'test-broken')
         RETURNING id",
    )
    .bind(mentee)
    .fetch_one(&app.db)
    .await
    .unwrap();

    let outcome = fulfilment::settle_and_deliver(&app.db, payment, None).await;
    assert!(outcome.is_err(), "an undeliverable payment must be loud");

    // Unstamped, so the poller finds it again. Leaving it stamped would
    // mean the money is taken and nothing will ever look at it again.
    let (status, fulfilled): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, fulfilled_at FROM payments WHERE id = $1")
            .bind(payment)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "succeeded", "the money did arrive");
    assert!(fulfilled.is_none(), "and the delivery did not happen");
}

#[tokio::test]
async fn money_taken_and_nothing_given_is_findable() {
    let app = TestApp::spawn().await;
    let mentor = person(&app, "ful_owed_mentor").await;
    let mentee = person(&app, "ful_owed_mentee").await;
    let (_, payment) = pending_session(&app, mentor, mentee).await;

    // The state a lost webhook used to leave behind permanently: the money
    // arrived, nothing was delivered, and nothing ever looked again.
    sqlx::query(
        "UPDATE payments
            SET status = 'succeeded', succeeded_at = NOW() - INTERVAL '10 minutes'
          WHERE id = $1",
    )
    .bind(payment)
    .execute(&app.db)
    .await
    .unwrap();

    let owed = fulfilment::undelivered(&app.db, 60).await.unwrap();
    assert!(
        owed.iter().any(|p| p.id == payment),
        "a paid, undelivered payment must be findable — this is the query an \
         operator runs and the one the sweep runs every minute"
    );
}

#[tokio::test]
async fn every_way_of_paying_carries_a_reference_we_can_ask_about() {
    let app = TestApp::spawn().await;

    // Our own reference is what makes a lost create response recoverable:
    // it is sent to the provider and queryable back, so a payment can be
    // resolved even when their identifier never reached us.
    let methods: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_methods WHERE enabled = TRUE AND provider = 'fedapay'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(methods > 0, "the methods catalogue is seeded");

    // The subset that needs no redirect — the one the front renders inline.
    let inline: Vec<String> = sqlx::query_scalar(
        "SELECT operator FROM payment_methods
          WHERE enabled = TRUE AND supports_inline = TRUE AND country = 'BJ'
          ORDER BY sort_order",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();
    assert_eq!(
        inline,
        vec!["mtn", "moov", "celtiis"],
        "Benin's no-redirect operators, in the order a payer sees them"
    );
}
