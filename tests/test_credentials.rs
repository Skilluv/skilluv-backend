//! Certifications somebody else issued.
//!
//! The distinction these tests exist to hold: Skilluv records these, it does
//! not award them. Nothing is verified until a person opened the issuer's
//! page and wrote down what they saw, nothing lapsed reads as current, and a
//! credential never carries the weight of an attestation.

mod common;
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

async fn an_admin(app: &TestApp, username: &str) {
    // `register_admin`, not `role = 'admin'`: since P21 the admin gate reads
    // `user_capabilities`, and the column on its own opens nothing. The helper
    // grants the capability and enrols the passkey the admin 2FA middleware
    // wants, then logs in.
    app.register_admin(username).await;
}

#[tokio::test]
async fn a_credential_arrives_claimed_and_stays_claimed() {
    let app = TestApp::spawn().await;
    app.register_user("cred_holder").await;
    app.login("cred_holder").await;

    let resp = app
        .post(
            "/api/users/me/credentials",
            &json!({
                "issuer": "cncf",
                "name": "Certified Kubernetes Administrator",
                "level": "professional",
                "evidence_url": "https://www.credly.com/badges/example",
                "issued_on": "2025-03-01",
                "expires_on": "2027-03-01",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let jv: serde_json::Value = resp.json().await.unwrap();
    // The person who added it is the person it belongs to, which is exactly
    // why their word is not the check.
    assert!(jv["data"]["credential"]["verified_at"].is_null());
    assert_eq!(jv["data"]["credential"]["is_current"], true);

    drop(app);
}

#[tokio::test]
async fn an_expired_credential_is_not_current() {
    let app = TestApp::spawn().await;
    app.register_user("cred_lapsed").await;
    app.login("cred_lapsed").await;

    let resp = app
        .post(
            "/api/users/me/credentials",
            &json!({
                "issuer": "aws",
                "name": "AWS Certified Solutions Architect – Associate",
                "evidence_url": "https://www.credly.com/badges/old",
                "issued_on": "2019-01-10",
                "expires_on": "2022-01-10",
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let jv: serde_json::Value = resp.json().await.unwrap();
    // Derived from the date rather than stored, so a reader that forgot to
    // compare dates cannot show it as current anyway.
    assert_eq!(jv["data"]["credential"]["is_current"], false);

    drop(app);
}

#[tokio::test]
async fn a_credential_without_a_public_link_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("cred_nolink").await;
    app.login("cred_nolink").await;

    let resp = app
        .post(
            "/api/users/me/credentials",
            &json!({
                "issuer": "hashicorp",
                "name": "Terraform Associate",
                "evidence_url": "je l'ai, promis",
                "issued_on": "2025-01-01",
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        400,
        "a certification nobody can open is a line on a CV"
    );

    drop(app);
}

#[tokio::test]
async fn an_unknown_issuer_is_refused_with_a_way_out() {
    let app = TestApp::spawn().await;
    app.register_user("cred_unknown").await;
    app.login("cred_unknown").await;

    let resp = app
        .post(
            "/api/users/me/credentials",
            &json!({
                "issuer": "some-bootcamp",
                "name": "Certificat DevOps",
                "evidence_url": "https://example.com/x",
                "issued_on": "2025-01-01",
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let jv: serde_json::Value = resp.json().await.unwrap();
    let message = jv["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("other"),
        "the refusal points at the escape hatch rather than just saying no: {message}"
    );

    drop(app);
}

#[tokio::test]
async fn a_review_without_a_record_of_what_was_checked_is_refused() {
    let app = TestApp::spawn().await;
    app.register_user("cred_owner_a").await;
    app.login("cred_owner_a").await;

    let resp = app
        .post(
            "/api/users/me/credentials",
            &json!({
                "issuer": "google_cloud",
                "name": "Professional Cloud Architect",
                "evidence_url": "https://www.credly.com/badges/gcp",
                "issued_on": "2025-06-01",
            }),
        )
        .await;
    let jv: serde_json::Value = resp.json().await.unwrap();
    let id = jv["data"]["credential"]["id"].as_str().unwrap().to_string();

    an_admin(&app, "cred_admin_a").await;

    let resp = app
        .post(
            &format!("/api/admin/credentials/{id}/verify"),
            &json!({ "note": "ok" }),
        )
        .await;
    assert_eq!(resp.status(), 400, "'OK' is not a record of a check");

    let resp = app
        .post(
            &format!("/api/admin/credentials/{id}/verify"),
            &json!({ "note": "Page Credly ouverte, le nom correspond au compte." }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let (verified, note): (bool, Option<String>) = sqlx::query_as(
        "SELECT verified_at IS NOT NULL, verification_note
           FROM external_credentials WHERE id = $1::UUID",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(verified);
    assert!(note.unwrap().contains("Credly"));

    drop(app);
}

#[tokio::test]
async fn only_a_verified_and_current_credential_scores() {
    let app = TestApp::spawn().await;
    app.register_user("cred_scored").await;
    app.login("cred_scored").await;

    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'cred_scored'")
        .fetch_one(&app.db)
        .await
        .unwrap();

    // One verified and valid, one verified but lapsed, one still claimed.
    // Only the first should count.
    for (name, issued, expires, verified) in [
        ("CKA", "2025-01-01", Some("2099-01-01"), true),
        ("CKS", "2018-01-01", Some("2020-01-01"), true),
        ("Terraform Associate", "2025-01-01", None, false),
    ] {
        sqlx::query(
            "INSERT INTO external_credentials
                (user_id, issuer, name, evidence_url, issued_on, expires_on,
                 verified_by, verified_at, verification_note)
             VALUES ($1, 'cncf', $2, 'https://example.com/c', $3::DATE, $4::DATE,
                     CASE WHEN $5 THEN $1 END,
                     CASE WHEN $5 THEN NOW() END,
                     CASE WHEN $5 THEN 'page ouverte et vérifiée' END)",
        )
        .bind(user_id)
        .bind(name)
        .bind(issued)
        .bind(expires)
        .bind(verified)
        .execute(&app.db)
        .await
        .unwrap();
    }

    let resp = app.get("/api/users/cred_scored/ops-profile").await;
    assert_eq!(resp.status(), 200);
    let jv: serde_json::Value = resp.json().await.unwrap();

    let credentials = jv["data"]["profile"]["credentials"].as_array().unwrap();
    assert_eq!(credentials.len(), 1, "only the verified, current one shows");

    let term = jv["data"]["profile"]["score"]["breakdown"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["term"] == "credentials_current")
        .expect("the term is in the breakdown");
    assert_eq!(term["measured"], 1.0);

    drop(app);
}

#[tokio::test]
async fn a_certification_is_worth_less_than_an_artefact() {
    let app = TestApp::spawn().await;

    // Not a matter of taste: an exam says somebody revised, a shipped module
    // says somebody built a thing another person now runs. If this ever
    // inverts, the platform is scoring the thing it exists to replace.
    let (credential, artefact): (i32, i32) = sqlx::query_as(
        "SELECT
             (SELECT weight FROM craft_score_weights
               WHERE skill_domain = 'ops' AND term = 'credentials_current')::INT,
             (SELECT weight FROM craft_score_weights
               WHERE skill_domain = 'ops' AND term = 'infra_artifacts_shipped')::INT",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert!(
        credential < artefact,
        "a certification ({credential}) must weigh less than a shipped artefact ({artefact})"
    );

    drop(app);
}
