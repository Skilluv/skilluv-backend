//! Tests P18.4 : routes API capabilities (public + admin grant/revoke).

mod common;
use common::TestApp;
use serde_json::json;

async fn grant_admin(app: &TestApp, uid: uuid::Uuid) {
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability, granted_reason)
         VALUES ($1, 'admin', 'test_setup') ON CONFLICT DO NOTHING",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn public_get_returns_active_capabilities() {
    let app = TestApp::spawn().await;
    app.register_user("kim184a").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kim184a'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'mentor')")
        .bind(uid)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app.get(&format!("/api/users/{uid}/capabilities")).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let caps: Vec<String> = body["data"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["capability"].as_str().unwrap().into())
        .collect();
    assert!(caps.contains(&"mentor".to_string()));
}

#[tokio::test]
async fn me_capabilities_requires_auth_and_returns_own() {
    let app = TestApp::spawn().await;
    app.register_user("kim184b").await;
    app.login("kim184b").await;
    let uid: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'kim184b'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'issue_proposer')",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get("/api/users/me/capabilities").await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["user_id"], uid.to_string());
    let caps: Vec<String> = body["data"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["capability"].as_str().unwrap().into())
        .collect();
    assert!(caps.contains(&"issue_proposer".to_string()));
}

#[tokio::test]
async fn admin_can_grant_capability_to_other_user() {
    let app = TestApp::spawn().await;
    app.register_user("adm184c").await;
    let adm_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'adm184c'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    grant_admin(&app, adm_id).await;

    app.register_user("target184c").await;
    let target_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'target184c'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Login admin
    app.login("adm184c").await;
    let resp = app
        .post(
            &format!("/api/admin/users/{target_id}/capabilities"),
            &json!({ "capability": "pr_reviewer" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 201);

    let has: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_capabilities
                         WHERE user_id = $1 AND capability = 'pr_reviewer' AND revoked_at IS NULL)",
    )
    .bind(target_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(has);
}

#[tokio::test]
async fn non_admin_cannot_grant() {
    let app = TestApp::spawn().await;
    app.register_user("normal184d").await;
    app.login("normal184d").await;
    app.register_user("target184d").await;
    let target_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'target184d'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    let resp = app
        .post(
            &format!("/api/admin/users/{target_id}/capabilities"),
            &json!({ "capability": "mentor" }),
        )
        .await;
    assert_eq!(resp.status().as_u16(), 403);
}

#[tokio::test]
async fn admin_can_revoke_active_capability() {
    let app = TestApp::spawn().await;
    app.register_user("adm184e").await;
    let adm_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'adm184e'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    grant_admin(&app, adm_id).await;

    app.register_user("target184e").await;
    let target_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'target184e'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query("INSERT INTO user_capabilities (user_id, capability) VALUES ($1, 'mentor')")
        .bind(target_id)
        .execute(&app.db)
        .await
        .unwrap();

    app.login("adm184e").await;
    let resp = app
        .delete(&format!("/api/admin/users/{target_id}/capabilities/mentor"))
        .await;
    assert_eq!(resp.status().as_u16(), 200);

    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_capabilities
                         WHERE user_id = $1 AND capability = 'mentor' AND revoked_at IS NULL)",
    )
    .bind(target_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(!active);
}

#[tokio::test]
async fn revoke_404_on_unknown_capability() {
    let app = TestApp::spawn().await;
    app.register_user("adm184f").await;
    let adm_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'adm184f'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    grant_admin(&app, adm_id).await;
    app.register_user("target184f").await;
    let target_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'target184f'")
            .fetch_one(&app.db)
            .await
            .unwrap();

    app.login("adm184f").await;
    let resp = app
        .delete(&format!("/api/admin/users/{target_id}/capabilities/mentor"))
        .await;
    assert_eq!(resp.status().as_u16(), 404);
}
