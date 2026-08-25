//! The catalogue of declarable platforms, and the one thing it kept getting
//! wrong.
//!
//! `portfolio_platforms.has_public_api` was read by the sweep as "somebody can
//! read this", and it only ever meant "an API exists". Sixteen platforms were
//! selected every pass so that the fetcher could fall through to its catch-all
//! arm, log a warning and write back nothing. Migration 0537 split the two
//! meanings; these tests hold them apart.

mod common;
use common::TestApp;

use skilluv_backend::services::code_portfolio::SYNCED_HERE;
use skilluv_backend::services::portfolio_sync::SYNCABLE;

/// Each sweep's list matches the rows the catalogue gives it.
///
/// Two workers read `user_external_portfolios`, and the column says which one
/// owns a platform. Before it existed they overlapped in both directions:
/// `portfolio_sync` selected the forges and handed them to a `match` with no
/// arm for them, and `code_portfolio` selected everything — stamping
/// `last_synced_at` on a dev.to row it could not read, so the module that
/// could never saw it come due.
#[tokio::test]
async fn each_sweep_owns_exactly_the_platforms_it_can_read() {
    let app = TestApp::spawn().await;

    for (worker, in_code) in [
        ("portfolio_sync", SYNCABLE),
        ("code_portfolio", SYNCED_HERE),
    ] {
        let mut in_catalogue: Vec<String> =
            sqlx::query_scalar("SELECT slug FROM portfolio_platforms WHERE synced_by = $1")
                .bind(worker)
                .fetch_all(&app.db)
                .await
                .unwrap();
        in_catalogue.sort();

        let mut expected: Vec<String> = in_code.iter().map(|s| s.to_string()).collect();
        expected.sort();

        assert_eq!(
            in_catalogue, expected,
            "{worker} and the catalogue disagree about which platforms it reads"
        );
    }

    // And no platform belongs to both. The column makes that structurally
    // impossible; it is asserted anyway, because it is the property that
    // broke.
    let overlap: Vec<String> = SYNCABLE
        .iter()
        .filter(|s| SYNCED_HERE.contains(s))
        .map(|s| s.to_string())
        .collect();
    assert!(overlap.is_empty(), "two sweeps claim {overlap:?}");
}

/// A platform with an API nobody reads yet is not swept.
///
/// The distinction is worth keeping rather than collapsing: `has_public_api`
/// is the shortlist of what is worth building next, and the sweep must not
/// touch it until somebody does.
#[tokio::test]
async fn an_api_that_exists_is_not_an_api_that_is_read() {
    let app = TestApp::spawn().await;

    let promised_but_unread: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM portfolio_platforms
          WHERE has_public_api AND synced_by IS NULL
          ORDER BY slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        !promised_but_unread.is_empty(),
        "if this list is empty the two columns say the same thing and one of \
         them should go"
    );

    // The other direction: a worker assigned a platform that publishes
    // nothing would fail on it every pass, forever.
    let would_be_swept: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM portfolio_platforms
          WHERE synced_by IS NOT NULL AND NOT has_public_api",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(
        would_be_swept, 0,
        "a fetcher was written for a platform the catalogue says publishes nothing"
    );
}

/// Every domain a member can belong to can hold a portfolio.
///
/// Leadership and quality had no rows at all until 0537 — the two domains
/// this branch opened were the two whose members could not declare anything.
/// The four rows with no domain serve everybody, so a domain is covered by
/// its own rows or by those.
#[tokio::test]
async fn every_open_domain_has_somewhere_to_declare() {
    let app = TestApp::spawn().await;

    let bare: Vec<String> = sqlx::query_scalar(
        "SELECT d.slug
           FROM skill_domains d
          WHERE d.is_active
            AND NOT EXISTS (SELECT 1 FROM portfolio_platforms p
                             WHERE p.skill_domain = d.slug)
          ORDER BY d.slug",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    // Named rather than filtered out by a rule, so opening a domain fails
    // this test until somebody decides which case it is.
    //
    // `security`, `soft_skills` and `game` predate the per-domain catalogue
    // and lean on the four shared rows. `design` is the opposite case: it
    // connects accounts through OAuth in `design_cloud_connections` rather
    // than declaring a handle, which is a stronger claim than this table can
    // make and not a gap.
    let leaning_on_the_shared_rows = ["design", "game", "security", "soft_skills"];
    let unexpected: Vec<&String> = bare
        .iter()
        .filter(|d| !leaning_on_the_shared_rows.contains(&d.as_str()))
        .collect();

    assert!(
        unexpected.is_empty(),
        "these domains are open and have no platform of their own: {unexpected:?}"
    );
}

/// A declared figure is never presented as a fetched one.
///
/// The platforms added by 0537 are all manual: LinkedIn closed its
/// endorsements API, and neither HackerOne nor Bugcrowd publishes somebody
/// else's disclosures. That is fine — a disclosed report's proof is the link.
/// What is not fine is a figure somebody typed being counted as one that was
/// read, and `figures_are_declared` is the column that separates them.
#[tokio::test]
async fn the_new_platforms_are_honest_about_being_declared() {
    let app = TestApp::spawn().await;

    let syncable_but_manual: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM portfolio_platforms
          WHERE skill_domain IN ('leadership', 'quality') AND synced_by IS NOT NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        syncable_but_manual.is_empty(),
        "nothing fetches these yet, and saying otherwise would present a typed \
         figure as a read one: {syncable_but_manual:?}"
    );

    for domain in ["leadership", "quality"] {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM portfolio_platforms WHERE skill_domain = $1")
                .bind(domain)
                .fetch_one(&app.db)
                .await
                .unwrap();
        assert!(count >= 3, "{domain} has {count} platforms");
    }
}
