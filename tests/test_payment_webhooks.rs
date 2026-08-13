//! A payout that fails at the provider must give the money back.
//!
//! Before this path existed, a Mobile Money payout was recorded as sent the
//! moment the operator said "accepted" and nothing ever listened for the
//! answer. When the answer was "the number is not registered", the ledger
//! kept the balance debited and the money existed nowhere. These tests hold
//! the whole chain to that: verify, store, apply, and never twice.

mod common;

use bigdecimal::BigDecimal;
use common::TestApp;
use serde_json::json;
use skilluv_backend::services::ledger::{self, Currency, State};
use skilluv_backend::services::payment_webhook_sources::MobileMoneySource;
use skilluv_backend::services::payment_webhooks::{self, Outcome};
use std::str::FromStr;
use uuid::Uuid;

const SECRET: &str = "test-callback-secret";

async fn person(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "INSERT INTO users (username, email, password_hash, display_name, first_name, last_name)
         VALUES ('{username}', '{username}@test.dev', 'x', '{username}', 'F', 'L')
         RETURNING id"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap()
}

/// A payout already recorded as sent, waiting for the operator to confirm.
async fn pending_payout(app: &TestApp, user: Uuid, amount: &str, reference: &str) -> Uuid {
    let amount = BigDecimal::from_str(amount).unwrap();

    // Money the person actually earned, so the reversal has somewhere to go.
    let subject = Uuid::new_v4();
    ledger::capture_for_recipient(
        &app.db,
        "mtn",
        format!("capture:{reference}"),
        user,
        amount.clone(),
        BigDecimal::from(0),
        Currency::Xof,
        "mentorship_session",
        subject,
    )
    .await
    .expect("capture");
    ledger::release(
        &app.db,
        user,
        amount.clone(),
        Currency::Xof,
        "mentorship_session",
        subject,
    )
    .await
    .expect("release");

    let key = format!("withdraw:{reference}");
    let posted = ledger::withdraw(
        &app.db,
        user,
        amount.clone(),
        Currency::Xof,
        "mtn",
        reference.to_string(),
        key.clone(),
    )
    .await
    .expect("withdraw");

    sqlx::query(
        "INSERT INTO payouts
            (user_id, ledger_transaction_id, provider, provider_reference, rail,
             amount, currency, idempotency_key)
         VALUES ($1, $2, 'mtn', $3, 'mobile_money', $4, 'XOF', $5)
         RETURNING id",
    )
    .bind(user)
    .bind(posted.transaction_id())
    .bind(reference)
    .bind(&amount)
    .bind(&key)
    .execute(&app.db)
    .await
    .expect("record the payout");

    user
}

fn sign(body: &str) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn source() -> MobileMoneySource {
    MobileMoneySource {
        operator: "mtn",
        webhook_secret: SECRET.into(),
    }
}

async fn available(app: &TestApp, user: Uuid) -> BigDecimal {
    ledger::user_balance(&app.db, user, State::Available, Currency::Xof)
        .await
        .expect("read balance")
}

#[tokio::test]
async fn a_failed_payout_returns_the_money() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wh_failed").await;
    pending_payout(&app, user, "5000", "txn-failed-1").await;

    // Withdrawn: the balance is down by the full amount.
    assert_eq!(available(&app, user).await, BigDecimal::from(0));

    let body = json!({
        "status": "failed",
        "transaction_id": "txn-failed-1",
        "message": "number not registered on MTN Benin"
    })
    .to_string();

    let outcome = payment_webhooks::receive(&app.db, &source(), &body, Some(&sign(&body)))
        .await
        .expect("the callback is accepted");
    assert_eq!(outcome, Outcome::Applied("payout.failed".into()));

    // The money is back where it was, which is the entire point.
    assert_eq!(available(&app, user).await, BigDecimal::from(5000));

    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, failure_reason FROM payouts WHERE provider_reference = $1")
            .bind("txn-failed-1")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(
        reason.as_deref(),
        Some("number not registered on MTN Benin"),
        "the operator's own words are what a person can act on"
    );
}

#[tokio::test]
async fn the_same_failure_delivered_twice_refunds_once() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wh_twice").await;
    pending_payout(&app, user, "3000", "txn-twice-1").await;

    let body = json!({ "status": "failed", "transaction_id": "txn-twice-1" }).to_string();
    let signature = sign(&body);

    let first = payment_webhooks::receive(&app.db, &source(), &body, Some(&signature))
        .await
        .unwrap();
    assert_eq!(first, Outcome::Applied("payout.failed".into()));

    // Every provider redelivers. Refunding twice would be free money.
    let second = payment_webhooks::receive(&app.db, &source(), &body, Some(&signature))
        .await
        .unwrap();
    assert_eq!(second, Outcome::Duplicate);

    assert_eq!(available(&app, user).await, BigDecimal::from(3000));
}

#[tokio::test]
async fn a_settlement_closes_the_payout_without_touching_the_books() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wh_settled").await;
    pending_payout(&app, user, "2000", "txn-sent-1").await;

    let body = json!({ "status": "SUCCESS", "transaction_id": "txn-sent-1" }).to_string();
    let outcome = payment_webhooks::receive(&app.db, &source(), &body, Some(&sign(&body)))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Applied("payout.settled".into()));

    let (status, settled_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, settled_at FROM payouts WHERE provider_reference = $1")
            .bind("txn-sent-1")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(status, "sent");
    assert!(settled_at.is_some(), "a settled payout has a settled_at");

    // The withdrawal was already recorded when it was sent. Recording it
    // again on confirmation would take the money twice.
    assert_eq!(available(&app, user).await, BigDecimal::from(0));
}

#[tokio::test]
async fn a_forged_callback_is_stored_and_refused() {
    let app = TestApp::spawn().await;
    let user = person(&app, "wh_forged").await;
    pending_payout(&app, user, "9000", "txn-forged-1").await;

    let body = json!({ "status": "failed", "transaction_id": "txn-forged-1" }).to_string();

    let refused = payment_webhooks::receive(&app.db, &source(), &body, Some("deadbeef")).await;
    assert!(refused.is_err(), "an unsigned callback must not be applied");

    // Nothing moved.
    assert_eq!(available(&app, user).await, BigDecimal::from(0));

    // But it was written down: silently dropping an attempt hides it.
    let stored: (bool, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT signature_verified, processed_at FROM payment_webhook_events
          WHERE provider = 'mtn' ORDER BY received_at DESC LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .expect("the attempt was recorded");
    assert!(!stored.0, "it is recorded as unverified");
    assert!(stored.1.is_none(), "and it was never applied");
}

#[tokio::test]
async fn an_event_about_an_unknown_payout_changes_nothing() {
    let app = TestApp::spawn().await;

    // A callback naming a reference we never recorded — another
    // environment sharing the credential, or money moved outside our books.
    let body = json!({ "status": "failed", "transaction_id": "txn-nobody-knows" }).to_string();
    let outcome = payment_webhooks::receive(&app.db, &source(), &body, Some(&sign(&body)))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Ignored);
}

#[tokio::test]
async fn an_event_we_cannot_read_is_kept_rather_than_dropped() {
    let app = TestApp::spawn().await;

    // No status field: nothing to act on, and exactly the body we will want
    // to read when a provider changes its format without telling anyone.
    let body = json!({ "something": "else" }).to_string();
    let outcome = payment_webhooks::receive(&app.db, &source(), &body, Some(&sign(&body)))
        .await
        .unwrap();
    assert_eq!(outcome, Outcome::Ignored);

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payment_webhook_events
          WHERE provider = 'mtn' AND kind IS NULL AND signature_verified = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(stored, 1, "the raw body is kept even when unread");
}

#[tokio::test]
async fn a_stored_event_cannot_be_edited_or_deleted() {
    let app = TestApp::spawn().await;
    let body = json!({ "status": "SUCCESS", "transaction_id": "txn-evidence" }).to_string();
    payment_webhooks::receive(&app.db, &source(), &body, Some(&sign(&body)))
        .await
        .unwrap();

    // The log is what settles an argument with a provider. Rewriting it is
    // falsifying evidence, so the database refuses.
    let edited = sqlx::query(
        "UPDATE payment_webhook_events SET payload = '{}'::jsonb WHERE provider = 'mtn'",
    )
    .execute(&app.db)
    .await;
    assert!(edited.is_err(), "the payload must be immutable");

    let deleted = sqlx::query("DELETE FROM payment_webhook_events WHERE provider = 'mtn'")
        .execute(&app.db)
        .await;
    assert!(deleted.is_err(), "events are never deleted");
}
