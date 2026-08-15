//! Review rights, granted by family of trade rather than one by one.

mod common;
use common::TestApp;
use uuid::Uuid;

/// A user holding one capability, and nothing else.
async fn user_with(app: &TestApp, username: &str, capability: Option<&str>) -> Uuid {
    app.register_user(username).await;
    let id: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();

    if let Some(capability) = capability {
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, granted_reason)
             VALUES ($1, $2, 'test')",
        )
        .bind(id)
        .bind(capability)
        .execute(&app.db)
        .await
        .expect("grant");
    }
    id
}

#[tokio::test]
async fn every_active_code_trade_belongs_to_a_reviewer_group() {
    let app = TestApp::spawn().await;

    let ungrouped: Vec<String> = sqlx::query_scalar(
        "SELECT slug FROM orientations
          WHERE primary_domain = 'code' AND NOT is_archived AND reviewer_group IS NULL",
    )
    .fetch_all(&app.db)
    .await
    .unwrap();

    assert!(
        ungrouped.is_empty(),
        "review rights cannot be granted for these at all: {ungrouped:?}"
    );
}

#[tokio::test]
async fn the_group_capability_covers_its_whole_family() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "rev_web", Some("code_reviewer:web")).await;

    // Five trades share the web family. One grant covers all of them, which
    // is the point of grouping.
    for slug in [
        "web-frontend-developer",
        "web-backend-developer",
        "web-fullstack-developer",
        "web-performance-engineer",
        "web3-frontend-developer",
    ] {
        skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
            &app.db, user, slug,
        )
        .await
        .unwrap_or_else(|e| panic!("{slug} should be reviewable: {e:?}"));
    }
}

#[tokio::test]
async fn a_grant_does_not_leak_into_another_family() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "rev_narrow", Some("code_reviewer:web")).await;

    // Judging a React component says nothing about judging a CUDA kernel.
    let refused = skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
        &app.db,
        user,
        "gpu-compute-developer",
    )
    .await;

    assert!(
        refused.is_err(),
        "web review rights must not reach the GPU family"
    );
}

#[tokio::test]
async fn the_wildcard_reaches_every_family() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "rev_all", Some("code_reviewer:all")).await;

    for slug in [
        "web-frontend-developer",
        "kernel-driver-developer",
        "hft-quant-developer",
        "lowcode-platform-developer",
    ] {
        skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
            &app.db, user, slug,
        )
        .await
        .unwrap_or_else(|e| panic!("the wildcard should reach {slug}: {e:?}"));
    }
}

#[tokio::test]
async fn holding_nothing_reviews_nothing() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "rev_none", None).await;

    let refused = skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
        &app.db,
        user,
        "web-frontend-developer",
    )
    .await;

    assert!(refused.is_err());
}

#[tokio::test]
async fn an_orientation_with_no_group_is_refused_and_says_why() {
    let app = TestApp::spawn().await;
    let user = user_with(&app, "rev_ungrouped", Some("code_reviewer:all")).await;

    // A trade nobody has been made responsible for. Refusing is the safe
    // answer, and the message has to name the fix rather than read as a
    // permission problem.
    sqlx::query(
        "INSERT INTO orientations (slug, name, description, primary_domain, is_curated)
         VALUES ('temp-ungrouped-trade', 'Sans groupe', 'x', 'code', TRUE)",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let err = skilluv_backend::middleware::capabilities::require_reviewer_for_orientation(
        &app.db,
        user,
        "temp-ungrouped-trade",
    )
    .await
    .expect_err("must refuse");

    assert!(
        format!("{err:?}").contains("reviewer group"),
        "the answer must name what is missing, got: {err:?}"
    );
}

#[tokio::test]
async fn the_capabilities_added_before_this_one_are_still_grantable() {
    let app = TestApp::spawn().await;

    // A CHECK cannot be extended, only replaced. Migration 0176 rewrote it,
    // and dropping a value here would silently disable the guard that reads
    // it — the validator workflow, and the apprentice sas.
    for (n, capability) in [
        "verified_apprentice",
        "apprentice_verifier",
        "challenge_validator:code",
        "challenge_validator:soft_skills",
        "community_curator",
    ]
    .iter()
    .enumerate()
    {
        // Numbered rather than named after the capability: usernames are
        // length-limited and "challenge_validator:soft_skills" does not fit.
        let user = user_with(&app, &format!("capkeep{n}"), Some(capability)).await;
        assert!(!user.is_nil());
    }
}
