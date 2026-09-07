//! SKI-368 — a fixture cannot be written into the public catalogue.
//!
//! Four orientations named `e2e-orient-<id>` were created on production
//! through `POST /admin/orientations` by a suite with no teardown. They were
//! curated and unarchived, so they passed every public filter: the signup
//! flow's trade carousel offered them, and `GET /orientation-counts` reported
//! 37 code orientations where 33 were real.
//!
//! The guard is on the write, not on the read. `is_curated` and `is_archived`
//! were behaving exactly as designed — teaching the public catalogue to
//! recognise test data would put that knowledge in the wrong place.

use crate::common::TestApp;
use serde_json::json;

/// The prefixes `refuse_reserved_slug` refuses. Written out rather than
/// imported: the constant lives in a private module, and a test that restates
/// the rule is what notices when the rule changes without the catalogue being
/// re-checked.
const RESERVED: [&str; 3] = ["e2e-", "fixture-", "tmp-"];

/// No seeded orientation sits in the reserved space.
///
/// This is the half that protects real trades. Six quality-domain slugs start
/// with `test-`, which is why `test-` is not reserved; if anyone reserves a
/// prefix that the catalogue already uses, a real trade silently becomes
/// uncreatable and this fails instead.
#[tokio::test]
async fn no_real_trade_lives_in_the_reserved_space() {
    let app = TestApp::spawn().await;

    for prefix in RESERVED {
        let clashing: Vec<String> = sqlx::query_scalar(
            "SELECT slug FROM orientations WHERE slug LIKE $1 || '%' ORDER BY slug",
        )
        .bind(prefix)
        .fetch_all(&app.db)
        .await
        .unwrap();

        assert!(
            clashing.is_empty(),
            "`{prefix}` is reserved for fixtures but the catalogue already \
             ships {clashing:?} — reserving it makes a real trade uncreatable"
        );
    }
}

/// A test environment may still create its own fixtures.
///
/// The suite runs with `environment = "test"`, so the guard stands down here.
/// That is also what proves it reads the environment rather than refusing
/// unconditionally — a guard that refused everywhere would pass the negative
/// test above and be useless.
#[tokio::test]
async fn the_suite_can_still_create_a_fixture() {
    let app = TestApp::spawn().await;
    app.register_admin("orientfixture").await;
    app.login("orientfixture").await;

    let resp = app
        .post(
            "/api/admin/orientations",
            &json!({
                "slug": "e2e-orient-from-the-suite",
                "name": "E2E Orientation from the suite",
                "description": "Created by a test, in a test environment.",
                "primary_domain": "code",
            }),
        )
        .await;

    assert!(
        resp.status().is_success(),
        "a test environment creates its own fixtures: {}",
        resp.text().await.unwrap()
    );
}
