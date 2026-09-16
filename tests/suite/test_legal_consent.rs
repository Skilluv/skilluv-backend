//! Recording consent, including from a browser whose session outlived its user.
//!
//! A cookie banner is the first thing a page does, before anything has signed
//! in, and it answered `500 DATABASE_ERROR` on staging. The cause was not the
//! banner: a signed access token names a user for fifteen minutes whether or
//! not the row still exists, and `consent_log.user_id` has a foreign key.
//!
//! Rebuilding a staging database under an open tab is the ordinary way to
//! produce that, and deleting an account is the other.

use crate::common::TestApp;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn anonymous_consent_is_recorded_without_a_user() {
    let app = TestApp::spawn().await;

    let resp = app
        .post(
            "/api/legal/consent",
            &json!({ "analytics": false, "marketing": false }),
        )
        .await;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "the banner answers before login"
    );

    let orphans: i64 = sqlx::query_scalar("SELECT count(*) FROM consent_log WHERE user_id IS NULL")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert!(orphans >= 1, "an anonymous decision is still a decision");
}

#[tokio::test]
async fn consent_from_a_signed_in_person_is_attached_to_them() {
    let app = TestApp::spawn().await;
    app.register_user("consent_live").await;
    app.login("consent_live").await;

    let resp = app
        .post(
            "/api/legal/consent",
            &json!({ "analytics": true, "marketing": false }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let uid: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'consent_live'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let attached: i64 = sqlx::query_scalar("SELECT count(*) FROM consent_log WHERE user_id = $1")
        .bind(uid)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(attached, 1, "a signed-in decision names who made it");

    // And the cached copy on the user row, which is what quick checks read.
    let (version, analytics): (i32, bool) = sqlx::query_as(
        "SELECT consent_version_accepted, consent_analytics FROM users WHERE id = $1",
    )
    .bind(uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(version >= 1);
    assert!(analytics);
}

/// The one that was a 500.
///
/// The token is still signed and still unexpired; only the row is gone. The
/// insert used to hit `consent_log_user_id_fkey` and the banner reported
/// `DATABASE_ERROR` to somebody who had not done anything wrong.
///
/// The column is nullable and `ON DELETE SET NULL`, so the schema already
/// says a consent event may belong to nobody. This holds the write to the
/// same statement.
#[tokio::test]
async fn consent_still_records_when_the_session_outlived_its_user() {
    let app = TestApp::spawn().await;
    app.register_user("consent_ghost").await;
    app.login("consent_ghost").await;

    let uid: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'consent_ghost'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM consent_log")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // What a database rebuild does to an open tab, minus the rebuild. The
    // cookie jar keeps the token, which stays valid because nothing about it
    // depends on the row.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app
        .post(
            "/api/legal/consent",
            &json!({ "analytics": true, "marketing": true }),
        )
        .await;
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        status, 200,
        "a token naming a deleted user is not a server error: {body}"
    );

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM consent_log")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(after, before + 1, "the decision is still recorded");

    // Recorded as anonymous, because that is what it now is.
    let orphan: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM consent_log WHERE user_id IS NULL AND marketing = TRUE",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(orphan >= 1, "it belongs to nobody, and says so");
}
