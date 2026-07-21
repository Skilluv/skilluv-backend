//! Admin visibility for SSO sessions — list + revoke.

mod common;

use reqwest::StatusCode;
use serde_json::json;

async fn insert_sso_session(app: &common::TestApp, user_id: uuid::Uuid) -> uuid::Uuid {
    // The refresh_hash is a sha256 blob — any 32-byte value is fine for this
    // test since we don't rotate the token, we only inspect / revoke.
    let hash: Vec<u8> = vec![0u8; 32];
    let row: (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO user_sessions (user_id, refresh_hash, ip, user_agent, login_method)
         VALUES ($1, $2, $3, $4, 'sso')
         RETURNING id",
    )
    .bind(user_id)
    .bind(hash)
    .bind::<Option<&str>>(Some("10.0.0.1"))
    .bind::<Option<&str>>(Some("test-ua"))
    .fetch_one(&app.db)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
async fn test_admin_lists_sso_sessions() {
    let app = common::TestApp::spawn().await;

    // Owner + enterprise membership so the join populates the enterprise columns.
    app.register_enterprise("SsoAdminCorp").await;
    let owner_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM users WHERE username = 'ssoadmincorp'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let session_id = insert_sso_session(&app, owner_id.0).await;

    // Admin lists — sees the SSO session with enterprise info.
    app.register_admin("root").await;
    let resp = app.get("/api/admin/sso/sessions").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    let sessions = body["data"]["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["session_id"], session_id.to_string());
    assert_eq!(sessions[0]["enterprise_slug"], "ssoadmincorp");
    assert_eq!(sessions[0]["company_name"], "SsoAdminCorp");
    assert_eq!(sessions[0]["ip"], "10.0.0.1");
}

#[tokio::test]
async fn test_admin_revokes_sso_session() {
    let app = common::TestApp::spawn().await;
    app.register_enterprise("RevokeCorp").await;
    let owner_id: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM users WHERE username = 'revokecorp'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let session_id = insert_sso_session(&app, owner_id.0).await;

    app.register_admin("root2").await;
    let revoke = app
        .post(
            &format!("/api/admin/sso/sessions/{session_id}/revoke"),
            &json!({}),
        )
        .await;
    assert_eq!(revoke.status(), StatusCode::OK);

    let revoked_at: (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT revoked_at FROM user_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(revoked_at.0.is_some());

    // Listing again must not return the revoked session.
    let resp = app.get("/api/admin/sso/sessions").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["sessions"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_admin_endpoint_requires_admin_role() {
    let app = common::TestApp::spawn().await;
    app.register_user("regular").await;
    app.login("regular").await;
    let resp = app.get("/api/admin/sso/sessions").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
