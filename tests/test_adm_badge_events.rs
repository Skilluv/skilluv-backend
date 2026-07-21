//! Tests ADM — POST /admin/badge-events (MVP.md Annexe A #8).

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

#[tokio::test]
async fn create_event_persists_row() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_ev_a").await;

    let resp = admin_post(
        &app,
        "/api/admin/badge-events",
        json!({
            "slug": "hacktoberfest-2026",
            "name": "Hacktoberfest 2026",
            "starts_at": "2026-10-01T00:00:00Z",
            "ends_at": "2026-10-31T23:59:59Z",
            "visual_theme": { "color": "#ff6b35" },
            "is_partner": false,
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["event"]["slug"], "hacktoberfest-2026");

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE slug = 'hacktoberfest-2026')")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn create_event_rejects_bad_slug() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_ev_b").await;
    let resp = admin_post(
        &app,
        "/api/admin/badge-events",
        json!({
            "slug": "Bad Slug With Spaces",
            "name": "X",
            "starts_at": "2026-10-01T00:00:00Z",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn create_event_rejects_ends_before_starts() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_ev_c").await;
    let resp = admin_post(
        &app,
        "/api/admin/badge-events",
        json!({
            "slug": "bad-window",
            "name": "Bad window",
            "starts_at": "2026-10-31T00:00:00Z",
            "ends_at":   "2026-10-01T00:00:00Z",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}
