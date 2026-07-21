//! Tests ADM-M5+ — proof-hooks sweep + admin gdpr-export + rank-history.

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
async fn sweep_dry_run_returns_count_without_running_engine() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_sweep_a").await;
    let resp = admin_post(
        &app,
        "/api/admin/proof-hooks/sweep?within_days=30&dry_run=true",
        json!({}),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["dry_run"], true);
    assert_eq!(body["data"]["within_days"], 30);
    assert!(body["data"]["would_process_count"].is_number());
}

#[tokio::test]
async fn admin_gdpr_export_queues_and_audits() {
    let app = TestApp::spawn().await;
    let admin_uid = setup_admin(&app, "adm_gdpr_a").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_gdpr@t.com', 'target_gdpr', 'x', 'A', 'B', 'AB', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    let resp = admin_post(
        &app,
        &format!("/api/admin/users/{target_id}/gdpr-export"),
        json!({
            "reason": "user requested export via support ticket #1234",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["data"]["status"], "queued");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE action = 'user.admin_gdpr_export' AND actor_id = $1 AND target_id = $2",
    )
    .bind(admin_uid)
    .bind(target_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn admin_gdpr_export_rejects_short_reason() {
    let app = TestApp::spawn().await;
    setup_admin(&app, "adm_gdpr_b").await;
    let target_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain)
         VALUES ('target_gdpr_b@t.com', 'target_gdpr_b', 'x', 'A', 'B', 'AB', 'user', 'code')
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();

    let resp = admin_post(
        &app,
        &format!("/api/admin/users/{target_id}/gdpr-export"),
        json!({
            "reason": "x",
        }),
    )
    .await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn rank_history_returns_public_data_with_profile_active() {
    let app = TestApp::spawn().await;
    let uid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain, profile_active)
         VALUES ('rankhist@t.com', 'rankhist', 'x', 'A', 'B', 'AB', 'user', 'code', TRUE)
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
         VALUES ($1, 'apprenti', 'ranger', 'test seed')",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get(&format!("/api/users/{uid}/rank-history")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let hist = body["data"]["history"].as_array().unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0]["to_rank"], "ranger");
}

#[tokio::test]
async fn rank_history_returns_empty_for_inactive_profile() {
    let app = TestApp::spawn().await;
    let uid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, username, password_hash, first_name, last_name, display_name, role, skill_domain, profile_active)
         VALUES ('rankhist_hidden@t.com', 'rankhist_hidden', 'x', 'A', 'B', 'AB', 'user', 'code', FALSE)
         RETURNING id",
    ).fetch_one(&app.db).await.unwrap();
    sqlx::query(
        "INSERT INTO user_rank_history (user_id, from_rank, to_rank, reason)
         VALUES ($1, 'apprenti', 'artisan', 'seed')",
    )
    .bind(uid)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app.get(&format!("/api/users/{uid}/rank-history")).await;
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let hist = body["data"]["history"].as_array().unwrap();
    assert_eq!(hist.len(), 0);
}
