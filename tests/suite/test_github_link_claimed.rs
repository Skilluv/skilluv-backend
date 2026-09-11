//! One GitHub account, one Skilluv profile - said in a sentence.
//!
//! `github_connections` carries two unique constraints: `user_id` as its
//! primary key, and `github_user_id`. The upsert that ends the OAuth callback
//! named the first one only, so a second account finishing the dance with a
//! GitHub identity that already belonged to a first one did not conflict on
//! `user_id` at all - it was rejected on
//! `github_connections_github_user_id_key`, and what the person read was
//! `DATABASE_ERROR`, 500, at the end of a browser redirect, inside onboarding
//! where the frontend removes the navbar.
//!
//! Both constraints are right and both stay. These tests hold the part that
//! was missing: the refusal is a 409 with words, and the first link survives
//! the attempt.

use crate::common::TestApp;
use skilluv_backend::services::{github, oauth};
use sqlx::PgPool;
use uuid::Uuid;

async fn user_id(app: &TestApp, username: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap()
}

/// Stand in for the tail of the OAuth callback.
///
/// The exchange with GitHub is GitHub's and is not what can break here; the
/// row this writes is. The token bytes are opaque to the table - it stores
/// ciphertext - so any bytes stand in for a real one.
async fn connect(db: &PgPool, user: Uuid, github_user_id: i64, login: &str) -> Result<(), String> {
    github::upsert_connection(
        db,
        user,
        github_user_id,
        login,
        Some("repo,read:user"),
        b"ciphertext",
        b"nonce",
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn github_profile(github_user_id: i64, login: &str) -> oauth::OAuthProfile {
    oauth::OAuthProfile {
        provider: "github",
        provider_user_id: github_user_id.to_string(),
        email: None,
        email_verified: false,
        display_name: Some("Ama".into()),
        avatar_url: None,
        username: Some(login.to_string()),
    }
}

/// The guard explains the constraint - it does not replace it.
///
/// If this index were ever dropped in favour of the check in
/// `services::oauth::refuse_if_claimed`, two concurrent callbacks would both
/// pass the check and both insert. The database is what makes the rule true;
/// the Rust is what makes it readable.
#[tokio::test]
async fn the_database_still_refuses_it_on_its_own() {
    let app = TestApp::spawn().await;

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM pg_constraint
            WHERE conrelid = 'github_connections'::regclass
              AND contype = 'u'
              AND pg_get_constraintdef(oid) LIKE '%github_user_id%'
         )",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(
        exists,
        "github_user_id must stay UNIQUE - the guard in services::oauth is a message, not the rule"
    );
}

#[tokio::test]
async fn one_github_account_cannot_dress_two_profiles() {
    let app = TestApp::spawn().await;
    app.register_user("gh_claim_a").await;
    app.register_user("gh_claim_b").await;
    let a = user_id(&app, "gh_claim_a").await;
    let b = user_id(&app, "gh_claim_b").await;

    connect(&app.db, a, 4242424, "ama").await.unwrap();
    let refused = connect(&app.db, b, 4242424, "ama").await;

    let err = refused.expect_err("the second claim must be refused");
    assert!(
        err.to_lowercase().contains("already linked"),
        "the refusal has to say what to do about it: {err}"
    );
    assert!(
        err.starts_with("Conflict:"),
        "this is a 409, not a database error: {err}"
    );
    assert!(
        err.contains("GitHub"),
        "the provider is named the way the person writes it: {err}"
    );

    // And the first link is untouched - a failed claim must not steal it.
    let owner: Uuid =
        sqlx::query_scalar("SELECT user_id FROM github_connections WHERE github_user_id = $1")
            .bind(4242424_i64)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(owner, a, "the account that got there first keeps it");
}

/// The ordinary case the `ON CONFLICT (user_id)` arm was written for.
///
/// Reconnecting refreshes the token and the login. Nothing about the guard may
/// stand in the way of somebody re-authorising their own account.
#[tokio::test]
async fn reconnecting_your_own_account_still_refreshes_it() {
    let app = TestApp::spawn().await;
    app.register_user("gh_reconnect").await;
    let uid = user_id(&app, "gh_reconnect").await;

    connect(&app.db, uid, 777001, "old-handle").await.unwrap();
    connect(&app.db, uid, 777001, "new-handle").await.unwrap();

    let login: String =
        sqlx::query_scalar("SELECT github_login FROM github_connections WHERE user_id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(login, "new-handle", "a rename has to land");
}

/// Changing which GitHub account is yours is allowed, as long as the new one
/// is free. The guard asks "does somebody else hold this", not "have you held
/// anything before".
#[tokio::test]
async fn swapping_to_an_unclaimed_account_is_allowed() {
    let app = TestApp::spawn().await;
    app.register_user("gh_swap").await;
    let uid = user_id(&app, "gh_swap").await;

    connect(&app.db, uid, 777002, "first").await.unwrap();
    connect(&app.db, uid, 777003, "second").await.unwrap();

    let id: i64 =
        sqlx::query_scalar("SELECT github_user_id FROM github_connections WHERE user_id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(id, 777003);
}

/// The repo-sync link and the sign-in button have to mean the same person.
///
/// `/auth/github/start` writes `github_connections` and nothing else, so the
/// sign-in flow - which looked only at `user_oauth_providers` - could not find
/// somebody who had connected GitHub from their settings. GitHub's `/user`
/// returns no email either, so there was no fallback: the flow created them a
/// second account, silently, with none of their proof attached to it. After
/// this change the two agree, and the guard above never has to fire for the
/// same person twice.
#[tokio::test]
async fn signing_in_finds_the_account_that_connected_for_repo_sync() {
    let app = TestApp::spawn().await;
    app.register_user("gh_bridge").await;
    let uid = user_id(&app, "gh_bridge").await;

    connect(&app.db, uid, 555123, "kwame").await.unwrap();

    let found = oauth::find_user_for_profile(&app.db, &github_profile(555123, "kwame"))
        .await
        .unwrap();

    assert_eq!(
        found,
        Some(uid),
        "the same GitHub identity must resolve to the same Skilluv account"
    );
}

/// The same rule, one table over.
///
/// `user_oauth_providers` has its own unique index on (provider,
/// provider_user_id), and `upsert_link` names `(user_id, provider)` - the same
/// shape of hole. It was guarded for Discord only, because Discord is where
/// the symptom was found first; Google and LinkedIn raised the raw violation.
#[tokio::test]
async fn a_google_identity_claimed_elsewhere_is_refused_in_words() {
    let app = TestApp::spawn().await;
    app.register_user("goog_a").await;
    app.register_user("goog_b").await;
    let a = user_id(&app, "goog_a").await;
    let b = user_id(&app, "goog_b").await;

    let profile = oauth::OAuthProfile {
        provider: "google",
        provider_user_id: "108877665544332211".into(),
        email: Some("ama@example.com".into()),
        email_verified: true,
        display_name: Some("Ama".into()),
        avatar_url: None,
        username: None,
    };

    oauth::upsert_link(&app.db, a, &profile).await.unwrap();
    let err = oauth::upsert_link(&app.db, b, &profile)
        .await
        .expect_err("the second claim must be refused")
        .to_string();

    assert!(
        err.to_lowercase().contains("already linked"),
        "the refusal has to say what to do about it: {err}"
    );
    assert!(
        err.contains("Google"),
        "the provider is named the way the person writes it: {err}"
    );
}
