//! Tests des 2 endpoints ajoutés pour combler les zones d'ombre admin front :
//! - GET /admin/enterprises/{id}
//! - GET /admin/badge-events

mod common;
use common::TestApp;
use serde_json::json;

async fn setup_admin(app: &TestApp, username: &str) -> uuid::Uuid {
    app.register_admin(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(&format!(
        "SELECT id FROM users WHERE username = '{username}'"
    ))
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO webauthn_credentials (user_id, credential_id, credential, label)
         VALUES ($1, $2, '{\"stub\":true}'::jsonb, 'test-passkey')",
    )
    .bind(uid)
    .bind(format!("cred-{uid}").into_bytes())
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();
    app.login(username).await;
    uid
}

async fn admin_get(app: &TestApp, path: &str) -> reqwest::Response {
    app.client
        .get(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap()
}

async fn admin_post(app: &TestApp, path: &str, body: serde_json::Value) -> reqwest::Response {
    app.client
        .post(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn seed_enterprise(app: &TestApp, slug: &str, owner_prefix: &str) -> uuid::Uuid {
    let owner_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ($1, $2, 'x', 'O', 'M', 'Owner M', 'user', 'code') RETURNING id",
    )
    .bind(format!("{owner_prefix}@t.com"))
    .bind(owner_prefix)
    .fetch_one(&app.db).await.unwrap();
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size, verified, industry)
         VALUES ($1, $2, $3, '1-10', TRUE, 'Fintech') RETURNING id",
    )
    .bind(owner_id)
    .bind(slug)
    .bind(slug)
    .fetch_one(&app.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn get_admin_enterprise_by_id_returns_full_row() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_gap_a").await;
    let ent_id = seed_enterprise(&app, "gap-corp", "owner_gap_a").await;

    let resp = admin_get(&app, &format!("/api/admin/enterprises/{ent_id}")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let e = &body["data"]["enterprise"];
    assert_eq!(e["id"], ent_id.to_string());
    assert_eq!(e["company_name"], "gap-corp");
    assert_eq!(e["slug"], "gap-corp");
    assert_eq!(e["industry"], "Fintech");
    assert_eq!(e["verified"], true);
    assert_eq!(e["enterprise_type"], "direct_hire");
    assert!(e["type_config"].is_object());
    assert!(e["created_at"].is_string());
}

#[tokio::test]
async fn get_admin_enterprise_by_id_404_on_unknown() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_gap_b").await;
    let random = uuid::Uuid::new_v4();
    let resp = admin_get(&app, &format!("/api/admin/enterprises/{random}")).await;
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn get_admin_badge_events_returns_paginated_list() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_gap_c").await;

    // Seed 2 events (1 active partner + 1 inactive).
    admin_post(
        &app,
        "/api/admin/badge-events",
        json!({
            "slug": "hackfest-2027",
            "name": "Hackfest 2027",
            "starts_at": "2027-01-01T00:00:00Z",
            "is_partner": true,
        }),
    )
    .await;
    admin_post(
        &app,
        "/api/admin/badge-events",
        json!({
            "slug": "skilluv-fest-2027",
            "name": "Skilluv Fest 2027",
            "starts_at": "2027-06-01T00:00:00Z",
            "ends_at":   "2027-06-30T23:59:59Z",
            "is_partner": false,
        }),
    )
    .await;

    let resp = admin_get(&app, "/api/admin/badge-events?per_page=10").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let items = body["data"].as_array().unwrap();
    assert!(items.len() >= 2);
    assert_eq!(body["pagination"]["per_page"], 10);

    // Filtre is_partner=true.
    let resp2 = admin_get(&app, "/api/admin/badge-events?is_partner=true").await;
    assert_eq!(resp2.status().as_u16(), 200);
    let body2: serde_json::Value = resp2.json().await.unwrap();
    let partner_items = body2["data"].as_array().unwrap();
    assert!(partner_items.iter().all(|e| e["is_partner"] == true));
}
