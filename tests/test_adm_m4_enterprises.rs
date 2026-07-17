//! Tests ADM-M4 — Enterprise type manager.

mod common;
use common::TestApp;
use serde_json::json;

async fn setup_admin(app: &TestApp, username: &str) {
    app.register_admin(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(&format!(
        "SELECT id FROM users WHERE username = '{username}'"
    ))
    .fetch_one(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO webauthn_credentials (user_id, credential_id, credential, label)
         VALUES ($1, $2, '{\"stub\":true}'::jsonb, 'test-passkey')",
    )
    .bind(uid).bind(format!("cred-{uid}").into_bytes())
    .execute(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test') ON CONFLICT DO NOTHING",
    )
    .bind(uid).execute(&app.db).await.unwrap();
    app.login(username).await;
}

/// Insère un enterprise + owner directement en DB (sans cookies) pour ne
/// pas casser la session admin en cours dans le client.
async fn seed_enterprise(app: &TestApp, slug: &str, owner_email_prefix: &str) -> uuid::Uuid {
    let owner_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ($1, $2, 'stubhash', 'Owner', 'M4', 'Owner M4', 'user', 'code')
         RETURNING id",
    )
    .bind(format!("{owner_email_prefix}@test.com"))
    .bind(owner_email_prefix)
    .fetch_one(&app.db).await.unwrap();
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO enterprises (owner_id, company_name, slug, company_size, verified)
         VALUES ($1, $2, $3, '1-10', TRUE) RETURNING id",
    )
    .bind(owner_id).bind(slug).bind(slug)
    .fetch_one(&app.db).await.unwrap()
}

async fn admin_get(app: &TestApp, path: &str) -> reqwest::Response {
    app.client
        .get(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .send().await.unwrap()
}

async fn admin_patch(app: &TestApp, path: &str, body: serde_json::Value) -> reqwest::Response {
    app.client
        .patch(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .json(&body)
        .send().await.unwrap()
}

#[tokio::test]
async fn list_enterprises_returns_paginated() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m4_a").await;
    seed_enterprise(&app, "acme-m4a", "owner_m4a").await;

    let resp = admin_get(&app, "/api/admin/enterprises?per_page=5").await;
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or_else(|_| serde_json::json!({}));
    assert_eq!(status, 200, "body={body}");
    assert!(body["data"].is_array());
    assert_eq!(body["pagination"]["per_page"], 5);
}

#[tokio::test]
async fn patch_type_changes_and_resets_config() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m4_b").await;
    let ent_id = seed_enterprise(&app, "beta-m4b", "owner_m4b").await;
    // Simule un type_config existant.
    sqlx::query("UPDATE enterprises SET type_config = '{\"foo\":\"bar\"}'::jsonb WHERE id = $1")
        .bind(ent_id).execute(&app.db).await.unwrap();

    let resp = admin_patch(&app, &format!("/api/admin/enterprises/{ent_id}/type"), json!({
        "enterprise_type": "remote_international",
        "reason": "expansion Europe request from client",
    })).await;
    assert_eq!(resp.status().as_u16(), 200);

    let (etype, conf): (String, serde_json::Value) = sqlx::query_as(
        "SELECT enterprise_type, type_config FROM enterprises WHERE id = $1",
    ).bind(ent_id).fetch_one(&app.db).await.unwrap();
    assert_eq!(etype, "remote_international");
    assert_eq!(conf, json!({}), "type_config reset on type change");
}

#[tokio::test]
async fn get_type_config_returns_current() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m4_c").await;
    let ent_id = seed_enterprise(&app, "gamma-m4c", "owner_m4c").await;

    let resp = admin_get(&app, &format!("/api/admin/enterprises/{ent_id}/type-config")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["enterprise_type"], "direct_hire");
}

#[tokio::test]
async fn agency_clients_returns_empty_for_direct_hire() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m4_d").await;
    let ent_id = seed_enterprise(&app, "delta-m4d", "owner_m4d").await;

    let resp = admin_get(&app, &format!("/api/admin/enterprises/{ent_id}/agency-clients")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["clients"].as_array().unwrap().len(), 0);
}
