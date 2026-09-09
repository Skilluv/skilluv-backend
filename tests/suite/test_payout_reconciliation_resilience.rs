//! One payout the sweep cannot finish does not strand the ones behind it.
//!
//! `reconciliation::sweep` reads a page of unconfirmed payouts and works
//! through it. Provider errors were caught per row from the start, but every
//! database call in the loop used `?`, so one failure returned from the whole
//! sweep and every payout after it in the page was left unexamined until the
//! next tick fifteen minutes later.
//!
//! A transient error recovers from that. A row that fails every time does not:
//! the sweep would abort at the same place forever, and the payouts behind it
//! would never be reconciled at all. That is money left `pending` against a
//! debited balance, which is the sentence `main.rs` prints when this fails and
//! the reason the worker exists.

use crate::common::TestApp;
use async_trait::async_trait;
use skilluv_backend::services::payout::{
    PayoutProvider, PayoutReceipt, PayoutRegistry, PayoutRequest, Rail, Recipient,
};
use skilluv_backend::services::reconciliation;
use uuid::Uuid;

/// A provider that answers every status question with a failure.
///
/// The rail is real and the credentials are not, which is the shape of a
/// provider having a bad afternoon.
struct AlwaysUnhappy;

#[async_trait]
impl PayoutProvider for AlwaysUnhappy {
    fn name(&self) -> &'static str {
        "stripe"
    }
    fn rail(&self) -> Rail {
        Rail::BankAccount
    }
    fn supports(&self, _currency: skilluv_backend::services::ledger::Currency) -> bool {
        true
    }
    async fn pay(
        &self,
        _request: &PayoutRequest<'_>,
        _recipient: &Recipient,
    ) -> Result<PayoutReceipt, skilluv_backend::errors::AppError> {
        Err(skilluv_backend::errors::AppError::Internal(
            "not used".into(),
        ))
    }
    async fn status(
        &self,
        _reference: &str,
    ) -> Result<
        Option<skilluv_backend::services::payout::PayoutState>,
        skilluv_backend::errors::AppError,
    > {
        Err(skilluv_backend::errors::AppError::Internal(
            "the provider is unreachable".into(),
        ))
    }
}

/// Writes an unconfirmed payout old enough for the sweep to pick up.
async fn stale_payout(app: &TestApp, user: Uuid, provider: &str, reference: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO payouts
             (user_id, provider, provider_reference, rail, amount, currency,
              status, created_at)
         VALUES ($1, $2, $3, 'bank_account', 10.0000, 'EUR', 'pending',
                 NOW() - INTERVAL '3 hours')
         RETURNING id",
    )
    .bind(user)
    .bind(provider)
    .bind(reference)
    .fetch_one(&app.db)
    .await
    .expect("insert payout")
}

/// Every row in the page is examined, whatever the ones before it did.
#[tokio::test]
async fn a_page_of_payouts_is_worked_through_to_the_end() {
    let app = TestApp::spawn().await;
    app.register_user("payee").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'payee'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // Three shapes the loop handles differently, in the order it meets them:
    // no reference at all, a provider this deployment cannot reach, and a
    // provider it does not hold credentials for.
    let no_reference = stale_payout(&app, user, "stripe", None).await;
    let unreachable = stale_payout(&app, user, "stripe", Some("pi_unreachable")).await;
    let unconfigured = stale_payout(&app, user, "fedapay", Some("fp_1")).await;

    let mut registry = PayoutRegistry::new();
    registry.register(std::sync::Arc::new(AlwaysUnhappy));

    let report = reconciliation::sweep(&app.db, &registry)
        .await
        .expect("a sweep must not fail as a whole because rows inside it did");

    assert_eq!(
        report.checked, 3,
        "all three payouts have to be examined, not just the ones before the first problem"
    );
    assert_eq!(
        report.errored, 0,
        "none of these three is a failure of the sweep itself"
    );

    // And the bookkeeping proves the loop reached each of them rather than
    // reporting a count it did not earn.
    for (id, label) in [
        (no_reference, "no reference"),
        (unreachable, "unreachable provider"),
    ] {
        let checks: i32 = sqlx::query_scalar("SELECT check_count FROM payouts WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(checks, 1, "the payout with {label} was never looked at");
    }

    // The unconfigured one is deliberately not counted as checked and not
    // touched: the deployment holds no credentials for it, which is not an
    // error and not work done either.
    let checks: i32 = sqlx::query_scalar("SELECT check_count FROM payouts WHERE id = $1")
        .bind(unconfigured)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        checks, 1,
        "the row is stamped before the provider is looked up, so it does not spin"
    );
}

/// A payout with no reference reaches a human rather than being retried.
#[tokio::test]
async fn a_payout_the_provider_never_acknowledged_is_escalated_once() {
    let app = TestApp::spawn().await;
    app.register_user("payee2").await;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'payee2'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    stale_payout(&app, user, "stripe", None).await;

    let mut registry = PayoutRegistry::new();
    registry.register(std::sync::Arc::new(AlwaysUnhappy));

    let first = reconciliation::sweep(&app.db, &registry).await.unwrap();
    assert_eq!(first.escalated, 1, "no reference means straight to a human");

    // Twice is not twice as loud. A queue that re-announces the same stuck
    // payout every fifteen minutes is a queue people mute.
    let announcements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE kind = 'admin.payout_needs_replay'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(announcements <= 1, "the same payout was announced twice");
}
