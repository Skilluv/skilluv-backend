//! LO-07 — the right to export and the right to erasure actually happen.
//!
//! A right to erasure that leaves rows naming the user is what a complaint
//! finds. `DELETE /api/auth/account` tombstones the account: erasure::erase
//! wipes the personal identifiers (email -> ...@invalid, username ->
//! supprime-..., deleted_at set) and deletes the purely-personal tables. This
//! asserts the promise, end to end, and that the export endpoint is reachable.

mod common;

use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn erasure_tombstones_the_account_and_export_is_reachable() {
    let app = TestApp::spawn().await;
    let reg = app.register_user("rgpd_user").await;
    let uid: uuid::Uuid = reg["data"]["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("uuid");
    app.login("rgpd_user").await;

    // Export: the request is accepted. The content contract (per PRIVACY.md) is
    // a separate concern; this holds that the promise is reachable, not spam.
    let resp = app.post("/api/auth/me/data-export", &json!({})).await;
    assert!(
        resp.status().is_success() || resp.status().as_u16() == 202,
        "data export was refused: {}",
        resp.status()
    );

    // Erasure requires the account password.
    let resp = app
        .delete_with_body(
            "/api/auth/account",
            &json!({ "password": TestApp::TEST_PASSWORD }),
        )
        .await;
    assert!(
        resp.status().is_success(),
        "account deletion was refused: {}",
        resp.status()
    );

    // The row is tombstoned: identifiers wiped, deleted_at set.
    let (email, deleted_at, username): (String, Option<chrono::DateTime<chrono::Utc>>, String) =
        sqlx::query_as("SELECT email, deleted_at, username FROM users WHERE id = $1")
            .bind(uid)
            .fetch_one(&app.db)
            .await
            .expect("user row still present as a tombstone");
    assert!(deleted_at.is_some(), "deleted_at was not set");
    assert!(email.ends_with("@invalid"), "email not anonymised: {email}");
    assert!(
        username.starts_with("supprime-"),
        "username not tombstoned: {username}"
    );

    // The original email is gone from the table.
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = $1")
        .bind("rgpd_user@test.com")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(remaining, 0, "the original email survived erasure");
}
