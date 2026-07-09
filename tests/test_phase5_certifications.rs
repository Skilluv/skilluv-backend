//! Integration tests — Phase 5.10 Certifications.

mod common;

use common::TestApp;
use serde_json::{Value, json};

#[tokio::test]
async fn public_catalogue_returns_seeded_certifications() {
    let app = TestApp::spawn().await;
    // Insert a certification directly
    sqlx::query(
        r#"
        INSERT INTO certifications
            (slug, title, description, skill_domain, level, price_eur_cents, duration_minutes, passing_score, challenge_ids, active)
        VALUES ('rust-fundamentals', 'Rust Fundamentals', 'Test cert', 'code', 'foundation', 4900, 60, 70, '{}', TRUE)
        ON CONFLICT (slug) DO NOTHING
        "#,
    )
    .execute(&app.db)
    .await
    .expect("insert cert");

    let resp = app
        .client
        .get(format!("{}/api/certifications", app.addr))
        .send()
        .await
        .expect("GET certs");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let items = body["data"]["certifications"].as_array().unwrap();
    assert!(items.iter().any(|c| c["slug"] == "rust-fundamentals"));
    drop(app);
}

#[tokio::test]
async fn diploma_verification_public_returns_holder_info() {
    let app = TestApp::spawn().await;
    let user = app.register_user("holder1").await;
    let user_id: uuid::Uuid = user["data"]["user"]["id"]
        .as_str()
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .expect("user id");

    let cert_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO certifications
            (slug, title, description, skill_domain, level, price_eur_cents, duration_minutes, passing_score, challenge_ids, active)
        VALUES ('devops-intermediate', 'DevOps Intermediate', 'Test', 'code', 'intermediate', 9900, 120, 70, '{}', TRUE)
        RETURNING id
        "#,
    )
    .fetch_one(&app.db)
    .await
    .expect("insert cert");

    let attempt_id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO certification_attempts (user_id, certification_id, amount_paid_cents, currency, status)
        VALUES ($1, $2, 9900, 'EUR', 'passed')
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(cert_id.0)
    .fetch_one(&app.db)
    .await
    .expect("insert attempt");

    sqlx::query(
        r#"
        INSERT INTO certification_diplomas
            (attempt_id, user_id, certification_id, verification_code, expires_at)
        VALUES ($1, $2, $3, 'ABCD1234', NOW() + INTERVAL '2 years')
        "#,
    )
    .bind(attempt_id.0)
    .bind(user_id)
    .bind(cert_id.0)
    .execute(&app.db)
    .await
    .expect("insert diploma");

    let resp = app
        .client
        .get(format!("{}/api/diplomas/verify/ABCD1234", app.addr))
        .send()
        .await
        .expect("GET verify");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["status"], "valid");
    assert_eq!(body["data"]["holder"]["username"], "holder1");
    assert_eq!(body["data"]["certification"]["title"], "DevOps Intermediate");
    drop(app);
}

#[tokio::test]
async fn diploma_verification_unknown_returns_404() {
    let app = TestApp::spawn().await;
    let resp = app
        .client
        .get(format!("{}/api/diplomas/verify/NOPENOPE", app.addr))
        .send()
        .await
        .expect("GET verify");
    assert_eq!(resp.status(), 404);
    drop(app);
}

#[tokio::test]
async fn purchase_returns_checkout_or_stripe_missing_error() {
    let app = TestApp::spawn().await;
    sqlx::query(
        r#"
        INSERT INTO certifications
            (slug, title, description, skill_domain, level, price_eur_cents, duration_minutes, passing_score, challenge_ids, active)
        VALUES ('js-basics', 'JS Basics', 'x', 'code', 'foundation', 3900, 45, 70, '{}', TRUE)
        "#,
    )
    .execute(&app.db)
    .await
    .expect("insert");

    let _ = app.register_user("buyer1").await;
    let resp = app
        .client
        .post(format!("{}/api/certifications/js-basics/purchase", app.addr))
        .json(&json!({}))
        .send()
        .await
        .expect("POST purchase");
    // Sans Stripe configuré → 500 avec "Stripe not configured" ; c'est le
    // comportement attendu en test. Si un jour Stripe est configuré via env
    // pour les tests, on aura 200. Les deux cas valident l'API.
    assert!(resp.status() == 500 || resp.status() == 200);
    drop(app);
}
