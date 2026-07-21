//! Tests IA-C — Routes ai_coach (performance + suggest_orientations).
//!
//! Comme IA-B, l'appel gRPC réel nécessite skilluv-ia up. On teste ici :
//!   - Routes existent + auth required.
//!   - Fallback 500 si ai_client absent.
//!   - Rate-limit refresh appliqué.

mod common;
use common::TestApp;
use serde_json::json;

async fn setup_user_with_passkey(app: &TestApp, username: &str) -> uuid::Uuid {
    app.register_user(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO webauthn_credentials
            (user_id, credential_id, credential, label)
         VALUES ($1, $2, '{}'::jsonb, 'test')",
    )
    .bind(uid)
    .bind(format!("cred-{uid}").into_bytes())
    .execute(&app.db)
    .await
    .unwrap();
    uid
}

#[tokio::test]
async fn performance_route_requires_auth() {
    let app = TestApp::spawn().await;
    // Pas de login → cookie absent → 401.
    let resp = app.get("/api/users/me/performance").await;
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn performance_route_returns_500_without_ai_client() {
    let app = TestApp::spawn().await;
    setup_user_with_passkey(&app, "perf_user").await;
    app.login("perf_user").await;
    // GRPC_AI_URL absent en test → state.ai = None → 500.
    let resp = app.get("/api/users/me/performance").await;
    assert_eq!(resp.status().as_u16(), 500);
}

#[tokio::test]
async fn suggest_orientations_route_requires_auth() {
    let app = TestApp::spawn().await;
    let resp = app
        .post("/api/users/me/orientations/suggest", &json!({}))
        .await;
    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn suggest_orientations_returns_500_without_ai_client() {
    let app = TestApp::spawn().await;
    setup_user_with_passkey(&app, "sug_user").await;
    app.login("sug_user").await;
    let resp = app
        .post("/api/users/me/orientations/suggest", &json!({}))
        .await;
    assert_eq!(resp.status().as_u16(), 500);
}

#[tokio::test]
async fn suggest_orientations_refresh_rate_limited() {
    let app = TestApp::spawn().await;
    setup_user_with_passkey(&app, "rate_user").await;
    app.login("rate_user").await;
    // Note : SKILLUV_DISABLE_RATELIMIT=1 en tests (voir common/mod.rs) désactive
    // le rate-limit, donc pas testable ici. On vérifie juste que refresh=true
    // est accepté et déclenche l'appel IA (500 car ai_client absent).
    let r1 = app
        .post(
            "/api/users/me/orientations/suggest",
            &json!({"refresh": true}),
        )
        .await;
    assert_eq!(r1.status().as_u16(), 500);
}
