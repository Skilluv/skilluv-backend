//! Tests final batch — recompute-capabilities + skills CRUD.

mod common;
use common::TestApp;
use serde_json::json;

async fn setup_admin(app: &TestApp, username: &str) -> uuid::Uuid {
    app.register_admin(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
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
async fn admin_put(app: &TestApp, path: &str, body: serde_json::Value) -> reqwest::Response {
    app.client
        .put(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn recompute_capabilities_returns_report_and_audits() {
    let app = TestApp::spawn().await;
    let admin_uid = setup_admin(&app, "adm_recap_a").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_recap@t.com', 'target_recap', 'x', 'A', 'B', 'AB', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    let resp = admin_post(
        &app,
        &format!("/api/admin/users/{target_id}/recompute-capabilities"),
        json!({}),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"]["granted"].is_array());

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE action = 'user.recompute_capabilities' AND actor_id = $1 AND target_id = $2",
    )
    .bind(admin_uid)
    .bind(target_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn skills_list_paginated() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_sk_a").await;
    let resp = admin_get(&app, "/api/admin/skills?domain=code&per_page=5").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
    assert_eq!(body["pagination"]["per_page"], 5);
}

#[tokio::test]
async fn skills_create_persists_row() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_sk_b").await;
    let slug = format!(
        "test-sk-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let resp = admin_post(
        &app,
        "/api/admin/skills",
        json!({
            "slug": slug,
            "display_name": "Tokio select! macro",
            "domain": "code",
            "aliases": ["tokio-select"],
            "is_skilluv_specific": false,
        }),
    )
    .await;
    let st = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(st, 200, "body={body}");
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM skill_nodes WHERE slug = $1)")
            .bind(&slug)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn skills_create_rejects_bad_domain() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_sk_c").await;
    let resp = admin_post(
        &app,
        "/api/admin/skills",
        json!({
            "slug": "test-bad",
            "display_name": "X",
            "domain": "biology",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn skills_update_edits_fields() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_sk_d").await;
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO skill_nodes (slug, display_name, domain)
         VALUES ('to-update-sk', 'Old Name', 'code') RETURNING id",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();

    let resp = admin_put(
        &app,
        &format!("/api/admin/skills/{id}"),
        json!({
            "display_name": "New Name",
            "is_skilluv_specific": true,
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let (name, specific): (String, bool) =
        sqlx::query_as("SELECT display_name, is_skilluv_specific FROM skill_nodes WHERE id = $1")
            .bind(id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(name, "New Name");
    assert!(specific);
}
