//! Adding a payout provider must cost one adapter and some rows.
//!
//! The claim the routing table exists to make is that a new rail changes no
//! caller and no branch. These tests hold it to that: they resolve providers
//! through the same path a withdrawal takes, and they assert on the rows
//! rather than on anything compiled in.

mod common;

use common::TestApp;
use skilluv_backend::services::ledger::Currency;
use skilluv_backend::services::payout::{self, Rail};
use skilluv_backend::services::payout_adapters::{FedaPayPayout, MomoPayout, StripePayout};
use std::sync::Arc;

/// A registry holding every adapter, regardless of what this machine has
/// credentials for. `registry_from_env` deliberately skips unconfigured
/// providers, which is right in production and useless for testing routing.
fn full_registry() -> payout::PayoutRegistry {
    use skilluv_backend::services::mobile_money::ProviderName;

    let mut registry = payout::PayoutRegistry::new();
    registry.register(Arc::new(FedaPayPayout {
        cfg: skilluv_backend::services::fedapay::FedaPayConfig {
            secret_key: "sk_sandbox_test".into(),
            live: false,
        },
    }));
    for operator in [ProviderName::Orange, ProviderName::Mtn, ProviderName::Wave] {
        registry.register(Arc::new(MomoPayout { operator }));
    }
    // Built from a stub rather than from the environment, like FedaPay
    // above. Whether a provider has an adapter is a question about the
    // code; gating it on credentials made this registry claim to be full
    // while missing Stripe on any machine without a key, so the orphan
    // check below reported the EUR bank corridor as unreachable.
    registry.register(Arc::new(StripePayout {
        cfg: skilluv_backend::services::stripe::StripeConfig {
            secret_key: "sk_test_routing".into(),
            webhook_secret: "whsec_test_routing".into(),
            success_url: "https://example.test/ok".into(),
            cancel_url: "https://example.test/ko".into(),
        },
    }));
    registry
}

#[tokio::test]
async fn niger_and_guinea_are_reachable_now_and_were_not_before() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    for country in ["NE", "GN"] {
        let provider = registry
            .resolve(&app.db, Some(country), Currency::Xof, Rail::MobileMoney)
            .await
            .unwrap_or_else(|e| panic!("{country} has no payout route: {e}"));
        assert_eq!(
            provider.name(),
            "fedapay",
            "{country} has no direct operator integration"
        );
    }
}

#[tokio::test]
async fn a_direct_operator_still_wins_over_the_aggregator() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    // FedaPay is a fallback where a direct rail exists: it costs more per
    // transfer, and the point of adding it was coverage, not replacement.
    for (country, expected) in [("BJ", "mtn"), ("CI", "orange"), ("SN", "wave")] {
        let provider = registry
            .resolve(&app.db, Some(country), Currency::Xof, Rail::MobileMoney)
            .await
            .expect("a route exists");
        assert_eq!(
            provider.name(),
            expected,
            "wrong primary rail for {country}"
        );
    }
}

#[tokio::test]
async fn disabling_a_route_falls_through_to_the_next_one() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    // The reason `enabled` exists: an operator outage should be one column,
    // reversible, with no deployment.
    sqlx::query(
        "UPDATE payout_routes SET enabled = FALSE
          WHERE country = 'BJ' AND currency = 'XOF' AND provider IN ('mtn', 'orange')",
    )
    .execute(&app.db)
    .await
    .expect("disable the direct Benin routes");

    let provider = registry
        .resolve(&app.db, Some("BJ"), Currency::Xof, Rail::MobileMoney)
        .await
        .expect("the fallback takes over");
    assert_eq!(provider.name(), "fedapay");
}

#[tokio::test]
async fn a_provider_without_credentials_is_skipped_not_fatal() {
    let app = TestApp::spawn().await;

    // A registry that knows only FedaPay, as a deployment holding no Mobile
    // Money credentials would be. Benin's first two routes name providers it
    // does not have; resolution must walk past them rather than fail.
    let mut registry = payout::PayoutRegistry::new();
    registry.register(Arc::new(FedaPayPayout {
        cfg: skilluv_backend::services::fedapay::FedaPayConfig {
            secret_key: "sk_sandbox_test".into(),
            live: false,
        },
    }));

    let provider = registry
        .resolve(&app.db, Some("BJ"), Currency::Xof, Rail::MobileMoney)
        .await
        .expect("an unconfigured route is skipped, not fatal");
    assert_eq!(provider.name(), "fedapay");
}

#[tokio::test]
async fn an_unreachable_destination_says_which_routes_it_tried() {
    let app = TestApp::spawn().await;
    let registry = full_registry();

    // Stripe cannot pay out to Benin, and no Mobile Money rail deals in EUR.
    // The failure has to name what was attempted, or the next person debugs
    // it by reading the routing table by hand.
    let outcome = registry
        .resolve(&app.db, Some("BJ"), Currency::Eur, Rail::MobileMoney)
        .await;
    let Err(err) = outcome else {
        panic!("EUR over Mobile Money reached a provider");
    };
    let message = err.to_string();
    assert!(message.contains("EUR"), "the message hides the currency");
    assert!(message.contains("BJ"), "the message hides the country");
}

#[tokio::test]
async fn every_routed_provider_exists_in_the_code() {
    let app = TestApp::spawn().await;
    let registry = full_registry();
    let known = registry.names();

    // A row naming a provider nothing implements is a destination that
    // silently resolves to the next route, or to nothing at all. The table is
    // edited without a deployment, so nothing else catches this.
    let routed: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT provider FROM payout_routes WHERE enabled = TRUE")
            .fetch_all(&app.db)
            .await
            .expect("read the routes");

    let orphans: Vec<&String> = routed
        .iter()
        .filter(|p| !known.contains(&p.as_str()))
        .collect();
    assert!(
        orphans.is_empty(),
        "routes name providers no adapter implements: {orphans:?} (known: {known:?})"
    );
}

#[tokio::test]
async fn the_xof_catch_all_points_at_a_provider_that_covers_the_zone() {
    let app = TestApp::spawn().await;

    // It used to name a single operator, promising coverage in countries
    // where that integration had never been exercised.
    let provider: String = sqlx::query_scalar(
        "SELECT provider FROM payout_routes
          WHERE country IS NULL AND currency = 'XOF' AND rail = 'mobile_money'
            AND enabled = TRUE
          ORDER BY priority
          LIMIT 1",
    )
    .fetch_one(&app.db)
    .await
    .expect("the XOF zone has a catch-all");
    assert_eq!(provider, "fedapay");
}
