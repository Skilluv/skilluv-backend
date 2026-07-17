//! Tests ADM-M3.2 — CRUD admin sur badge_rules.

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

async fn admin_post(app: &TestApp, path: &str, body: serde_json::Value) -> reqwest::Response {
    app.client
        .post(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .json(&body)
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
async fn create_rule_persists_row() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_2a").await;

    let resp = admin_post(&app, "/api/admin/badge-rules", json!({
        "slug": "test_rule_a",
        "output_type": "skill_patch",
        "display_name": "Test Rule A",
        "conditions": { "proof_types": ["deliverable_verified"], "min_count": 3 },
        "rarity": "common",
    })).await;
    assert_eq!(resp.status().as_u16(), 200);
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM badge_rules WHERE slug = 'test_rule_a')",
    )
    .fetch_one(&app.db).await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn create_rule_rejects_bad_output_type() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_2b").await;
    let resp = admin_post(&app, "/api/admin/badge-rules", json!({
        "slug": "test_rule_bad",
        "output_type": "hologram",
        "display_name": "X",
        "conditions": {},
    })).await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn patch_rule_rejects_when_admin_editable_false() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_2c").await;
    sqlx::query(
        "INSERT INTO badge_rules (slug, output_type, display_name, conditions, admin_editable)
         VALUES ('core_rule_x', 'rank', 'Core', '{}'::jsonb, FALSE)",
    ).execute(&app.db).await.unwrap();

    let resp = admin_patch(&app, "/api/admin/badge-rules/core_rule_x", json!({
        "display_name": "Hacked",
    })).await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn deprecate_rule_soft_deletes_and_is_idempotent() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_2d").await;
    sqlx::query(
        "INSERT INTO badge_rules (slug, output_type, display_name, conditions)
         VALUES ('to_deprecate_x', 'medal', 'Old Medal', '{}'::jsonb)",
    ).execute(&app.db).await.unwrap();

    let resp = admin_post(&app, "/api/admin/badge-rules/to_deprecate_x/deprecate", json!({
        "reason": "obsolete for MVP season",
    })).await;
    assert_eq!(resp.status().as_u16(), 200);
    let dep: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT deprecated_at FROM badge_rules WHERE slug = 'to_deprecate_x'",
    ).fetch_one(&app.db).await.unwrap();
    assert!(dep.is_some());

    // Idempotent : 2e call = 200.
    let resp2 = admin_post(&app, "/api/admin/badge-rules/to_deprecate_x/deprecate", json!({
        "reason": "obsolete for MVP season",
    })).await;
    assert_eq!(resp2.status().as_u16(), 200);
}
