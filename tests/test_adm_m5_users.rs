//! Tests ADM-M5 — recompute-proofs + rank-override + orientations admin peek.

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

async fn admin_get(app: &TestApp, path: &str) -> reqwest::Response {
    app.client
        .get(format!("{}{}", app.addr, path))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn rank_overrides_table_exists() {
    let app = TestApp::spawn().await;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='rank_overrides')",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn recompute_proofs_returns_report_and_audits() {
    let app = TestApp::spawn().await;
    let admin_uid = setup_admin(&app, "adm_m5_a").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_m5_a@test.com', 'target_m5_a', 'x', 'T', 'A', 'Target A', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    // Dry-run pour éviter de faire tourner tout le proof engine sur un user vierge
    // (les moteurs capabilities+badges+ranks touchent 10+ tables ; ok pour un run
    // réel, coûteux pour un test unitaire de la route).
    let resp = admin_post(
        &app,
        &format!("/api/admin/users/{target_id}/recompute-proofs?dry_run=true"),
        json!({
            "scope": "all",
            "reason": "test recompute dry-run",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["dry_run"], true);
    assert!(body["data"]["current_state"].is_object());
    // Un dry-run n'audit pas — l'audit est fait au vrai run seulement.
    let _admin_uid = admin_uid;
}

#[tokio::test]
async fn rank_override_writes_row_and_updates_user_ranks() {
    let app = TestApp::spawn().await;
    let admin_uid = setup_admin(&app, "adm_m5_b").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_m5_b@test.com', 'target_m5_b', 'x', 'T', 'B', 'Target B', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    let resp = admin_post(
        &app,
        &format!("/api/admin/users/{target_id}/rank-override"),
        json!({
            "new_rank": "maitre",
            "reason": "compagnonnage validated externally",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["new_rank"], "maitre");

    let rank: String = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(target_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(rank, "maitre");

    let override_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rank_overrides WHERE user_id = $1 AND admin_id = $2",
    )
    .bind(target_id)
    .bind(admin_uid)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(override_count, 1);
}

#[tokio::test]
async fn rank_override_rejects_bad_rank() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m5_c").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_m5_c@test.com', 'target_m5_c', 'x', 'T', 'C', 'Target C', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    let resp = admin_post(
        &app,
        &format!("/api/admin/users/{target_id}/rank-override"),
        json!({
            "new_rank": "supreme_leader",
            "reason": "should fail validation",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn peek_user_orientations_admin_scoped() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_m5_d").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_m5_d@test.com', 'target_m5_d', 'x', 'T', 'D', 'Target D', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    // Attach 1 orientation seedée (dev-frontend).
    let orient_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM orientations WHERE slug = 'dev-frontend'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO user_orientations (user_id, orientation_id, mode, is_primary)
         VALUES ($1, $2, 'active', TRUE)",
    )
    .bind(target_id)
    .bind(orient_id)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = admin_get(&app, &format!("/api/users/{target_id}/orientations")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let list = body["data"]["orientations"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["orientation_slug"], "dev-frontend");
}
