//! Taking money must cost the same to extend as sending it.
//!
//! The asymmetry this closes: payouts had a trait, a routing table and one
//! adapter per provider, while collection had a registry nobody constructed
//! and three route modules calling Stripe directly. The practical
//! consequence was that a Beninese enterprise could not pay at all, because
//! Stripe does not take Mobile Money there.

mod common;

use common::TestApp;
use skilluv_backend::services::collect::{self, CollectionRegistry, Method};
use skilluv_backend::services::collect_adapters::{FedaPayCollect, StripeCollect};
use skilluv_backend::services::ledger::Currency;
use std::sync::Arc;

/// Every adapter, whatever this machine holds credentials for.
fn full_registry() -> CollectionRegistry {
    let mut registry = CollectionRegistry::new();
    registry.register(Arc::new(StripeCollect {
        cfg: skilluv_backend::services::stripe::StripeConfig {
            secret_key: "sk_test".into(),
            webhook_secret: "whsec_test".into(),
            success_url: "https://skill-uv.com/ok".into(),
            cancel_url: "https://skill-uv.com/no".into(),
        },
    }));
    registry.register(Arc::new(FedaPayCollect {
        cfg: skilluv_backend::services::fedapay::FedaPayConfig {
            secret_key: "sk_sandbox_test".into(),
            live: false,
        },
        callback_url: "https://skill-uv.com/payment/return".into(),
    }));
    registry
}

#[tokio::test]
async fn mobile_money_in_the_franc_zone_is_reachable_now() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    // The whole point. Roughly seventy percent of adults in the UEMOA hold
    // money this way, against about a quarter with a bank account, and
    // before this there was no route at all.
    for country in ["BJ", "CI", "SN", "TG", "BF", "NE"] {
        let provider = registry
            .resolve(&app.db, Some(country), Currency::Xof, Method::MobileMoney)
            .await
            .unwrap_or_else(|e| panic!("{country} cannot pay by Mobile Money: {e}"));
        assert_eq!(provider.name(), "fedapay");
    }
}

#[tokio::test]
async fn a_card_in_euros_still_goes_to_stripe() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    let provider = registry
        .resolve(&app.db, Some("FR"), Currency::Eur, Method::Card)
        .await
        .expect("cards in EUR are routed");
    assert_eq!(provider.name(), "stripe");
}

#[tokio::test]
async fn nobody_is_routed_at_a_provider_that_cannot_serve_them() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    // Stripe does not settle XOF. Routing a Beninese payer there would fail
    // at the API with a message about currencies, rather than here with one
    // about coverage.
    let outcome = registry
        .resolve(&app.db, Some("BJ"), Currency::Eur, Method::MobileMoney)
        .await;
    let Err(err) = outcome else {
        panic!("EUR over Mobile Money reached a provider");
    };
    let message = err.to_string();
    assert!(message.contains("EUR"), "the message hides the currency");
    assert!(
        message.contains("mobile_money"),
        "the message hides the method"
    );
}

#[tokio::test]
async fn disabling_a_route_is_one_column() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    sqlx::query(
        "UPDATE collection_routes SET enabled = FALSE
          WHERE currency = 'XOF' AND method = 'mobile_money'",
    )
    .execute(&app.db)
    .await
    .unwrap();

    // An outage at a provider must be reversible without a deployment.
    let outcome = registry
        .resolve(&app.db, Some("BJ"), Currency::Xof, Method::MobileMoney)
        .await;
    assert!(outcome.is_err());
}

#[tokio::test]
async fn every_routed_provider_exists_in_the_code() {
    let app = TestApp::spawn().await;
    let registry = full_registry();
    let known = registry.names();

    // A row naming a provider nothing implements is a corridor that looks
    // open and is not. The table is edited without a deployment, so nothing
    // else catches this.
    let routed: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT provider FROM collection_routes WHERE enabled = TRUE")
            .fetch_all(&app.db)
            .await
            .unwrap();

    let orphans: Vec<&String> = routed
        .iter()
        .filter(|p| !known.contains(&p.as_str()))
        .collect();
    assert!(
        orphans.is_empty(),
        "routes name providers no adapter implements: {orphans:?}"
    );
}

#[tokio::test]
async fn a_charge_is_recorded_before_the_provider_is_asked() {
    let app = TestApp::spawn().await;
    let registry = full_registry();
    let subject = uuid::Uuid::new_v4();

    let provider = registry
        .resolve(&app.db, Some("BJ"), Currency::Xof, Method::MobileMoney)
        .await
        .unwrap();

    // The sandbox key is not real, so the provider call fails. What must
    // survive is the row: a charge nobody recorded is one nobody can ever
    // refund, which is exactly why `refund_from_dispute` used to move the
    // books over a card that was never credited.
    let amount = bigdecimal::BigDecimal::from(5000);
    let attempt = collect::start(
        &app.db,
        provider.as_ref(),
        Method::MobileMoney,
        collect::CollectionRequest {
            payer_id: None,
            payer_enterprise_id: None,
            payer_email: "awa@test.dev",
            payer_name: "Awa Diallo",
            payer_country: Some("BJ"),
            payer_phone: Some("+22997000000"),
            subject_type: "mentorship_session",
            subject_id: subject,
            amount: &amount,
            currency: Currency::Xof,
            description: "session",
            success_url: "https://skill-uv.com/ok",
            cancel_url: "https://skill-uv.com/no",
            idempotency_key: "test:collect:1",
            operator: Some("mtn"),
            credits: None,
            merchant_reference: None,
        },
    )
    .await;
    assert!(attempt.is_err(), "a fake key cannot open a real checkout");

    let (status, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT status, failure_reason FROM payments
          WHERE subject_type = 'mentorship_session' AND subject_id = $1",
    )
    .bind(subject)
    .fetch_one(&app.db)
    .await
    .expect("the attempt was recorded before the provider was asked");

    // Failed, not pending: a checkout that never opened is not one the
    // payer abandoned, and a reconciliation that cannot tell them apart is
    // one nobody trusts.
    assert_eq!(status, "failed");
    assert!(reason.is_some(), "the provider's own words are kept");
}

#[tokio::test]
async fn giving_back_more_than_was_taken_is_impossible() {
    let app = TestApp::spawn().await;
    let subject = uuid::Uuid::new_v4();

    sqlx::query(
        "INSERT INTO payments
            (subject_type, subject_id, provider, method, amount, currency, status,
             succeeded_at, provider_reference)
         VALUES ('mentorship_session', $1, 'stripe', 'card', 100, 'EUR', 'succeeded',
                 NOW(), 'pi_test')",
    )
    .bind(subject)
    .execute(&app.db)
    .await
    .unwrap();

    let too_much = sqlx::query("UPDATE payments SET refunded_amount = 150 WHERE subject_id = $1")
        .bind(subject)
        .execute(&app.db)
        .await;
    assert!(
        too_much.is_err(),
        "refunding more than was charged is a gift funded by a bug"
    );
}

#[tokio::test]
async fn a_refund_needs_the_providers_own_reference() {
    let app = TestApp::spawn().await;
    let registry = full_registry();
    let subject = uuid::Uuid::new_v4();

    // A succeeded charge with no provider reference: the state every
    // payment was in before this table existed. `refund` must report that
    // it did nothing rather than let the caller believe a card was
    // credited.
    sqlx::query(
        "INSERT INTO payments
            (subject_type, subject_id, provider, method, amount, currency, status, succeeded_at)
         VALUES ('mentorship_session', $1, 'stripe', 'card', 100, 'EUR', 'succeeded', NOW())",
    )
    .bind(subject)
    .execute(&app.db)
    .await
    .unwrap();

    let refunded = collect::refund(
        &app.db,
        &registry,
        "mentorship_session",
        subject,
        "dispute settled for the payer",
    )
    .await
    .expect("reports rather than fails");
    assert!(refunded.is_none(), "nothing was refunded at the provider");
}
