//! Tests ADM-M3.1 — CRUD admin sur orientations + orientation_skill_map.

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

async fn admin_post(app: &TestApp, path: &str, body: serde_json::Value) -> reqwest::Response {
    app.client
        .post(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn admin_patch(app: &TestApp, path: &str, body: serde_json::Value) -> reqwest::Response {
    app.client
        .patch(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn admin_delete(app: &TestApp, path: &str) -> reqwest::Response {
    app.client
        .delete(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn create_orientation_persists_row() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_1a").await;

    let resp = admin_post(
        &app,
        "/api/admin/orientations",
        json!({
            "slug": "dev-embedded",
            "name": "Développeur Embarqué",
            "description": "C/C++, temps réel, IoT.",
            "primary_domain": "code",
            "tags": ["embedded"],
            "is_curated": true,
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM orientations WHERE slug = 'dev-embedded' AND is_curated = TRUE)",
    )
    .fetch_one(&app.db).await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn create_orientation_rejects_bad_domain() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_1b").await;

    let resp = admin_post(
        &app,
        "/api/admin/orientations",
        json!({
            "slug": "test-bad",
            "name": "X",
            "primary_domain": "quantum",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn patch_orientation_rejects_slug_rename() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_1c").await;
    // Utilise une orientation seedée par mig 0088.
    let resp = admin_patch(
        &app,
        "/api/admin/orientations/dev-frontend",
        json!({
            "slug": "renamed-slug",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn patch_orientation_updates_name() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_1d").await;
    let resp = admin_patch(
        &app,
        "/api/admin/orientations/dev-frontend",
        json!({
            "name": "Front-End Ninja",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let name: String =
        sqlx::query_scalar("SELECT name FROM orientations WHERE slug='dev-frontend'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(name, "Front-End Ninja");
}

#[tokio::test]
async fn attach_skill_upserts_and_detach_is_idempotent() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m3_1e").await;

    // Récupère une skill existante seedée quelque part (ou insère-en une).
    let skill_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO skill_nodes (display_name, slug, domain) VALUES ('WebAssembly', 'wasm', 'code')
         ON CONFLICT (slug) DO UPDATE SET display_name = EXCLUDED.display_name RETURNING id",
    )
    .fetch_one(&app.db).await.unwrap();

    // Attach.
    let resp = admin_post(
        &app,
        "/api/admin/orientations/dev-frontend/skills",
        json!({
            "skill_id": skill_id.to_string(), "is_core": false, "weight": 0.5,
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);

    // Re-attach (upsert) → même endpoint OK.
    let resp2 = admin_post(
        &app,
        "/api/admin/orientations/dev-frontend/skills",
        json!({
            "skill_id": skill_id.to_string(), "is_core": true, "weight": 1.5,
        }),
    )
    .await;
    assert_eq!(resp2.status().as_u16(), 200);
    let is_core: bool =
        sqlx::query_scalar("SELECT is_core FROM orientation_skill_map WHERE skill_id = $1")
            .bind(skill_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(is_core);

    // Detach.
    let path = format!("/api/admin/orientations/dev-frontend/skills/{skill_id}");
    let resp3 = admin_delete(&app, &path).await;
    assert_eq!(resp3.status().as_u16(), 200);

    // Idempotent : re-detach = OK.
    let resp4 = admin_delete(&app, &path).await;
    assert_eq!(resp4.status().as_u16(), 200);
}
