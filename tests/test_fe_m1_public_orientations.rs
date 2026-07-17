//! Tests FE-M1 — GET /api/users/{id}/orientations (route publique).

mod common;
use common::TestApp;

async fn seed_user(app: &TestApp, username: &str, active: bool) -> uuid::Uuid {
    let uid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain, profile_active)
         VALUES ($1, $2, 'x', 'A', 'B', 'AB', 'user', 'code', $3) RETURNING id",
    )
    .bind(format!("{username}@t.com"))
    .bind(username)
    .bind(active)
    .fetch_one(&app.db).await.unwrap();
    uid
}

async fn attach_orientation(app: &TestApp, user_id: uuid::Uuid, slug: &str, primary: bool) {
    let oid: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM orientations WHERE slug = $1",
    ).bind(slug).fetch_one(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode, is_primary)
         VALUES ($1, $2, 'active', $3)",
    ).bind(user_id).bind(oid).bind(primary).execute(&app.db).await.unwrap();
}

#[tokio::test]
async fn public_orientations_returns_active_ones_for_public_profile() {
    let app = TestApp::spawn().await;
    let uid = seed_user(&app, "public_a", true).await;
    attach_orientation(&app, uid, "dev-frontend", true).await;

    let resp = app.get(&format!("/api/users/{uid}/orientations")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["orientation_slug"], "dev-frontend");
    assert_eq!(list[0]["is_primary"], true);
}

#[tokio::test]
async fn public_orientations_returns_empty_when_profile_inactive() {
    let app = TestApp::spawn().await;
    let uid = seed_user(&app, "hidden_a", false).await;
    attach_orientation(&app, uid, "dev-backend", true).await;

    let resp = app.get(&format!("/api/users/{uid}/orientations")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert_eq!(list.len(), 0, "profile inactive → empty (pas 403 pour éviter énumération)");
}

#[tokio::test]
async fn public_orientations_returns_404_for_unknown_user() {
    let app = TestApp::spawn().await;
    let random = uuid::Uuid::new_v4();
    let resp = app.get(&format!("/api/users/{random}/orientations")).await;
    assert_eq!(resp.status().as_u16(), 404);
}
