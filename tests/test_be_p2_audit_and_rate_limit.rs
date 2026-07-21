//! Tests BE-D + BE-E + BE-F — Rate-limit destructif, audit append-only, audit sur handlers.

mod common;
use common::TestApp;
use serde_json::json;

async fn setup_admin_with_passkey(app: &TestApp, username: &str) -> uuid::Uuid {
    app.register_admin(username).await;
    let uid: uuid::Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT id FROM users WHERE username = '{username}'"
    )))
    .fetch_one(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO webauthn_credentials
            (user_id, credential_id, credential, label)
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
    uid
}

// ═══════════════════════════════════════════════════════════════════
// BE-E — Append-only enforcement (rôle audit_admin)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn audit_admin_role_exists_after_migration_0099() {
    let app = TestApp::spawn().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname = 'audit_admin')")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(
        exists,
        "role audit_admin doit être créé par la migration 0099"
    );
}

#[tokio::test]
async fn audit_log_table_has_revoke_documented_via_comment() {
    let app = TestApp::spawn().await;
    let comment: Option<String> =
        sqlx::query_scalar("SELECT obj_description('admin_audit_log'::regclass, 'pg_class')")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let text = comment.unwrap_or_default();
    assert!(
        text.contains("Append-only"),
        "comment doit documenter le contrat append-only"
    );
}

// ═══════════════════════════════════════════════════════════════════
// BE-F — Audit sur handlers admin sensibles
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn kyc_decide_writes_audit_log() {
    let app = TestApp::spawn().await;
    let admin_uid = setup_admin_with_passkey(&app, "kyc_admin").await;

    // Setup un enterprise + une KYC en pending.
    app.register_enterprise("acmecorp").await;
    let ent_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM enterprises WHERE company_name = 'acmecorp'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO enterprise_kyc (enterprise_id, status)
         VALUES ($1, 'pending') ON CONFLICT DO NOTHING",
    )
    .bind(ent_id)
    .execute(&app.db)
    .await
    .unwrap();

    // Login admin puis approve KYC.
    app.login("kyc_admin").await;
    let resp = app
        .client
        .post(format!(
            "{}/api/admin/enterprise-kyc/{}/decide",
            app.addr, ent_id
        ))
        .header("origin", "http://localhost:5174")
        .json(&json!({ "action": "approve", "level": "basic" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    // Vérifie audit log écrit.
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE action = 'kyc_decide' AND actor_id = $1 AND target_id = $2",
    )
    .bind(admin_uid)
    .bind(ent_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
async fn sso_revoke_writes_audit_log() {
    let app = TestApp::spawn().await;
    let admin_uid = setup_admin_with_passkey(&app, "sso_admin").await;

    // Setup une user_session SSO à révoquer.
    app.register_user("victim_sso").await;
    let victim_uid: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM users WHERE username = 'victim_sso'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    let session_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO user_sessions (user_id, refresh_hash, login_method)
         VALUES ($1, 'stub_hash'::BYTEA, 'sso') RETURNING id",
    )
    .bind(victim_uid)
    .fetch_one(&app.db)
    .await
    .unwrap();

    app.login("sso_admin").await;
    let resp = app
        .client
        .post(format!(
            "{}/api/admin/sso/sessions/{}/revoke",
            app.addr, session_id
        ))
        .header("origin", "http://localhost:5174")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE action = 'sso_session_revoke' AND actor_id = $1 AND target_id = $2",
    )
    .bind(admin_uid)
    .bind(session_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

// ═══════════════════════════════════════════════════════════════════
// BE-D — Rate-limit destructif + dry-run
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn dry_run_env_is_readable_and_default_false() {
    // Test purement fonctionnel du helper — sans écrire de DB.
    let was = std::env::var("SKILLUV_ADMIN_DRY_RUN").ok();
    // SAFETY: single-threaded env access in a self-contained test.
    unsafe {
        std::env::remove_var("SKILLUV_ADMIN_DRY_RUN");
    }
    assert!(!skilluv_backend::middleware::admin_destructive::is_admin_dry_run());
    unsafe {
        std::env::set_var("SKILLUV_ADMIN_DRY_RUN", "1");
    }
    assert!(skilluv_backend::middleware::admin_destructive::is_admin_dry_run());
    // Restore.
    match was {
        Some(v) => unsafe {
            std::env::set_var("SKILLUV_ADMIN_DRY_RUN", v);
        },
        None => unsafe {
            std::env::remove_var("SKILLUV_ADMIN_DRY_RUN");
        },
    }
}
