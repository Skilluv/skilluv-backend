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

/// An absent AI worker is 503, not 500.
///
/// This asserted 500 until the derived sweep in
/// `tests/test_read_endpoints_answer.rs` reached the endpoint and found it
/// answering 500 to every signed-in caller. The distinction was already the
/// platform's position — four Stripe handlers were corrected for exactly this
/// in August — and the AI ones were missed because the sweep's list was
/// maintained by hand and did not include them.
///
/// It matters to a caller: 503 is retryable and says the deployment lacks an
/// integration, 500 says the server is broken and is worth a bug report.
#[tokio::test]
async fn performance_route_is_unavailable_without_an_ai_worker() {
    let app = TestApp::spawn().await;
    setup_user_with_passkey(&app, "perf_user").await;
    app.login("perf_user").await;
    // GRPC_AI_URL is unset in tests, so `state.ai` is None.
    let resp = app.get("/api/users/me/performance").await;
    assert_eq!(resp.status().as_u16(), 503);
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
async fn suggest_orientations_is_unavailable_without_an_ai_worker() {
    let app = TestApp::spawn().await;
    setup_user_with_passkey(&app, "sug_user").await;
    app.login("sug_user").await;
    let resp = app
        .post("/api/users/me/orientations/suggest", &json!({}))
        .await;
    assert_eq!(resp.status().as_u16(), 503);
}

#[tokio::test]
async fn suggest_orientations_refresh_rate_limited() {
    let app = TestApp::spawn().await;
    setup_user_with_passkey(&app, "rate_user").await;
    app.login("rate_user").await;
    // SKILLUV_DISABLE_RATELIMIT=1 in tests (see common/mod.rs) turns the rate
    // limit off, so the limit itself is not exercised here. What is checked is
    // that `refresh: true` is accepted and reaches the AI call — which is 503
    // because no worker is connected, and would have been 401 or 400 if the
    // parameter were being refused earlier.
    let r1 = app
        .post(
            "/api/users/me/orientations/suggest",
            &json!({"refresh": true}),
        )
        .await;
    assert_eq!(r1.status().as_u16(), 503);
}
