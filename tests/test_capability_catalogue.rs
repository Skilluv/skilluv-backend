//! The capability catalogue, served (SKI-351).
//!
//! Part of it is generated: migration 0404 replaced a CHECK that five
//! migrations had restated with a table, and put a trigger on `orientations`
//! behind it. So the grantable set is a function of the trade catalogue, and
//! any copy held by a client is correct until somebody adds an orientation.
//!
//! The admin panel held such a copy, anchored to a CHECK that no longer exists.
//! `domain_curator:design`, `mission_arbiter` and `security_triager` gate three
//! surfaces shipped this week, and nothing could hand them to anybody.

mod common;
use common::TestApp;
use serde_json::Value;
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(&app.db)
    .await
    .unwrap();
    app.login(username).await;
    id
}

#[tokio::test]
async fn the_catalogue_serves_the_capabilities_that_gate_this_weeks_surfaces() {
    let app = TestApp::spawn().await;
    an_admin(&app, "cap_admin").await;

    let body: Value = app
        .get("/api/admin/capabilities")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"].as_array().expect("a list");
    let names: Vec<&str> = rows
        .iter()
        .map(|r| r["capability"].as_str().unwrap())
        .collect();

    // The three the ticket named, and the reason it was filed: each one gates
    // a surface, and none could be granted from the panel.
    for wanted in [
        "domain_curator:design",
        "mission_arbiter",
        "security_triager",
        "community_curator",
        "admin",
    ] {
        assert!(names.contains(&wanted), "{wanted} is not in the catalogue");
    }

    // Every row says what holding it lets somebody do. Without this an
    // operator choosing between `domain_curator:design` and
    // `community_curator` sees two slugs and picks the wider one.
    for r in rows {
        let d = r["description"].as_str().unwrap_or_default();
        assert!(
            d.len() > 10,
            "{} has no usable description",
            r["capability"]
        );
    }
}

#[tokio::test]
async fn the_generated_reviewer_capabilities_are_served_too() {
    let app = TestApp::spawn().await;
    an_admin(&app, "cap_admin2").await;

    let body: Value = app
        .get("/api/admin/capabilities")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"].as_array().unwrap();

    // These come from the trigger, not from any migration's literal list.
    // They are the half a client cannot hold as a constant, and the reason
    // this endpoint exists rather than a longer hard-coded array.
    let derived: Vec<&str> = rows
        .iter()
        .filter(|r| r["is_derived"] == true)
        .map(|r| r["capability"].as_str().unwrap())
        .collect();
    assert!(
        derived.len() >= 10,
        "only {} derived capabilities; the trigger of 0404 produces more",
        derived.len()
    );
    assert!(
        derived.iter().any(|c| c.starts_with("security_reviewer:")),
        "no security reviewer family: {derived:?}"
    );

    // `admin_security::require_reader` accepts `security_reviewer:%` through a
    // LIKE. That is only safe because the catalogue is what a grant is checked
    // against — asserted below.
    assert!(
        derived
            .iter()
            .any(|c| c.starts_with("challenge_validator:")),
        "no challenge validator: {derived:?}"
    );
}

/// The question the ticket raised, answered by the database rather than by
/// reading code: can any string be placed on a user?
///
/// No. Migration 0404 turned the CHECK into a foreign key, so an invented
/// capability is refused at write time. That matters because
/// `require_reader` matches `security_reviewer:%` with a LIKE: if arbitrary
/// strings could be stored, `security_reviewer:anything` would be a way in.
#[tokio::test]
async fn a_capability_that_is_not_in_the_catalogue_cannot_be_granted() {
    let app = TestApp::spawn().await;
    an_admin(&app, "cap_admin3").await;
    app.register_user("cap_target").await;
    let target: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'cap_target'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // Straight at the table, because the point is that the database refuses
    // it — not that a handler happens to check first.
    let refused = sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'security_reviewer:anything-i-like', 'test')",
    )
    .bind(target)
    .execute(&app.db)
    .await;
    assert!(
        refused.is_err(),
        "an invented capability was stored; require_reader's LIKE is then a way in"
    );
}

#[tokio::test]
async fn the_catalogue_is_not_public() {
    let app = TestApp::spawn().await;
    app.register_user("cap_nobody").await;
    app.login("cap_nobody").await;

    // Which capabilities exist is a map of this platform's privileges. Not
    // secret, but not something to hand an anonymous reader either.
    assert_eq!(app.get("/api/admin/capabilities").await.status(), 403);
}

#[tokio::test]
async fn the_catalogue_says_which_ones_the_engine_puts_back() {
    let app = TestApp::spawn().await;
    an_admin(&app, "cap_admin4").await;

    let body: Value = app
        .get("/api/admin/capabilities")
        .await
        .json()
        .await
        .unwrap();
    let rows = body["data"].as_array().unwrap();
    let managed = |name: &str| -> bool {
        rows.iter()
            .find(|r| r["capability"] == name)
            .map(|r| r["engine_managed"] == true)
            .unwrap_or(false)
    };

    // `services::capabilities_engine` grants and re-grants these. Revoking one
    // by hand does not stick, and an operator who does not know that spends an
    // afternoon on it.
    assert!(managed("mentor"), "mentor is engine-managed");
    assert!(managed("challenger"), "challenger is engine-managed");

    // And these are nominations. The engine says so itself: "trop sensible
    // pour de l'auto-promotion basée sur des compteurs."
    assert!(!managed("plagiarism_reviewer"));
    assert!(!managed("kyc_reviewer"));
    assert!(!managed("admin"));
}
